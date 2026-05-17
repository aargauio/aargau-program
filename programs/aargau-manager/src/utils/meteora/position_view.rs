//! Lightweight validator for Meteora's `PositionV2` account.
//!
//! Pre-CPI guard: ensures the account passed in as the keeper / vault
//! `position` is actually a `PositionV2` owned by the Meteora DLMM program
//! (rather than an arbitrary account with a colliding pubkey constraint).
//!
//! Mirrors the pattern of `lb_pair_view`: a pure byte parser that lives in a
//! standalone module so the integration test crate can exercise it without
//! pulling in `AccountInfo`.

use crate::{
    constants::{METEORA_DLMM_PROGRAM_ID, METEORA_POSITION_V2_DISCRIMINATOR},
    errors::AargauError,
};
use anchor_lang::prelude::*;

/// Pure byte-level validation. Returns `Ok(())` iff `data` starts with the
/// `PositionV2` Anchor discriminator and is long enough to be a valid base
/// struct. Length floor is the discriminator alone — full layout parsing
/// (per-bin liqShares, fee infos, lower/upper bin ids) is intentionally out
/// of scope here: this helper only certifies the account *type*.
pub fn validate_position_v2_discriminator(data: &[u8]) -> Result<()> {
    require!(
        data.len() >= METEORA_POSITION_V2_DISCRIMINATOR.len(),
        AargauError::InvalidPositionDiscriminator
    );
    require!(
        data[..8] == METEORA_POSITION_V2_DISCRIMINATOR,
        AargauError::InvalidPositionDiscriminator
    );
    Ok(())
}

/// `AccountInfo`-facing wrapper. Validates both the owning program (DLMM) and
/// the account discriminator before letting the caller proceed with a CPI.
///
/// Use this at the top of any handler that consumes a Meteora `position`
/// account. The existing `ExecuteAction` Accounts struct only enforces
/// `position.key() == vault.position_address`, which is necessary but not
/// sufficient — an attacker who closes a position outside the vault and the
/// vault never updates `position_address` would still pass that constraint.
pub fn require_position_v2(account: &AccountInfo<'_>) -> Result<()> {
    require_keys_eq!(
        *account.owner,
        METEORA_DLMM_PROGRAM_ID,
        AargauError::InvalidPool
    );
    let data = account.try_borrow_data()?;
    validate_position_v2_discriminator(&data)
}
