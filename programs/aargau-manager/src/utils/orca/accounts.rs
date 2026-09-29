//! Address derivation and account-type guards shared by the Orca CPI
//! builders.
//!
//! Extracted into a standalone module (mirroring `utils/meteora/accounts.rs`)
//! so the pure helpers can be exercised directly from the integration test
//! crate without an SVM runtime.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::pubkey::Pubkey;

use crate::constants::{
    ORCA_POSITION_DISCRIMINATOR, ORCA_POSITION_SEED, ORCA_TICK_ARRAY_SEED,
    ORCA_WHIRLPOOL_PROGRAM_ID, TICK_ARRAY_SIZE,
};
use crate::errors::AargauError;

/// True floor division toward negative infinity.
///
/// Rust's `/` truncates toward zero, which yields the wrong `TickArray`
/// start index for negative ticks (e.g. `-1 / 88 == 0`, but the array
/// covering tick `-1` starts at `-88`). The Whirlpools program computes the
/// start index with floor division, so we must match it exactly. Mirrors the
/// backend `tx/helpers.rs::floor_div`.
pub fn floor_div(a: i32, b: i32) -> i32 {
    let quotient = a / b;
    let remainder = a % b;
    if (remainder != 0) && ((remainder < 0) != (b < 0)) {
        quotient - 1
    } else {
        quotient
    }
}

/// Start tick index of the `TickArray` covering `tick` for the given
/// `tick_spacing`.
///
/// `start = floor_div(tick, TICK_ARRAY_SIZE * tick_spacing) * (TICK_ARRAY_SIZE * tick_spacing)`.
///
/// Returns `Err` when `tick_spacing == 0` (would divide by zero).
pub fn tick_array_start_index(tick: i32, tick_spacing: u16) -> Result<i32> {
    let span = TICK_ARRAY_SIZE
        .checked_mul(i32::from(tick_spacing))
        .ok_or(AargauError::Overflow)?;
    require!(span != 0, AargauError::DivisionByZero);
    let start = floor_div(tick, span)
        .checked_mul(span)
        .ok_or(AargauError::Overflow)?;
    Ok(start)
}

/// Derive the `TickArray` PDA for a whirlpool and a start tick index.
///
/// Seeds: `[b"tick_array", whirlpool, start_tick_index.to_string().as_bytes()]`.
/// The third seed is the **ASCII decimal string** of the start index — not
/// its little-endian bytes. Getting this wrong silently yields a different
/// (uninitialised) PDA and the CPI fails downstream, so the binding is
/// validated against the caller-supplied account.
pub fn derive_tick_array_pda(whirlpool: &Pubkey, start_tick_index: i32) -> Pubkey {
    let start_str = start_tick_index.to_string();
    let (pda, _) = Pubkey::find_program_address(
        &[
            ORCA_TICK_ARRAY_SEED,
            whirlpool.as_ref(),
            start_str.as_bytes(),
        ],
        &ORCA_WHIRLPOOL_PROGRAM_ID,
    );
    pda
}

/// Derive the Orca `Position` PDA from the position NFT mint.
///
/// Seeds: `[b"position", position_mint]`.
pub fn derive_position_pda(position_mint: &Pubkey) -> Pubkey {
    let (pda, _) = Pubkey::find_program_address(
        &[ORCA_POSITION_SEED, position_mint.as_ref()],
        &ORCA_WHIRLPOOL_PROGRAM_ID,
    );
    pda
}

/// Pure byte-level validation that `data` is an Orca `Position` account.
/// Certifies only the account *type* (discriminator) — full layout parsing
/// is out of scope. Never panics on malformed input.
pub fn validate_position_discriminator(data: &[u8]) -> Result<()> {
    require!(
        data.len() >= ORCA_POSITION_DISCRIMINATOR.len(),
        AargauError::InvalidPositionDiscriminator
    );
    require!(
        data[..8] == ORCA_POSITION_DISCRIMINATOR,
        AargauError::InvalidPositionDiscriminator
    );
    Ok(())
}

/// `AccountInfo`-facing guard: the `position` account must be owned by the
/// Whirlpools program AND carry the `Position` discriminator. The key-equality
/// check against `vault.position_address` alone is necessary but not
/// sufficient — an account closed outside the vault could otherwise slip
/// through. Mirrors `meteora::require_position_v2`.
pub fn require_orca_position(account: &AccountInfo<'_>) -> Result<()> {
    require_keys_eq!(
        *account.owner,
        ORCA_WHIRLPOOL_PROGRAM_ID,
        AargauError::InvalidPool
    );
    let data = account.try_borrow_data()?;
    validate_position_discriminator(&data)
}
