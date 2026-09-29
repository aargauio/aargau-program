//! Raw CPI builder for Raydium CLMM `open_position_with_token22_nft`.
//!
//! Always opens an **empty** position: `liquidity = 0` with `base_flag = None`
//! makes Raydium return before any token transfer, so no funds move here.
//! Liquidity is added afterwards with `increase_liquidity_v2`, signed by the
//! vault. The split exists because Raydium's open pulls tokens from the
//! `payer`'s authority, and the payer must be a system account that can fund
//! rent — the user, never the vault PDA. The empty-open parameters are
//! hard-coded below and are not exposed to callers.
//!
//! The payer (user) funds the NFT mint, the NFT account, `PersonalPositionState`
//! and any tick array Raydium creates on demand. The NFT is minted into the
//! vault PDA's Token-2022 ATA (`position_nft_owner` = vault PDA), so the vault
//! custodies the position. The vault is not a signer of this instruction, so
//! the CPI uses `invoke` without vault seeds.
//!
//! Account order mirrors the on-chain `#[derive(Accounts)]`
//! `OpenPositionWithToken22Nft`:
//!
//! | idx | account                  | flags             | binding                               |
//! |----:|--------------------------|-------------------|---------------------------------------|
//! |  0  | payer                    | writable + signer | user wallet (pays rent)               |
//! |  1  | position_nft_owner       | readonly          | **vault PDA**                         |
//! |  2  | position_nft_mint        | writable + signer | fresh keypair, co-signed off-chain    |
//! |  3  | position_nft_account     | writable          | ATA(vault, nft_mint, Token-2022)      |
//! |  4  | pool_state               | writable          | vault pool                            |
//! |  5  | protocol_position        | readonly          | deprecated; `RAYDIUM_PROTOCOL_POSITION_PLACEHOLDER` |
//! |  6  | tick_array_lower         | writable          | PDA(pool, start(tick_lower)) BE seed  |
//! |  7  | tick_array_upper         | writable          | PDA(pool, start(tick_upper)) BE seed  |
//! |  8  | personal_position        | writable          | PDA [b"position", nft_mint] (init)    |
//! |  9  | token_account_0          | writable          | vault ATA, mint 0 (nothing moves)     |
//! | 10  | token_account_1          | writable          | vault ATA, mint 1 (nothing moves)     |
//! | 11  | token_vault_0            | writable          | == pool.token_vault_0                 |
//! | 12  | token_vault_1            | writable          | == pool.token_vault_1                 |
//! | 13  | rent                     | readonly          | Rent sysvar                           |
//! | 14  | system_program           | readonly          |                                       |
//! | 15  | token_program            | readonly          | == SPL_TOKEN_PROGRAM_ID               |
//! | 16  | associated_token_program | readonly          |                                       |
//! | 17  | token_program_2022       | readonly          | == TOKEN_2022_PROGRAM_ID              |
//! | 18  | vault_0_mint             | readonly          | == pool.token_mint_0                  |
//! | 19  | vault_1_mint             | readonly          | == pool.token_mint_1                  |
//! | 20  | tick_array_bitmap_ext    | writable          | remaining[0], PDA(pool)               |
//!
//! Args (Borsh):
//! ```text
//! discriminator: [u8; 8]
//! tick_lower_index: i32
//! tick_upper_index: i32
//! tick_array_lower_start_index: i32
//! tick_array_upper_start_index: i32
//! liquidity: u128          // pinned 0
//! amount_0_max: u64        // pinned 0
//! amount_1_max: u64        // pinned 0
//! with_metadata: bool      // pinned false
//! base_flag: Option<bool>  // pinned None (single 0u8)
//! ```

use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke,
};

use crate::constants::{
    RAYDIUM_CLMM_PROGRAM_ID, RAYDIUM_OPEN_POSITION_WITH_TOKEN22_NFT_DISCRIMINATOR,
    RAYDIUM_PROTOCOL_POSITION_PLACEHOLDER, SPL_TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID,
};
use crate::errors::AargauError;

use super::accounts::tick_array_start_index;

/// Empty open: zero liquidity and no base token, so Raydium skips the deposit.
const EMPTY_OPEN_LIQUIDITY: u128 = 0;
const EMPTY_OPEN_AMOUNT_MAX: u64 = 0;
/// No Metaplex-style metadata on the NFT (saves the payer the realloc rent).
const WITH_METADATA: bool = false;
/// Borsh `Option::<bool>::None`.
const BASE_FLAG_NONE: u8 = 0;

pub struct OpenPositionWithToken22NftCpi<'info> {
    pub clmm_program: AccountInfo<'info>,
    /// User wallet — payer + signer; funds every account the open creates.
    pub payer: AccountInfo<'info>,
    /// Vault PDA — receives the position NFT.
    pub vault: AccountInfo<'info>,
    /// Fresh Token-2022 mint keypair (co-signed off-chain).
    pub position_nft_mint: AccountInfo<'info>,
    pub position_nft_account: AccountInfo<'info>,
    pub pool_state: AccountInfo<'info>,
    pub tick_array_lower: AccountInfo<'info>,
    pub tick_array_upper: AccountInfo<'info>,
    pub personal_position: AccountInfo<'info>,
    pub token_account_0: AccountInfo<'info>,
    pub token_account_1: AccountInfo<'info>,
    pub token_vault_0: AccountInfo<'info>,
    pub token_vault_1: AccountInfo<'info>,
    pub rent: AccountInfo<'info>,
    pub system_program: AccountInfo<'info>,
    pub token_program: AccountInfo<'info>,
    pub associated_token_program: AccountInfo<'info>,
    pub token_program_2022: AccountInfo<'info>,
    pub vault_0_mint: AccountInfo<'info>,
    pub vault_1_mint: AccountInfo<'info>,
    pub tick_array_bitmap_extension: AccountInfo<'info>,
}

/// Account keys for [`build_open_position_instruction_data`], in wire order.
pub struct OpenPositionKeys {
    pub payer: Pubkey,
    pub position_nft_owner: Pubkey,
    pub position_nft_mint: Pubkey,
    pub position_nft_account: Pubkey,
    pub pool_state: Pubkey,
    pub protocol_position: Pubkey,
    pub tick_array_lower: Pubkey,
    pub tick_array_upper: Pubkey,
    pub personal_position: Pubkey,
    pub token_account_0: Pubkey,
    pub token_account_1: Pubkey,
    pub token_vault_0: Pubkey,
    pub token_vault_1: Pubkey,
    pub rent: Pubkey,
    pub system_program: Pubkey,
    pub token_program: Pubkey,
    pub associated_token_program: Pubkey,
    pub token_program_2022: Pubkey,
    pub vault_0_mint: Pubkey,
    pub vault_1_mint: Pubkey,
    pub tick_array_bitmap_extension: Pubkey,
}

/// Tick range and the start indices of the tick arrays that cover it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenPositionRange {
    pub tick_lower: i32,
    pub tick_upper: i32,
    pub tick_array_lower_start: i32,
    pub tick_array_upper_start: i32,
}

impl OpenPositionRange {
    /// Compute the covering tick-array start indices from the pool's tick
    /// spacing, so they always match the tick-array PDAs the handler bound
    /// with `require_tick_array`.
    pub fn new(tick_lower: i32, tick_upper: i32, tick_spacing: u16) -> Result<Self> {
        Ok(Self {
            tick_lower,
            tick_upper,
            tick_array_lower_start: tick_array_start_index(tick_lower, tick_spacing)?,
            tick_array_upper_start: tick_array_start_index(tick_upper, tick_spacing)?,
        })
    }
}

/// Pure constructor for the `open_position_with_token22_nft` instruction
/// bytes + account metas. Extracted so the test crate can assert the wire
/// format without an SVM runtime.
pub fn build_open_position_instruction_data(
    keys: &OpenPositionKeys,
    range: &OpenPositionRange,
) -> Result<(Vec<u8>, Vec<AccountMeta>)> {
    require!(
        range.tick_upper > range.tick_lower,
        AargauError::InvalidActionPayload
    );

    let mut data = Vec::with_capacity(8 + 4 * 4 + 16 + 8 + 8 + 1 + 1);
    data.extend_from_slice(&RAYDIUM_OPEN_POSITION_WITH_TOKEN22_NFT_DISCRIMINATOR);
    data.extend_from_slice(&range.tick_lower.to_le_bytes());
    data.extend_from_slice(&range.tick_upper.to_le_bytes());
    data.extend_from_slice(&range.tick_array_lower_start.to_le_bytes());
    data.extend_from_slice(&range.tick_array_upper_start.to_le_bytes());
    data.extend_from_slice(&EMPTY_OPEN_LIQUIDITY.to_le_bytes());
    data.extend_from_slice(&EMPTY_OPEN_AMOUNT_MAX.to_le_bytes());
    data.extend_from_slice(&EMPTY_OPEN_AMOUNT_MAX.to_le_bytes());
    data.push(u8::from(WITH_METADATA));
    data.push(BASE_FLAG_NONE);

    let metas = vec![
        AccountMeta::new(keys.payer, true),
        AccountMeta::new_readonly(keys.position_nft_owner, false),
        AccountMeta::new(keys.position_nft_mint, true),
        AccountMeta::new(keys.position_nft_account, false),
        AccountMeta::new(keys.pool_state, false),
        AccountMeta::new_readonly(keys.protocol_position, false),
        AccountMeta::new(keys.tick_array_lower, false),
        AccountMeta::new(keys.tick_array_upper, false),
        AccountMeta::new(keys.personal_position, false),
        AccountMeta::new(keys.token_account_0, false),
        AccountMeta::new(keys.token_account_1, false),
        AccountMeta::new(keys.token_vault_0, false),
        AccountMeta::new(keys.token_vault_1, false),
        AccountMeta::new_readonly(keys.rent, false),
        AccountMeta::new_readonly(keys.system_program, false),
        AccountMeta::new_readonly(keys.token_program, false),
        AccountMeta::new_readonly(keys.associated_token_program, false),
        AccountMeta::new_readonly(keys.token_program_2022, false),
        AccountMeta::new_readonly(keys.vault_0_mint, false),
        AccountMeta::new_readonly(keys.vault_1_mint, false),
        AccountMeta::new(keys.tick_array_bitmap_extension, false),
    ];

    Ok((data, metas))
}

pub fn invoke_open_position_with_token22_nft(
    cpi: &OpenPositionWithToken22NftCpi<'_>,
    tick_lower: i32,
    tick_upper: i32,
    tick_spacing: u16,
) -> Result<()> {
    require_keys_eq!(cpi.clmm_program.key(), RAYDIUM_CLMM_PROGRAM_ID);
    require_keys_eq!(cpi.token_program.key(), SPL_TOKEN_PROGRAM_ID);
    require_keys_eq!(cpi.token_program_2022.key(), TOKEN_2022_PROGRAM_ID);

    let range = OpenPositionRange::new(tick_lower, tick_upper, tick_spacing)?;
    let keys = OpenPositionKeys {
        payer: cpi.payer.key(),
        position_nft_owner: cpi.vault.key(),
        position_nft_mint: cpi.position_nft_mint.key(),
        position_nft_account: cpi.position_nft_account.key(),
        pool_state: cpi.pool_state.key(),
        protocol_position: RAYDIUM_PROTOCOL_POSITION_PLACEHOLDER,
        tick_array_lower: cpi.tick_array_lower.key(),
        tick_array_upper: cpi.tick_array_upper.key(),
        personal_position: cpi.personal_position.key(),
        token_account_0: cpi.token_account_0.key(),
        token_account_1: cpi.token_account_1.key(),
        token_vault_0: cpi.token_vault_0.key(),
        token_vault_1: cpi.token_vault_1.key(),
        rent: cpi.rent.key(),
        system_program: cpi.system_program.key(),
        token_program: cpi.token_program.key(),
        associated_token_program: cpi.associated_token_program.key(),
        token_program_2022: cpi.token_program_2022.key(),
        vault_0_mint: cpi.vault_0_mint.key(),
        vault_1_mint: cpi.vault_1_mint.key(),
        tick_array_bitmap_extension: cpi.tick_array_bitmap_extension.key(),
    };
    let (data, metas) = build_open_position_instruction_data(&keys, &range)?;

    let ix = Instruction {
        program_id: RAYDIUM_CLMM_PROGRAM_ID,
        accounts: metas,
        data,
    };

    // `clmm_program` doubles as the `protocol_position` placeholder.
    let infos = [
        cpi.payer.clone(),
        cpi.vault.clone(),
        cpi.position_nft_mint.clone(),
        cpi.position_nft_account.clone(),
        cpi.pool_state.clone(),
        cpi.clmm_program.clone(),
        cpi.tick_array_lower.clone(),
        cpi.tick_array_upper.clone(),
        cpi.personal_position.clone(),
        cpi.token_account_0.clone(),
        cpi.token_account_1.clone(),
        cpi.token_vault_0.clone(),
        cpi.token_vault_1.clone(),
        cpi.rent.clone(),
        cpi.system_program.clone(),
        cpi.token_program.clone(),
        cpi.associated_token_program.clone(),
        cpi.token_program_2022.clone(),
        cpi.vault_0_mint.clone(),
        cpi.vault_1_mint.clone(),
        cpi.tick_array_bitmap_extension.clone(),
    ];

    invoke(&ix, &infos).map_err(Into::into)
}
