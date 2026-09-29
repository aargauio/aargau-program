//! Address derivation and bindings for the Raydium CLMM accounts the vault
//! forwards into its CPIs.
//!
//! Mirrors `utils/orca/accounts.rs`. The one wire-level difference from Orca
//! is the tick-array seed: Raydium encodes the start index as **big-endian**
//! i32 bytes, Orca as an ASCII decimal string. A wrong seed silently derives a
//! different (uninitialised) PDA, so every forwarded tick array is bound to
//! the derived address before the CPI.

use anchor_lang::prelude::*;

use crate::constants::{
    RAYDIUM_CLMM_PROGRAM_ID, RAYDIUM_POSITION_SEED, RAYDIUM_TICK_ARRAY_BITMAP_EXTENSION_SEED,
    RAYDIUM_TICK_ARRAY_SEED, RAYDIUM_TICK_ARRAY_SIZE,
};
use crate::errors::AargauError;
use crate::utils::orca::accounts::floor_div;

/// Derive the `PersonalPositionState` PDA: `[b"position", nft_mint]`.
pub fn derive_personal_position_pda(nft_mint: &Pubkey) -> Pubkey {
    let (pda, _) = Pubkey::find_program_address(
        &[RAYDIUM_POSITION_SEED, nft_mint.as_ref()],
        &RAYDIUM_CLMM_PROGRAM_ID,
    );
    pda
}

/// Start tick index of the `TickArrayState` covering `tick`:
/// `floor(tick / (60 * tick_spacing)) * (60 * tick_spacing)`.
///
/// Matches Raydium's `TickArrayState::get_array_start_index`, which rounds
/// negative ticks toward negative infinity. Returns `Err` when
/// `tick_spacing == 0`.
pub fn tick_array_start_index(tick: i32, tick_spacing: u16) -> Result<i32> {
    let span = RAYDIUM_TICK_ARRAY_SIZE
        .checked_mul(i32::from(tick_spacing))
        .ok_or(AargauError::Overflow)?;
    require!(span != 0, AargauError::DivisionByZero);
    let start = floor_div(tick, span)
        .checked_mul(span)
        .ok_or(AargauError::Overflow)?;
    Ok(start)
}

/// Derive the `TickArrayState` PDA:
/// `[b"tick_array", pool_state, start_tick_index.to_be_bytes()]`.
pub fn derive_tick_array_pda(pool_state: &Pubkey, start_tick_index: i32) -> Pubkey {
    let (pda, _) = Pubkey::find_program_address(
        &[
            RAYDIUM_TICK_ARRAY_SEED,
            pool_state.as_ref(),
            &start_tick_index.to_be_bytes(),
        ],
        &RAYDIUM_CLMM_PROGRAM_ID,
    );
    pda
}

/// Derive the `TickArrayBitmapExtension` PDA:
/// `[b"pool_tick_array_bitmap_extension", pool_state]`.
///
/// Every pool has one (`create_pool` initialises it). The vault always
/// forwards it to open / increase / decrease: open and increase index it as
/// `remaining_accounts[0]` whenever a tick array leaves the default bitmap,
/// decrease looks it up by key, and all three ignore it otherwise.
pub fn derive_tick_array_bitmap_extension_pda(pool_state: &Pubkey) -> Pubkey {
    let (pda, _) = Pubkey::find_program_address(
        &[
            RAYDIUM_TICK_ARRAY_BITMAP_EXTENSION_SEED,
            pool_state.as_ref(),
        ],
        &RAYDIUM_CLMM_PROGRAM_ID,
    );
    pda
}

/// Bind a passed tick-array account to the PDA covering `tick` for the given
/// pool and tick spacing. Rejects a mismatch with `InvalidPool`.
pub fn require_tick_array(
    pool_state: &Pubkey,
    tick: i32,
    tick_spacing: u16,
    passed: &Pubkey,
) -> Result<()> {
    let start = tick_array_start_index(tick, tick_spacing)?;
    let expected = derive_tick_array_pda(pool_state, start);
    require_keys_eq!(*passed, expected, AargauError::InvalidPool);
    Ok(())
}

/// Bind a passed bitmap-extension account to the pool's derived PDA. Rejects
/// a mismatch with `InvalidPool`.
pub fn require_tick_array_bitmap_extension(pool_state: &Pubkey, passed: &Pubkey) -> Result<()> {
    let expected = derive_tick_array_bitmap_extension_pda(pool_state);
    require_keys_eq!(*passed, expected, AargauError::InvalidPool);
    Ok(())
}
