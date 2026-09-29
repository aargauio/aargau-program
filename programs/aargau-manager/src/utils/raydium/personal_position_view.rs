//! Pure byte parsing of Raydium CLMM's `PersonalPositionState` and its binding
//! to the vault.
//!
//! Reading the position on-chain lets handlers use Raydium's own `liquidity`
//! (e.g. to drain a position completely) instead of trusting a caller-supplied
//! amount, and lets them bind the position to the NFT mint the vault recorded
//! on open. Offsets are absolute and live in `constants.rs`.

use anchor_lang::prelude::*;

use crate::constants::{
    RAYDIUM_CLMM_PROGRAM_ID, RAYDIUM_PERSONAL_POSITION_DISCRIMINATOR,
    RAYDIUM_PERSONAL_POSITION_FEES_OWED_0_OFFSET, RAYDIUM_PERSONAL_POSITION_FEES_OWED_1_OFFSET,
    RAYDIUM_PERSONAL_POSITION_LEN, RAYDIUM_PERSONAL_POSITION_LIQUIDITY_OFFSET,
    RAYDIUM_PERSONAL_POSITION_NFT_MINT_OFFSET, RAYDIUM_PERSONAL_POSITION_POOL_ID_OFFSET,
    RAYDIUM_PERSONAL_POSITION_REWARD_OWED_OFFSETS, RAYDIUM_PERSONAL_POSITION_TICK_LOWER_OFFSET,
    RAYDIUM_PERSONAL_POSITION_TICK_UPPER_OFFSET, RAYDIUM_REWARD_SLOTS,
};
use crate::errors::AargauError;
use crate::utils::account_bytes::{read_i32_le, read_pubkey, read_u128_le, read_u64_le};

const POSITION_ERROR: AargauError = AargauError::InvalidPositionDiscriminator;

/// Subset of `PersonalPositionState` fields read directly from raw account
/// data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersonalPositionView {
    pub nft_mint: Pubkey,
    pub pool_id: Pubkey,
    pub tick_lower: i32,
    pub tick_upper: i32,
    pub liquidity: u128,
    pub token_fees_owed_0: u64,
    pub token_fees_owed_1: u64,
    pub reward_amounts_owed: [u64; RAYDIUM_REWARD_SLOTS],
}

/// Pure byte parser. Requires the full account length and the
/// `PersonalPositionState` discriminator before reading any field; never
/// panics on malformed input.
pub fn parse_personal_position_view_from_bytes(data: &[u8]) -> Result<PersonalPositionView> {
    require!(
        data.len() >= RAYDIUM_PERSONAL_POSITION_LEN,
        AargauError::InvalidPositionDiscriminator
    );
    require!(
        data[..RAYDIUM_PERSONAL_POSITION_DISCRIMINATOR.len()]
            == RAYDIUM_PERSONAL_POSITION_DISCRIMINATOR,
        AargauError::InvalidPositionDiscriminator
    );

    let mut reward_amounts_owed = [0u64; RAYDIUM_REWARD_SLOTS];
    for (owed, offset) in reward_amounts_owed
        .iter_mut()
        .zip(RAYDIUM_PERSONAL_POSITION_REWARD_OWED_OFFSETS)
    {
        *owed = read_u64_le(data, offset, POSITION_ERROR)?;
    }

    Ok(PersonalPositionView {
        nft_mint: read_pubkey(
            data,
            RAYDIUM_PERSONAL_POSITION_NFT_MINT_OFFSET,
            POSITION_ERROR,
        )?,
        pool_id: read_pubkey(
            data,
            RAYDIUM_PERSONAL_POSITION_POOL_ID_OFFSET,
            POSITION_ERROR,
        )?,
        tick_lower: read_i32_le(
            data,
            RAYDIUM_PERSONAL_POSITION_TICK_LOWER_OFFSET,
            POSITION_ERROR,
        )?,
        tick_upper: read_i32_le(
            data,
            RAYDIUM_PERSONAL_POSITION_TICK_UPPER_OFFSET,
            POSITION_ERROR,
        )?,
        liquidity: read_u128_le(
            data,
            RAYDIUM_PERSONAL_POSITION_LIQUIDITY_OFFSET,
            POSITION_ERROR,
        )?,
        token_fees_owed_0: read_u64_le(
            data,
            RAYDIUM_PERSONAL_POSITION_FEES_OWED_0_OFFSET,
            POSITION_ERROR,
        )?,
        token_fees_owed_1: read_u64_le(
            data,
            RAYDIUM_PERSONAL_POSITION_FEES_OWED_1_OFFSET,
            POSITION_ERROR,
        )?,
        reward_amounts_owed,
    })
}

/// `AccountInfo`-facing reader: the position must be owned by the CLMM
/// program before its bytes are trusted. Key equality with
/// `vault.position_address` alone is not enough — a position closed outside
/// the vault would still match the stored key.
pub fn read_personal_position_view(position: &AccountInfo<'_>) -> Result<PersonalPositionView> {
    require_keys_eq!(
        *position.owner,
        RAYDIUM_CLMM_PROGRAM_ID,
        AargauError::InvalidPool
    );
    let data = position.try_borrow_data()?;
    parse_personal_position_view_from_bytes(&data)
}

/// Bind the position to the NFT mint the vault recorded on open and to the
/// vault's pool.
pub fn require_personal_position_bindings(
    view: &PersonalPositionView,
    expected_nft_mint: &Pubkey,
    expected_pool: &Pubkey,
) -> Result<()> {
    require_keys_eq!(
        view.nft_mint,
        *expected_nft_mint,
        AargauError::InvalidVaultPda
    );
    require_keys_eq!(view.pool_id, *expected_pool, AargauError::InvalidPool);
    Ok(())
}
