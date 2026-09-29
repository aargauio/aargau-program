//! Pure guards and balance arithmetic shared by the Raydium CLMM instructions
//! (`execute_action_raydium`, `start_rebalance_raydium`,
//! `retry_pending_rebalance_raydium`).
//!
//! No account access: handlers read balances and on-chain state and pass the
//! values in, so every rule is unit-tested without an SVM runtime.

use anchor_lang::prelude::*;

use crate::constants::{RAYDIUM_MAX_TICK, RAYDIUM_MIN_TICK};
use crate::errors::AargauError;

use super::personal_position_view::PersonalPositionView;

/// The open's NFT mint must be a fresh keypair that signed the transaction:
/// Raydium creates the mint account itself.
pub fn is_fresh_position_nft_mint(is_signer: bool, owner: &Pubkey, data_len: usize) -> bool {
    is_signer && *owner == anchor_lang::system_program::ID && data_len == 0
}

/// For instructions whose CPIs take no reward triples: any
/// `remaining_accounts` entry is unexpected input and is rejected rather
/// than ignored.
pub fn require_no_remaining_accounts(remaining_account_count: usize) -> Result<()> {
    require!(
        remaining_account_count == 0,
        AargauError::InvalidActionPayload
    );
    Ok(())
}

/// A range `open_position_with_token22_nft` accepts: ordered, and each bound
/// a multiple of the pool tick spacing inside `[MIN_TICK, MAX_TICK]`.
///
/// The two-phase rebalance checks this before it persists a target range, so
/// a pending range is always one the OpenNew leg can open.
pub fn require_openable_tick_range(
    tick_lower: i32,
    tick_upper: i32,
    tick_spacing: u16,
) -> Result<()> {
    require!(tick_upper > tick_lower, AargauError::InvalidActionPayload);
    require_openable_tick(tick_lower, tick_spacing)?;
    require_openable_tick(tick_upper, tick_spacing)
}

fn require_openable_tick(tick: i32, tick_spacing: u16) -> Result<()> {
    require!(
        (RAYDIUM_MIN_TICK..=RAYDIUM_MAX_TICK).contains(&tick),
        AargauError::InvalidActionPayload
    );
    let remainder = tick
        .checked_rem(i32::from(tick_spacing))
        .ok_or(AargauError::DivisionByZero)?;
    require!(remainder == 0, AargauError::InvalidActionPayload);
    Ok(())
}

/// Increase payload bounds: non-zero liquidity, at least one non-zero
/// maximum, and neither maximum above the vault's balance.
pub fn require_raydium_increase_bounds(
    liquidity_amount: u128,
    token_max_a: u64,
    token_max_b: u64,
    vault_balance_a: u64,
    vault_balance_b: u64,
) -> Result<()> {
    require!(liquidity_amount > 0, AargauError::InvalidActionPayload);
    require!(
        token_max_a > 0 || token_max_b > 0,
        AargauError::InvalidActionPayload
    );
    require!(
        token_max_a <= vault_balance_a,
        AargauError::InsufficientFunds
    );
    require!(
        token_max_b <= vault_balance_b,
        AargauError::InsufficientFunds
    );
    Ok(())
}

/// Slippage floor for draining a whole position: a position that still holds
/// liquidity returns principal on at least one side, so a 0/0 floor would
/// switch the protection off (only `emergency_withdraw` may exit that way).
/// An empty position moves no principal and needs no floor.
pub fn require_full_drain_floor(
    position_liquidity: u128,
    token_min_a: u64,
    token_min_b: u64,
) -> Result<()> {
    if position_liquidity == 0 {
        return Ok(());
    }
    require!(
        token_min_a > 0 || token_min_b > 0,
        AargauError::SlippageExceeded
    );
    Ok(())
}

/// Raydium only closes an empty position; checking the same fields up front
/// gives a clear error instead of Raydium's `ClosePositionErr`.
pub fn require_raydium_position_empty(position: &PersonalPositionView) -> Result<()> {
    let is_empty = position.liquidity == 0
        && position.token_fees_owed_0 == 0
        && position.token_fees_owed_1 == 0
        && position.reward_amounts_owed.iter().all(|owed| *owed == 0);
    require!(is_empty, AargauError::PositionNotEmpty);
    Ok(())
}

/// Tokens that left a vault ATA across a CPI. A balance that grew during a
/// deposit-style CPI is an accounting inconsistency (`Underflow`).
pub fn observed_debit(balance_before: u64, balance_after: u64) -> Result<u64> {
    balance_before
        .checked_sub(balance_after)
        .ok_or(error!(AargauError::Underflow))
}

/// Tokens that arrived in a vault ATA across a CPI (`PostCpiBalanceDecreased`
/// if it shrank).
pub fn observed_credit(balance_before: u64, balance_after: u64) -> Result<u64> {
    balance_after
        .checked_sub(balance_before)
        .ok_or(error!(AargauError::PostCpiBalanceDecreased))
}

/// Max-in check on the observed debit.
pub fn require_consumed_within_max(consumed: u64, token_max: u64) -> Result<()> {
    require!(consumed <= token_max, AargauError::SlippageExceeded);
    Ok(())
}

/// Min-out check on the observed principal credit.
pub fn require_principal_meets_min(received: u64, token_min: u64) -> Result<()> {
    require!(received >= token_min, AargauError::SlippageExceeded);
    Ok(())
}
