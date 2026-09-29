//! Pure byte parsing of Raydium CLMM's `PoolState` and the cross-checks that
//! bind the pool to the mints and pool vaults passed in by the caller.
//!
//! Mirrors `utils/orca/whirlpool_view`. Offsets are absolute into the account
//! data buffer (the 8-byte Anchor discriminator counts as bytes 0..8) and live
//! in `constants.rs`. `PoolState` is `#[repr(C, packed)]`, so the offsets have
//! no alignment padding.

use anchor_lang::prelude::*;

use crate::constants::{
    RAYDIUM_CLMM_PROGRAM_ID, RAYDIUM_POOL_REWARD_INFOS_OFFSET, RAYDIUM_POOL_REWARD_INFO_STRIDE,
    RAYDIUM_POOL_STATE_DISCRIMINATOR, RAYDIUM_POOL_TICK_SPACING_OFFSET,
    RAYDIUM_POOL_TOKEN_MINT_0_OFFSET, RAYDIUM_POOL_TOKEN_MINT_1_OFFSET,
    RAYDIUM_POOL_TOKEN_VAULT_0_OFFSET, RAYDIUM_POOL_TOKEN_VAULT_1_OFFSET,
    RAYDIUM_REWARD_INFO_TOKEN_MINT_RELATIVE_OFFSET,
    RAYDIUM_REWARD_INFO_TOKEN_VAULT_RELATIVE_OFFSET, RAYDIUM_REWARD_SLOTS,
};
use crate::errors::AargauError;
use crate::utils::account_bytes::{read_pubkey, read_u16_le};

const POOL_STATE_DISCRIMINATOR_LEN: usize = 8;

/// Last byte read is `reward_infos[2].token_vault`:
/// 397 + 2 * 169 + 89 + 32 = 856.
pub const POOL_STATE_REQUIRED_LEN: usize = RAYDIUM_POOL_REWARD_INFOS_OFFSET
    + (RAYDIUM_REWARD_SLOTS - 1) * RAYDIUM_POOL_REWARD_INFO_STRIDE
    + RAYDIUM_REWARD_INFO_TOKEN_VAULT_RELATIVE_OFFSET
    + 32;

const _: () = assert!(POOL_STATE_REQUIRED_LEN >= RAYDIUM_POOL_TICK_SPACING_OFFSET + 2);

/// One reward slot read from the pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RaydiumRewardSlot {
    pub token_mint: Pubkey,
    pub token_vault: Pubkey,
}

impl RaydiumRewardSlot {
    /// Matches Raydium's `RewardInfo::initialized()`. Ended rewards stay
    /// initialized, and `decrease_liquidity_v2` requires one account triple
    /// for every initialized slot.
    pub fn is_initialized(&self) -> bool {
        self.token_mint != Pubkey::default()
    }
}

/// Subset of `PoolState` fields read directly from raw account data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolStateView {
    pub token_mint_0: Pubkey,
    pub token_mint_1: Pubkey,
    pub token_vault_0: Pubkey,
    pub token_vault_1: Pubkey,
    pub tick_spacing: u16,
    pub rewards: [RaydiumRewardSlot; RAYDIUM_REWARD_SLOTS],
}

impl PoolStateView {
    /// Initialized reward slots, in slot order.
    pub fn initialized_rewards(&self) -> impl Iterator<Item = &RaydiumRewardSlot> {
        self.rewards.iter().filter(|slot| slot.is_initialized())
    }
}

/// Pure byte parser for `PoolState`. Validates length and discriminator before
/// reading any field; never panics on malformed input.
pub fn parse_pool_state_view_from_bytes(data: &[u8]) -> Result<PoolStateView> {
    require!(
        data.len() >= POOL_STATE_REQUIRED_LEN,
        AargauError::InvalidPool
    );
    require!(
        data[..POOL_STATE_DISCRIMINATOR_LEN] == RAYDIUM_POOL_STATE_DISCRIMINATOR,
        AargauError::InvalidPoolDiscriminator
    );

    let mut rewards = [RaydiumRewardSlot::default(); RAYDIUM_REWARD_SLOTS];
    for (index, slot) in rewards.iter_mut().enumerate() {
        let base = RAYDIUM_POOL_REWARD_INFOS_OFFSET + index * RAYDIUM_POOL_REWARD_INFO_STRIDE;
        slot.token_mint =
            read_pool_pubkey(data, base + RAYDIUM_REWARD_INFO_TOKEN_MINT_RELATIVE_OFFSET)?;
        slot.token_vault =
            read_pool_pubkey(data, base + RAYDIUM_REWARD_INFO_TOKEN_VAULT_RELATIVE_OFFSET)?;
    }

    Ok(PoolStateView {
        token_mint_0: read_pool_pubkey(data, RAYDIUM_POOL_TOKEN_MINT_0_OFFSET)?,
        token_mint_1: read_pool_pubkey(data, RAYDIUM_POOL_TOKEN_MINT_1_OFFSET)?,
        token_vault_0: read_pool_pubkey(data, RAYDIUM_POOL_TOKEN_VAULT_0_OFFSET)?,
        token_vault_1: read_pool_pubkey(data, RAYDIUM_POOL_TOKEN_VAULT_1_OFFSET)?,
        tick_spacing: read_u16_le(
            data,
            RAYDIUM_POOL_TICK_SPACING_OFFSET,
            AargauError::InvalidPool,
        )?,
        rewards,
    })
}

/// `AccountInfo`-facing reader: the pool must be owned by the CLMM program
/// before its bytes are trusted.
pub fn read_pool_state_view(pool_state: &AccountInfo<'_>) -> Result<PoolStateView> {
    require_keys_eq!(
        *pool_state.owner,
        RAYDIUM_CLMM_PROGRAM_ID,
        AargauError::InvalidPool
    );
    let data = pool_state.try_borrow_data()?;
    parse_pool_state_view_from_bytes(&data)
}

fn read_pool_pubkey(data: &[u8], offset: usize) -> Result<Pubkey> {
    read_pubkey(data, offset, AargauError::InvalidPool)
}

/// Cross-check the passed pair mints and pool vaults against the on-chain
/// `PoolState`. Without it a caller could substitute an arbitrary token
/// account for a pool vault, which would corrupt the vault-outflow fee
/// measurement. Mint 0 / vault 0 pair with the vault's token A side.
pub fn require_pool_state_bindings(
    view: &PoolStateView,
    mint_0: &Pubkey,
    mint_1: &Pubkey,
    vault_0: &Pubkey,
    vault_1: &Pubkey,
) -> Result<()> {
    require_keys_eq!(*mint_0, view.token_mint_0, AargauError::InvalidPoolMint);
    require_keys_eq!(*mint_1, view.token_mint_1, AargauError::InvalidPoolMint);
    require_keys_eq!(
        *vault_0,
        view.token_vault_0,
        AargauError::InvalidPoolReserve
    );
    require_keys_eq!(
        *vault_1,
        view.token_vault_1,
        AargauError::InvalidPoolReserve
    );
    Ok(())
}
