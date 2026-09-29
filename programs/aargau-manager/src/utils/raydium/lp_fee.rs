//! LP-fee measurement for Raydium CLMM fee collection.
//!
//! Raydium pays LP fees and rewards in the same `decrease_liquidity_v2` call,
//! and a reward mint may equal a pair mint, so the vault ATA's balance change
//! cannot tell them apart. LP fees are the only transfers out of the pool's
//! `token_vault_0/1` in a zero-liquidity decrease, while rewards leave from
//! separate reward vaults. The performance fee is therefore based on the pool
//! vault outflow, converted into what the vault actually received after any
//! Token-2022 transfer fee, and never on rewards.
//!
//! Pure functions only: the handler reads the balances and the live
//! transfer-fee config and passes them in.

use anchor_lang::prelude::*;

use crate::errors::AargauError;
use crate::utils::fee::{calc_aargau_fee, calc_net_after_transfer_fee};
use crate::utils::token_2022::TransferFeeSnapshot;

/// Balances around the zero-liquidity decrease, for one side of the pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LpFeeLegBalances {
    pub pool_vault_before: u64,
    pub pool_vault_after: u64,
    pub vault_ata_before: u64,
    pub vault_ata_after: u64,
}

/// Gross amount that left the pool vault. A pool vault that grew during a
/// decrease is inconsistent with a fee payout (`LpFeeMeasurementMismatch`).
pub fn pool_vault_outflow(before: u64, after: u64) -> Result<u64> {
    before
        .checked_sub(after)
        .ok_or(error!(AargauError::LpFeeMeasurementMismatch))
}

/// Amount the recipient receives from a `transfer_checked` of
/// `pool_vault_outflow`, given the transfer fee Token-2022 applies at the
/// current epoch (`None` for SPL classic and fee-less mints). Token-2022 rounds
/// the fee up, so this rounds the received amount down.
pub fn lp_fee_received(pool_vault_outflow: u64, transfer_fee: Option<TransferFeeSnapshot>) -> u64 {
    match transfer_fee {
        Some(fee) => {
            calc_net_after_transfer_fee(pool_vault_outflow, fee.transfer_fee_bps, fee.maximum_fee)
        }
        None => pool_vault_outflow,
    }
}

/// Measure the LP fee the vault received on one side of the pair and bound it
/// by the vault ATA's own balance increase, which also holds any reward paid
/// in the same mint. Returns the received LP fee — the performance-fee base.
///
/// Errors:
/// - `LpFeeMeasurementMismatch` if the pool vault grew, or if the computed
///   received fee exceeds what the vault ATA actually gained;
/// - `PostCpiBalanceDecreased` if the vault ATA shrank.
pub fn measure_lp_fee_received(
    balances: &LpFeeLegBalances,
    transfer_fee: Option<TransferFeeSnapshot>,
) -> Result<u64> {
    let outflow = pool_vault_outflow(balances.pool_vault_before, balances.pool_vault_after)?;
    let vault_ata_increase = balances
        .vault_ata_after
        .checked_sub(balances.vault_ata_before)
        .ok_or(error!(AargauError::PostCpiBalanceDecreased))?;
    let received = lp_fee_received(outflow, transfer_fee);
    require!(
        received <= vault_ata_increase,
        AargauError::LpFeeMeasurementMismatch
    );
    Ok(received)
}

/// One LP-fee leg split between the treasury and the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LpFeeSplit {
    pub gross: u64,
    pub aargau_fee: u64,
    pub user_net: u64,
}

/// Split a received LP fee with `calc_aargau_fee` (rounds down, user favored).
pub fn split_lp_fee(gross: u64, fee_rate_bps: u16) -> Result<LpFeeSplit> {
    let aargau_fee = calc_aargau_fee(gross, fee_rate_bps)?;
    let user_net = gross
        .checked_sub(aargau_fee)
        .ok_or(error!(AargauError::FeeExceedsGross))?;
    Ok(LpFeeSplit {
        gross,
        aargau_fee,
        user_net,
    })
}
