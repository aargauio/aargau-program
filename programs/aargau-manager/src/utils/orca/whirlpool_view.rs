//! Pure byte parsing of Orca's `Whirlpool` account and the cross-checks that
//! bind the pool to the mints / vaults / reward accounts passed in by the
//! caller.
//!
//! Extracted from the handler (mirrors `utils/meteora/lb_pair_view`) so the
//! byte-level helpers can be exercised directly from the `tests/` crate. The
//! handler keeps a thin wrapper that reads `AccountInfo::try_borrow_data` and
//! delegates here.
//!
//! Offsets are absolute into the account data buffer (the 8-byte Anchor
//! discriminator counts as bytes 0..8) and match the on-chain-verified
//! offsets in the backend Orca tx builders.

use crate::constants::{ORCA_REWARD_SLOTS, ORCA_WHIRLPOOL_DISCRIMINATOR};
use crate::errors::AargauError;
use anchor_lang::prelude::*;

// Absolute offsets into the `Whirlpool` account buffer.
pub const WHIRLPOOL_DISCRIMINATOR_LEN: usize = 8;
pub const WHIRLPOOL_TICK_SPACING_OFFSET: usize = 41;
pub const WHIRLPOOL_SQRT_PRICE_OFFSET: usize = 65;
pub const WHIRLPOOL_TICK_CURRENT_OFFSET: usize = 81;
pub const WHIRLPOOL_TOKEN_MINT_A_OFFSET: usize = 101;
pub const WHIRLPOOL_TOKEN_VAULT_A_OFFSET: usize = 133;
pub const WHIRLPOOL_TOKEN_MINT_B_OFFSET: usize = 181;
pub const WHIRLPOOL_TOKEN_VAULT_B_OFFSET: usize = 213;

// RewardInfo array starts at byte 269 (after fee_growth_global_b[16] +
// reward_last_updated_timestamp[8]); each RewardInfo is 128 bytes laid out as
// [mint:32][vault:32][extension:32][emissions_per_second_x64:16][growth_global_x64:16].
pub const WHIRLPOOL_REWARD_INFOS_OFFSET: usize = 269;
pub const WHIRLPOOL_REWARD_INFO_STRIDE: usize = 128;
pub const WHIRLPOOL_REWARD_VAULT_RELATIVE_OFFSET: usize = 32;

// Longest field read is reward[2].vault ending at 269 + 2*128 + 64 = 589.
pub const WHIRLPOOL_REQUIRED_LEN: usize =
    WHIRLPOOL_REWARD_INFOS_OFFSET + (ORCA_REWARD_SLOTS - 1) * WHIRLPOOL_REWARD_INFO_STRIDE + 64;

const _: () = assert!(WHIRLPOOL_REQUIRED_LEN >= WHIRLPOOL_TOKEN_VAULT_B_OFFSET + 32);

/// A single reward slot read from the `Whirlpool`. `vault == Pubkey::default()`
/// means the slot is inactive (`collect_reward_v2` is skipped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WhirlpoolRewardSlot {
    pub mint: Pubkey,
    pub vault: Pubkey,
}

/// Subset of `Whirlpool` fields read directly from raw account data.
#[derive(Debug)]
pub struct WhirlpoolView {
    pub tick_spacing: u16,
    pub sqrt_price: u128,
    pub tick_current_index: i32,
    pub token_mint_a: Pubkey,
    pub token_vault_a: Pubkey,
    pub token_mint_b: Pubkey,
    pub token_vault_b: Pubkey,
    pub rewards: [WhirlpoolRewardSlot; ORCA_REWARD_SLOTS],
}

/// Pure byte parser for `Whirlpool`. Validates length and discriminator before
/// reading any field; never panics on malformed input.
pub fn parse_whirlpool_view_from_bytes(data: &[u8]) -> Result<WhirlpoolView> {
    require!(
        data.len() >= WHIRLPOOL_REQUIRED_LEN,
        AargauError::InvalidPool
    );
    require!(
        data[0..WHIRLPOOL_DISCRIMINATOR_LEN] == ORCA_WHIRLPOOL_DISCRIMINATOR,
        AargauError::InvalidPoolDiscriminator
    );

    let tick_spacing = u16::from_le_bytes(
        data[WHIRLPOOL_TICK_SPACING_OFFSET..WHIRLPOOL_TICK_SPACING_OFFSET + 2]
            .try_into()
            .map_err(|_| error!(AargauError::InvalidPool))?,
    );
    let sqrt_price = u128::from_le_bytes(
        data[WHIRLPOOL_SQRT_PRICE_OFFSET..WHIRLPOOL_SQRT_PRICE_OFFSET + 16]
            .try_into()
            .map_err(|_| error!(AargauError::InvalidPool))?,
    );
    let tick_current_index = i32::from_le_bytes(
        data[WHIRLPOOL_TICK_CURRENT_OFFSET..WHIRLPOOL_TICK_CURRENT_OFFSET + 4]
            .try_into()
            .map_err(|_| error!(AargauError::InvalidPool))?,
    );
    let token_mint_a = read_pubkey_at(data, WHIRLPOOL_TOKEN_MINT_A_OFFSET)?;
    let token_vault_a = read_pubkey_at(data, WHIRLPOOL_TOKEN_VAULT_A_OFFSET)?;
    let token_mint_b = read_pubkey_at(data, WHIRLPOOL_TOKEN_MINT_B_OFFSET)?;
    let token_vault_b = read_pubkey_at(data, WHIRLPOOL_TOKEN_VAULT_B_OFFSET)?;

    let mut rewards = [WhirlpoolRewardSlot {
        mint: Pubkey::default(),
        vault: Pubkey::default(),
    }; ORCA_REWARD_SLOTS];
    for (i, slot) in rewards.iter_mut().enumerate() {
        let base = WHIRLPOOL_REWARD_INFOS_OFFSET + i * WHIRLPOOL_REWARD_INFO_STRIDE;
        slot.mint = read_pubkey_at(data, base)?;
        slot.vault = read_pubkey_at(data, base + WHIRLPOOL_REWARD_VAULT_RELATIVE_OFFSET)?;
    }

    Ok(WhirlpoolView {
        tick_spacing,
        sqrt_price,
        tick_current_index,
        token_mint_a,
        token_vault_a,
        token_mint_b,
        token_vault_b,
        rewards,
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

/// Cross-check the passed `mint_a`, `mint_b`, `vault_a` and `vault_b` account
/// keys against the on-chain `Whirlpool` state. Without this binding the
/// `mint_*` / `vault_*` accounts could be substituted for arbitrary token /
/// mint accounts — Anchor only enforces shape, not the pool relationship.
pub fn require_whirlpool_bindings(
    view: &WhirlpoolView,
    mint_a: &Pubkey,
    mint_b: &Pubkey,
    vault_a: &Pubkey,
    vault_b: &Pubkey,
) -> Result<()> {
    require_keys_eq!(*mint_a, view.token_mint_a, AargauError::InvalidPoolMint);
    require_keys_eq!(*mint_b, view.token_mint_b, AargauError::InvalidPoolMint);
    require_keys_eq!(
        *vault_a,
        view.token_vault_a,
        AargauError::InvalidPoolReserve
    );
    require_keys_eq!(
        *vault_b,
        view.token_vault_b,
        AargauError::InvalidPoolReserve
    );
    Ok(())
}
