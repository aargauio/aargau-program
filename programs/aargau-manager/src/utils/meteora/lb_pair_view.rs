//! Pure byte parsing of Meteora's `LbPair` account and the cross-checks that
//! bind the pool to the mints / reserves passed in by the caller.
//!
//! Extracted from the `execute_action` handler so the byte-level helpers can
//! be exercised directly from the `tests/` integration crate (mirrors the
//! pattern of `utils/pool_validation::read_mint_a`). The handler keeps a thin
//! wrapper that reads `AccountInfo::try_borrow_data` and delegates here.

use crate::{constants::METEORA_LB_PAIR_DISCRIMINATOR, errors::AargauError};
use anchor_lang::prelude::*;

/// Subset of `LbPair` fields read directly from the raw account data.
///
/// Offsets are **absolute into the account data buffer** (the 8-byte Anchor
/// discriminator counts as bytes 0..8). Cross-validated against the backend
/// parser at `aargau-backend/.../meteora/adapter.rs` (constants
/// `LB_PAIR_ACTIVE_ID_OFFSET = 76`, `LB_PAIR_TOKEN_X_MINT_OFFSET = 88`,
/// `LB_PAIR_TOKEN_Y_MINT_OFFSET = 120`) and against the tx builder at
/// `aargau-backend/.../meteora/txs/add.rs` (which reads `[152..184]` for
/// `reserve_x` and `[184..216]` for `reserve_y`).
#[derive(Debug)]
pub struct LbPairView {
    pub active_id: i32,
    pub token_x_mint: Pubkey,
    pub token_y_mint: Pubkey,
    pub reserve_x: Pubkey,
    pub reserve_y: Pubkey,
}

// Absolute offsets into the `LbPair` account buffer (the 8-byte Anchor
// discriminator is part of the buffer at bytes 0..8).
pub const LB_PAIR_DISCRIMINATOR_LEN: usize = 8;
pub const LB_PAIR_ACTIVE_ID_OFFSET: usize = 76;
pub const LB_PAIR_TOKEN_X_MINT_OFFSET: usize = 88;
pub const LB_PAIR_TOKEN_Y_MINT_OFFSET: usize = 120;
pub const LB_PAIR_RESERVE_X_OFFSET: usize = 152;
pub const LB_PAIR_RESERVE_Y_OFFSET: usize = 184;
pub const LB_PAIR_REQUIRED_LEN: usize = LB_PAIR_RESERVE_Y_OFFSET + 32;

// Compile-time enforcement that `LB_PAIR_REQUIRED_LEN` covers every offset
// read by `parse_lb_pair_view_from_bytes`. Adding a new field offset here
// without bumping `LB_PAIR_REQUIRED_LEN` will fail to build.
const _: () = assert!(LB_PAIR_REQUIRED_LEN >= LB_PAIR_ACTIVE_ID_OFFSET + 4);
const _: () = assert!(LB_PAIR_REQUIRED_LEN >= LB_PAIR_TOKEN_X_MINT_OFFSET + 32);
const _: () = assert!(LB_PAIR_REQUIRED_LEN >= LB_PAIR_TOKEN_Y_MINT_OFFSET + 32);
const _: () = assert!(LB_PAIR_REQUIRED_LEN >= LB_PAIR_RESERVE_X_OFFSET + 32);
const _: () = assert!(LB_PAIR_REQUIRED_LEN >= LB_PAIR_RESERVE_Y_OFFSET + 32);

/// Pure byte parser for `LbPair`. Validates length and discriminator before
/// reading any field; never panics on malformed input.
pub fn parse_lb_pair_view_from_bytes(data: &[u8]) -> Result<LbPairView> {
    require!(data.len() >= LB_PAIR_REQUIRED_LEN, AargauError::InvalidPool);
    require!(
        data[0..LB_PAIR_DISCRIMINATOR_LEN] == METEORA_LB_PAIR_DISCRIMINATOR,
        AargauError::InvalidPoolDiscriminator
    );

    let active_id = i32::from_le_bytes(
        data[LB_PAIR_ACTIVE_ID_OFFSET..LB_PAIR_ACTIVE_ID_OFFSET + 4]
            .try_into()
            .map_err(|_| AargauError::InvalidPool)?,
    );
    let token_x_mint = read_pubkey_at(data, LB_PAIR_TOKEN_X_MINT_OFFSET)?;
    let token_y_mint = read_pubkey_at(data, LB_PAIR_TOKEN_Y_MINT_OFFSET)?;
    let reserve_x = read_pubkey_at(data, LB_PAIR_RESERVE_X_OFFSET)?;
    let reserve_y = read_pubkey_at(data, LB_PAIR_RESERVE_Y_OFFSET)?;

    Ok(LbPairView {
        active_id,
        token_x_mint,
        token_y_mint,
        reserve_x,
        reserve_y,
    })
}

fn read_pubkey_at(data: &[u8], offset: usize) -> Result<Pubkey> {
    let bytes: [u8; 32] = data
        .get(offset..offset + 32)
        .ok_or(error!(AargauError::InvalidPool))?
        .try_into()
        .map_err(|_| error!(AargauError::InvalidPool))?;
    Ok(Pubkey::from(bytes))
}

/// Cross-check the passed `mint_a`, `mint_b`, `reserve_x` and `reserve_y`
/// account keys against the on-chain `LbPair` state. Without this the
/// `mint_*` and `reserve_*` accounts could be substituted for arbitrary
/// SPL token / mint accounts because Anchor only enforces shape — not
/// the binding to the pool.
pub fn require_lb_pair_bindings(
    view: &LbPairView,
    mint_a: &Pubkey,
    mint_b: &Pubkey,
    reserve_x: &Pubkey,
    reserve_y: &Pubkey,
) -> Result<()> {
    require_keys_eq!(*mint_a, view.token_x_mint, AargauError::InvalidPoolMint);
    require_keys_eq!(*mint_b, view.token_y_mint, AargauError::InvalidPoolMint);
    require_keys_eq!(*reserve_x, view.reserve_x, AargauError::InvalidPoolReserve);
    require_keys_eq!(*reserve_y, view.reserve_y, AargauError::InvalidPoolReserve);
    Ok(())
}

/// Reject Token-2022 mints in this milestone. Transfer-hook plumbing on the
/// Meteora `remaining_accounts_info` tail and on the treasury fee transfers
/// is deferred to a future scope.
pub fn require_spl_classic_mints(
    mint_a_owner: &Pubkey,
    mint_b_owner: &Pubkey,
    token_program_key: &Pubkey,
) -> Result<()> {
    require!(
        mint_a_owner == token_program_key,
        AargauError::Token2022NotSupported
    );
    require!(
        mint_b_owner == token_program_key,
        AargauError::Token2022NotSupported
    );
    Ok(())
}
