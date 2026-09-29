//! Vault-custodied Raydium CLMM position operations, shared by every
//! instruction that manages a Raydium position (`execute_action_raydium`,
//! `start_rebalance_raydium`, `retry_pending_rebalance_raydium`).
//!
//! Each operation wraps one builder from this module's siblings with the
//! custodial checks that must travel with it: the performance-fee split on
//! collected LP fees, the observed min-out on principal and the observed
//! max-in on deposits.
//!
//! The operations take an explicit bundle of `AccountInfo`s and bind nothing:
//! the calling handler must already have bound the pool, position, NFT, pool
//! vaults, mints, tick arrays, bitmap extension, reward triples and treasury
//! ATAs. Vault ATA balances are read from account data before and after every
//! CPI (the `AccountInfo` form of `reload()`); a caller that keeps a typed
//! `InterfaceAccount` for those ATAs must `reload()` it before trusting its
//! cached amount.

use anchor_lang::prelude::*;

use crate::events::FeesClaimed;
use crate::utils::token_2022::{read_epoch_transfer_fee, TransferFeeSnapshot};
use crate::utils::token_account::read_token_account_view;
use crate::utils::vault_ops::transfer_performance_fee;

use super::close_position::{invoke_close_position, ClosePositionCpi};
use super::decrease_liquidity::{invoke_decrease_liquidity_v2, DecreaseLiquidityV2Cpi};
use super::increase_liquidity::{invoke_increase_liquidity_v2, IncreaseLiquidityV2Cpi};
use super::lp_fee::{measure_lp_fee_received, split_lp_fee, LpFeeLegBalances, LpFeeSplit};
use super::open_position::{invoke_open_position_with_token22_nft, OpenPositionWithToken22NftCpi};
use super::position_guards::{
    observed_credit, observed_debit, require_consumed_within_max, require_principal_meets_min,
    require_raydium_increase_bounds,
};
use super::reward_accounts::RewardTransferAccounts;

/// A zero-liquidity `decrease_liquidity_v2` only pays out fees and rewards
/// owed. Raydium skips its slippage check when `liquidity == 0` and no
/// principal moves, so zero minimums disable nothing here.
const COLLECT_ONLY_LIQUIDITY: u128 = 0;
const COLLECT_ONLY_MIN_AMOUNT: u64 = 0;

/// One side of the pool pair, as the vault sees it.
#[derive(Clone)]
pub struct PairLegAccounts<'info> {
    /// Pool mint (`pool.token_mint_0/1`); its owner is the leg's token program.
    pub mint: AccountInfo<'info>,
    pub decimals: u8,
    /// The vault PDA's token account for `mint`.
    pub vault_ata: AccountInfo<'info>,
    /// Pool token vault (`pool.token_vault_0/1`), the LP-fee measurement base.
    pub pool_vault: AccountInfo<'info>,
}

/// Accounts every CPI on the vault's position needs. Leg 0 is the vault's
/// token A side, leg 1 its token B side.
#[derive(Clone)]
pub struct RaydiumPositionAccounts<'info> {
    pub clmm_program: AccountInfo<'info>,
    /// Vault PDA: owns the NFT and signs every CPI except the open.
    pub vault: AccountInfo<'info>,
    pub pool_state: AccountInfo<'info>,
    pub personal_position: AccountInfo<'info>,
    pub position_nft_mint: AccountInfo<'info>,
    pub position_nft_account: AccountInfo<'info>,
    pub tick_array_lower: AccountInfo<'info>,
    pub tick_array_upper: AccountInfo<'info>,
    pub tick_array_bitmap_extension: AccountInfo<'info>,
    pub token_program: AccountInfo<'info>,
    pub token_program_2022: AccountInfo<'info>,
    pub leg_0: PairLegAccounts<'info>,
    pub leg_1: PairLegAccounts<'info>,
}

/// Extra accounts the empty open needs: the user pays every account Raydium
/// creates.
pub struct OpenPositionFundingAccounts<'info> {
    pub payer: AccountInfo<'info>,
    pub rent: AccountInfo<'info>,
    pub system_program: AccountInfo<'info>,
    pub associated_token_program: AccountInfo<'info>,
}

/// Extra accounts every `decrease_liquidity_v2` needs: the memo slot and one
/// bound reward triple per initialized pool reward slot.
pub struct PositionPayoutAccounts<'a, 'info> {
    pub memo_program: AccountInfo<'info>,
    pub rewards: &'a [RewardTransferAccounts<'info>],
}

/// Treasury side of the performance-fee split.
pub struct TreasuryFeeAccounts<'info> {
    /// Treasury PDA's token account for leg 0's mint.
    pub treasury_ata_0: AccountInfo<'info>,
    /// Treasury PDA's token account for leg 1's mint.
    pub treasury_ata_1: AccountInfo<'info>,
    pub fee_rate_bps: u16,
}

/// Token amounts observed on the two vault ATAs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PairAmounts {
    pub amount_0: u64,
    pub amount_1: u64,
}

impl<'info> RaydiumPositionAccounts<'info> {
    pub fn open_cpi(
        &self,
        funding: &OpenPositionFundingAccounts<'info>,
    ) -> OpenPositionWithToken22NftCpi<'info> {
        OpenPositionWithToken22NftCpi {
            clmm_program: self.clmm_program.clone(),
            payer: funding.payer.clone(),
            vault: self.vault.clone(),
            position_nft_mint: self.position_nft_mint.clone(),
            position_nft_account: self.position_nft_account.clone(),
            pool_state: self.pool_state.clone(),
            tick_array_lower: self.tick_array_lower.clone(),
            tick_array_upper: self.tick_array_upper.clone(),
            personal_position: self.personal_position.clone(),
            token_account_0: self.leg_0.vault_ata.clone(),
            token_account_1: self.leg_1.vault_ata.clone(),
            token_vault_0: self.leg_0.pool_vault.clone(),
            token_vault_1: self.leg_1.pool_vault.clone(),
            rent: funding.rent.clone(),
            system_program: funding.system_program.clone(),
            token_program: self.token_program.clone(),
            associated_token_program: funding.associated_token_program.clone(),
            token_program_2022: self.token_program_2022.clone(),
            vault_0_mint: self.leg_0.mint.clone(),
            vault_1_mint: self.leg_1.mint.clone(),
            tick_array_bitmap_extension: self.tick_array_bitmap_extension.clone(),
        }
    }

    pub fn increase_cpi(&self) -> IncreaseLiquidityV2Cpi<'info> {
        IncreaseLiquidityV2Cpi {
            clmm_program: self.clmm_program.clone(),
            vault: self.vault.clone(),
            nft_account: self.position_nft_account.clone(),
            pool_state: self.pool_state.clone(),
            personal_position: self.personal_position.clone(),
            tick_array_lower: self.tick_array_lower.clone(),
            tick_array_upper: self.tick_array_upper.clone(),
            vault_token_0: self.leg_0.vault_ata.clone(),
            vault_token_1: self.leg_1.vault_ata.clone(),
            token_vault_0: self.leg_0.pool_vault.clone(),
            token_vault_1: self.leg_1.pool_vault.clone(),
            token_program: self.token_program.clone(),
            token_program_2022: self.token_program_2022.clone(),
            vault_0_mint: self.leg_0.mint.clone(),
            vault_1_mint: self.leg_1.mint.clone(),
            tick_array_bitmap_extension: self.tick_array_bitmap_extension.clone(),
        }
    }

    pub fn decrease_cpi<'a>(
        &self,
        payout: &PositionPayoutAccounts<'a, 'info>,
    ) -> DecreaseLiquidityV2Cpi<'a, 'info> {
        DecreaseLiquidityV2Cpi {
            clmm_program: self.clmm_program.clone(),
            vault: self.vault.clone(),
            nft_account: self.position_nft_account.clone(),
            personal_position: self.personal_position.clone(),
            pool_state: self.pool_state.clone(),
            token_vault_0: self.leg_0.pool_vault.clone(),
            token_vault_1: self.leg_1.pool_vault.clone(),
            tick_array_lower: self.tick_array_lower.clone(),
            tick_array_upper: self.tick_array_upper.clone(),
            vault_token_0: self.leg_0.vault_ata.clone(),
            vault_token_1: self.leg_1.vault_ata.clone(),
            token_program: self.token_program.clone(),
            token_program_2022: self.token_program_2022.clone(),
            memo_program: payout.memo_program.clone(),
            vault_0_mint: self.leg_0.mint.clone(),
            vault_1_mint: self.leg_1.mint.clone(),
            tick_array_bitmap_extension: self.tick_array_bitmap_extension.clone(),
            reward_accounts: payout.rewards,
        }
    }

    pub fn close_cpi(&self, system_program: &AccountInfo<'info>) -> ClosePositionCpi<'info> {
        ClosePositionCpi {
            clmm_program: self.clmm_program.clone(),
            vault: self.vault.clone(),
            position_nft_mint: self.position_nft_mint.clone(),
            position_nft_account: self.position_nft_account.clone(),
            personal_position: self.personal_position.clone(),
            system_program: system_program.clone(),
            token_program_2022: self.token_program_2022.clone(),
            pool_state: self.pool_state.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------

/// Open an empty position over `[tick_lower, tick_upper]`, NFT minted to the
/// vault PDA, rent paid by the user. No tokens move.
pub fn open_empty_position<'info>(
    position: &RaydiumPositionAccounts<'info>,
    funding: &OpenPositionFundingAccounts<'info>,
    tick_lower: i32,
    tick_upper: i32,
    tick_spacing: u16,
) -> Result<()> {
    invoke_open_position_with_token22_nft(
        &position.open_cpi(funding),
        tick_lower,
        tick_upper,
        tick_spacing,
    )
}

/// Add `liquidity` from the vault ATAs, spending at most `token_max_0/1`.
/// Rejects maxima above the vault balance up front and re-checks the observed
/// debit after the CPI, so the bound never depends on Raydium alone. Returns
/// the amounts that left the vault.
pub fn increase_liquidity_from_vault(
    position: &RaydiumPositionAccounts<'_>,
    liquidity: u128,
    token_max_0: u64,
    token_max_1: u64,
    vault_signer_seeds: &[&[u8]],
) -> Result<PairAmounts> {
    let before = read_vault_balances(position)?;
    require_raydium_increase_bounds(
        liquidity,
        token_max_0,
        token_max_1,
        before.amount_0,
        before.amount_1,
    )?;

    invoke_increase_liquidity_v2(
        &position.increase_cpi(),
        liquidity,
        token_max_0,
        token_max_1,
        vault_signer_seeds,
    )?;

    let after = read_vault_balances(position)?;
    let consumed = PairAmounts {
        amount_0: observed_debit(before.amount_0, after.amount_0)?,
        amount_1: observed_debit(before.amount_1, after.amount_1)?,
    };
    // Raydium enforces the maxima against the gross debit; re-check the
    // observed debit as defence in depth.
    require_consumed_within_max(consumed.amount_0, token_max_0)?;
    require_consumed_within_max(consumed.amount_1, token_max_1)?;
    Ok(consumed)
}

/// Collect every fee and reward owed with a zero-liquidity decrease, then send
/// the performance fee on the LP-fee part only to the treasury and emit
/// `FeesClaimed`.
///
/// The fee base is the pool-vault outflow converted to what the vault
/// received, never the vault ATA delta: rewards leave from separate reward
/// vaults and stay untaxed even when a reward mint equals a pair mint.
pub fn collect_lp_fees_to_treasury<'info>(
    position: &RaydiumPositionAccounts<'info>,
    payout: &PositionPayoutAccounts<'_, 'info>,
    treasury: &TreasuryFeeAccounts<'info>,
    vault_signer_seeds: &[&[u8]],
) -> Result<()> {
    let pool_vault_before_0 = read_token_amount(&position.leg_0.pool_vault)?;
    let pool_vault_before_1 = read_token_amount(&position.leg_1.pool_vault)?;
    let vault_ata_before = read_vault_balances(position)?;

    invoke_decrease_liquidity_v2(
        &position.decrease_cpi(payout),
        COLLECT_ONLY_LIQUIDITY,
        COLLECT_ONLY_MIN_AMOUNT,
        COLLECT_ONLY_MIN_AMOUNT,
        vault_signer_seeds,
    )?;

    let vault_ata_after = read_vault_balances(position)?;
    let epoch = Clock::get()?.epoch;
    let lp_fee_0 = measure_lp_fee_received(
        &LpFeeLegBalances {
            pool_vault_before: pool_vault_before_0,
            pool_vault_after: read_token_amount(&position.leg_0.pool_vault)?,
            vault_ata_before: vault_ata_before.amount_0,
            vault_ata_after: vault_ata_after.amount_0,
        },
        read_live_transfer_fee(&position.leg_0.mint, epoch)?,
    )?;
    let lp_fee_1 = measure_lp_fee_received(
        &LpFeeLegBalances {
            pool_vault_before: pool_vault_before_1,
            pool_vault_after: read_token_amount(&position.leg_1.pool_vault)?,
            vault_ata_before: vault_ata_before.amount_1,
            vault_ata_after: vault_ata_after.amount_1,
        },
        read_live_transfer_fee(&position.leg_1.mint, epoch)?,
    )?;

    let split_0 = split_lp_fee(lp_fee_0, treasury.fee_rate_bps)?;
    let split_1 = split_lp_fee(lp_fee_1, treasury.fee_rate_bps)?;
    let signer: &[&[&[u8]]] = &[vault_signer_seeds];
    transfer_leg_fee(
        position,
        &position.leg_0,
        &treasury.treasury_ata_0,
        split_0,
        signer,
    )?;
    transfer_leg_fee(
        position,
        &position.leg_1,
        &treasury.treasury_ata_1,
        split_1,
        signer,
    )?;

    emit!(FeesClaimed {
        vault: position.vault.key(),
        gross_a: split_0.gross,
        gross_b: split_1.gross,
        aargau_fee_a: split_0.aargau_fee,
        aargau_fee_b: split_1.aargau_fee,
        user_net_a: split_0.user_net,
        user_net_b: split_1.user_net,
        timestamp: Clock::get()?.unix_timestamp,
    });
    Ok(())
}

/// Remove `liquidity` of principal into the vault ATAs and enforce the
/// min-out floors on the observed credit. Call it after
/// [`collect_lp_fees_to_treasury`] in the same instruction: the collect zeroes
/// the fees owed and no swap can accrue new ones in between, so this decrease
/// carries principal only. Returns the principal received.
pub fn decrease_principal_to_vault<'info>(
    position: &RaydiumPositionAccounts<'info>,
    payout: &PositionPayoutAccounts<'_, 'info>,
    liquidity: u128,
    token_min_0: u64,
    token_min_1: u64,
    vault_signer_seeds: &[&[u8]],
) -> Result<PairAmounts> {
    let before = read_vault_balances(position)?;

    invoke_decrease_liquidity_v2(
        &position.decrease_cpi(payout),
        liquidity,
        token_min_0,
        token_min_1,
        vault_signer_seeds,
    )?;

    let after = read_vault_balances(position)?;
    let received = PairAmounts {
        amount_0: observed_credit(before.amount_0, after.amount_0)?,
        amount_1: observed_credit(before.amount_1, after.amount_1)?,
    };
    // Raydium checks the floors against principal net of transfer fee; the
    // observed ATA credit is the same quantity. Re-check as defence in depth.
    require_principal_meets_min(received.amount_0, token_min_0)?;
    require_principal_meets_min(received.amount_1, token_min_1)?;
    Ok(received)
}

/// Burn the NFT and close the (already empty) position. Raydium refunds all
/// rent to the vault PDA: the caller must sweep it to the user
/// (`vault_ops::sweep_excess_vault_lamports`) in the same instruction.
pub fn close_empty_position<'info>(
    position: &RaydiumPositionAccounts<'info>,
    system_program: &AccountInfo<'info>,
    vault_signer_seeds: &[&[u8]],
) -> Result<()> {
    invoke_close_position(&position.close_cpi(system_program), vault_signer_seeds)
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Transfer the Aargau share of one LP-fee leg to the treasury through the
/// token program that owns the leg's mint.
fn transfer_leg_fee<'info>(
    position: &RaydiumPositionAccounts<'info>,
    leg: &PairLegAccounts<'info>,
    treasury_ata: &AccountInfo<'info>,
    split: LpFeeSplit,
    signer: &[&[&[u8]]],
) -> Result<()> {
    transfer_performance_fee(
        *leg.mint.owner,
        leg.vault_ata.clone(),
        leg.mint.clone(),
        treasury_ata.clone(),
        position.vault.clone(),
        leg.decimals,
        split.aargau_fee,
        signer,
    )
}

fn read_vault_balances(position: &RaydiumPositionAccounts<'_>) -> Result<PairAmounts> {
    Ok(PairAmounts {
        amount_0: read_token_amount(&position.leg_0.vault_ata)?,
        amount_1: read_token_amount(&position.leg_1.vault_ata)?,
    })
}

fn read_token_amount(token_account: &AccountInfo<'_>) -> Result<u64> {
    Ok(read_token_account_view(token_account)?.amount)
}

fn read_live_transfer_fee(
    mint: &AccountInfo<'_>,
    epoch: u64,
) -> Result<Option<TransferFeeSnapshot>> {
    let data = mint.try_borrow_data()?;
    Ok(read_epoch_transfer_fee(&data, epoch))
}
