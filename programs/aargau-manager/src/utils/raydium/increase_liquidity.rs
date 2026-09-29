//! Raw CPI builder for Raydium CLMM `increase_liquidity_v2`.
//!
//! Pulls at most `amount_0_max` / `amount_1_max` from the vault ATAs into the
//! position. The vault PDA is `nft_owner` (it holds the NFT, so it is the
//! transfer authority) and signs via `invoke_signed`. `base_flag` is pinned to
//! `None`, so Raydium requires `liquidity > 0` (`MissingBaseFlag` otherwise);
//! the builder rejects zero up front.
//!
//! Both token programs sit in fixed slots; Raydium picks one per transfer from
//! the source token account's owner, so SPL / Token-2022 pair mints need no
//! per-side program here.
//!
//! Account order mirrors the on-chain `#[derive(Accounts)]` `IncreaseLiquidityV2`:
//!
//! | idx | account               | flags             | binding                                  |
//! |----:|-----------------------|-------------------|------------------------------------------|
//! |  0  | nft_owner             | readonly + signer | **vault PDA** (signs)                    |
//! |  1  | nft_account           | readonly          | vault NFT ATA (amount == 1)              |
//! |  2  | pool_state            | writable          | vault pool                               |
//! |  3  | protocol_position     | readonly          | deprecated; `RAYDIUM_PROTOCOL_POSITION_PLACEHOLDER` |
//! |  4  | personal_position     | writable          | PDA [b"position", nft_mint]              |
//! |  5  | tick_array_lower      | writable          | PDA(pool, start(tick_lower)) BE seed     |
//! |  6  | tick_array_upper      | writable          | PDA(pool, start(tick_upper)) BE seed     |
//! |  7  | token_account_0       | writable          | vault ATA, mint 0 (source)               |
//! |  8  | token_account_1       | writable          | vault ATA, mint 1 (source)               |
//! |  9  | token_vault_0         | writable          | == pool.token_vault_0                    |
//! | 10  | token_vault_1         | writable          | == pool.token_vault_1                    |
//! | 11  | token_program         | readonly          | == SPL_TOKEN_PROGRAM_ID                  |
//! | 12  | token_program_2022    | readonly          | == TOKEN_2022_PROGRAM_ID                 |
//! | 13  | vault_0_mint          | readonly          | == pool.token_mint_0                     |
//! | 14  | vault_1_mint          | readonly          | == pool.token_mint_1                     |
//! | 15  | tick_array_bitmap_ext | writable          | remaining[0], PDA(pool)                  |
//!
//! Args (Borsh):
//! ```text
//! discriminator: [u8; 8]
//! liquidity: u128
//! amount_0_max: u64
//! amount_1_max: u64
//! base_flag: Option<bool>  // pinned None (single 0u8)
//! ```

use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};

use crate::constants::{
    RAYDIUM_CLMM_PROGRAM_ID, RAYDIUM_INCREASE_LIQUIDITY_V2_DISCRIMINATOR,
    RAYDIUM_PROTOCOL_POSITION_PLACEHOLDER, SPL_TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID,
};
use crate::errors::AargauError;

/// Borsh `Option::<bool>::None`.
const BASE_FLAG_NONE: u8 = 0;

pub struct IncreaseLiquidityV2Cpi<'info> {
    pub clmm_program: AccountInfo<'info>,
    /// Vault PDA — `nft_owner` signer.
    pub vault: AccountInfo<'info>,
    pub nft_account: AccountInfo<'info>,
    pub pool_state: AccountInfo<'info>,
    pub personal_position: AccountInfo<'info>,
    pub tick_array_lower: AccountInfo<'info>,
    pub tick_array_upper: AccountInfo<'info>,
    pub vault_token_0: AccountInfo<'info>,
    pub vault_token_1: AccountInfo<'info>,
    pub token_vault_0: AccountInfo<'info>,
    pub token_vault_1: AccountInfo<'info>,
    pub token_program: AccountInfo<'info>,
    pub token_program_2022: AccountInfo<'info>,
    pub vault_0_mint: AccountInfo<'info>,
    pub vault_1_mint: AccountInfo<'info>,
    pub tick_array_bitmap_extension: AccountInfo<'info>,
}

/// Account keys for [`build_increase_liquidity_v2_instruction_data`].
pub struct IncreaseLiquidityV2Keys {
    pub nft_owner: Pubkey,
    pub nft_account: Pubkey,
    pub pool_state: Pubkey,
    pub protocol_position: Pubkey,
    pub personal_position: Pubkey,
    pub tick_array_lower: Pubkey,
    pub tick_array_upper: Pubkey,
    pub token_account_0: Pubkey,
    pub token_account_1: Pubkey,
    pub token_vault_0: Pubkey,
    pub token_vault_1: Pubkey,
    pub token_program: Pubkey,
    pub token_program_2022: Pubkey,
    pub vault_0_mint: Pubkey,
    pub vault_1_mint: Pubkey,
    pub tick_array_bitmap_extension: Pubkey,
}

/// Pure constructor for the `increase_liquidity_v2` instruction bytes +
/// account metas.
pub fn build_increase_liquidity_v2_instruction_data(
    keys: &IncreaseLiquidityV2Keys,
    liquidity: u128,
    amount_0_max: u64,
    amount_1_max: u64,
) -> Result<(Vec<u8>, Vec<AccountMeta>)> {
    require!(liquidity > 0, AargauError::InvalidActionPayload);

    let mut data = Vec::with_capacity(8 + 16 + 8 + 8 + 1);
    data.extend_from_slice(&RAYDIUM_INCREASE_LIQUIDITY_V2_DISCRIMINATOR);
    data.extend_from_slice(&liquidity.to_le_bytes());
    data.extend_from_slice(&amount_0_max.to_le_bytes());
    data.extend_from_slice(&amount_1_max.to_le_bytes());
    data.push(BASE_FLAG_NONE);

    let metas = vec![
        AccountMeta::new_readonly(keys.nft_owner, true),
        AccountMeta::new_readonly(keys.nft_account, false),
        AccountMeta::new(keys.pool_state, false),
        AccountMeta::new_readonly(keys.protocol_position, false),
        AccountMeta::new(keys.personal_position, false),
        AccountMeta::new(keys.tick_array_lower, false),
        AccountMeta::new(keys.tick_array_upper, false),
        AccountMeta::new(keys.token_account_0, false),
        AccountMeta::new(keys.token_account_1, false),
        AccountMeta::new(keys.token_vault_0, false),
        AccountMeta::new(keys.token_vault_1, false),
        AccountMeta::new_readonly(keys.token_program, false),
        AccountMeta::new_readonly(keys.token_program_2022, false),
        AccountMeta::new_readonly(keys.vault_0_mint, false),
        AccountMeta::new_readonly(keys.vault_1_mint, false),
        AccountMeta::new(keys.tick_array_bitmap_extension, false),
    ];

    Ok((data, metas))
}

pub fn invoke_increase_liquidity_v2(
    cpi: &IncreaseLiquidityV2Cpi<'_>,
    liquidity: u128,
    amount_0_max: u64,
    amount_1_max: u64,
    vault_signer_seeds: &[&[u8]],
) -> Result<()> {
    require_keys_eq!(cpi.clmm_program.key(), RAYDIUM_CLMM_PROGRAM_ID);
    require_keys_eq!(cpi.token_program.key(), SPL_TOKEN_PROGRAM_ID);
    require_keys_eq!(cpi.token_program_2022.key(), TOKEN_2022_PROGRAM_ID);

    let keys = IncreaseLiquidityV2Keys {
        nft_owner: cpi.vault.key(),
        nft_account: cpi.nft_account.key(),
        pool_state: cpi.pool_state.key(),
        protocol_position: RAYDIUM_PROTOCOL_POSITION_PLACEHOLDER,
        personal_position: cpi.personal_position.key(),
        tick_array_lower: cpi.tick_array_lower.key(),
        tick_array_upper: cpi.tick_array_upper.key(),
        token_account_0: cpi.vault_token_0.key(),
        token_account_1: cpi.vault_token_1.key(),
        token_vault_0: cpi.token_vault_0.key(),
        token_vault_1: cpi.token_vault_1.key(),
        token_program: cpi.token_program.key(),
        token_program_2022: cpi.token_program_2022.key(),
        vault_0_mint: cpi.vault_0_mint.key(),
        vault_1_mint: cpi.vault_1_mint.key(),
        tick_array_bitmap_extension: cpi.tick_array_bitmap_extension.key(),
    };
    let (data, metas) =
        build_increase_liquidity_v2_instruction_data(&keys, liquidity, amount_0_max, amount_1_max)?;

    let ix = Instruction {
        program_id: RAYDIUM_CLMM_PROGRAM_ID,
        accounts: metas,
        data,
    };

    // `clmm_program` doubles as the `protocol_position` placeholder.
    let infos = [
        cpi.vault.clone(),
        cpi.nft_account.clone(),
        cpi.pool_state.clone(),
        cpi.clmm_program.clone(),
        cpi.personal_position.clone(),
        cpi.tick_array_lower.clone(),
        cpi.tick_array_upper.clone(),
        cpi.vault_token_0.clone(),
        cpi.vault_token_1.clone(),
        cpi.token_vault_0.clone(),
        cpi.token_vault_1.clone(),
        cpi.token_program.clone(),
        cpi.token_program_2022.clone(),
        cpi.vault_0_mint.clone(),
        cpi.vault_1_mint.clone(),
        cpi.tick_array_bitmap_extension.clone(),
    ];

    invoke_signed(&ix, &infos, &[vault_signer_seeds]).map_err(Into::into)
}
