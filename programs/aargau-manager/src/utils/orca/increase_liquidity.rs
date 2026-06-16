//! Raw CPI builder for Orca `increase_liquidity_v2` (ModifyLiquidityV2).
//!
//! Pulls `token_max_a` / `token_max_b` from the vault ATAs into the active
//! position; the vault PDA is the `position_authority` and signs via
//! `invoke_signed`. The v2 variant accepts Token-2022 pair mints (per-side
//! token program + memo program).
//!
//! Account order mirrors the on-chain Whirlpools `#[derive(Accounts)]` for
//! ModifyLiquidityV2:
//!
//! | idx | account              | flags             |
//! |----:|----------------------|-------------------|
//! |  0  | whirlpool            | writable          |
//! |  1  | token_program_a      | readonly          |
//! |  2  | token_program_b      | readonly          |
//! |  3  | memo_program         | readonly          |
//! |  4  | position_authority   | readonly + signer | ← vault PDA
//! |  5  | position             | writable          |
//! |  6  | position_token_account | readonly        | ← vault NFT ATA (amount==1)
//! |  7  | token_mint_a         | readonly          |
//! |  8  | token_mint_b         | readonly          |
//! |  9  | token_owner_account_a| writable          | ← vault ATA A
//! | 10  | token_owner_account_b| writable          | ← vault ATA B
//! | 11  | token_vault_a        | writable          | ← pool vault A
//! | 12  | token_vault_b        | writable          | ← pool vault B
//! | 13  | tick_array_lower     | writable          | pre-initialised off-chain
//! | 14  | tick_array_upper     | writable          | pre-initialised off-chain
//! | ... | transfer-hook remaining accounts (none for transfer-fee-only mints) |
//!
//! Args (Borsh):
//! ```text
//! discriminator: [u8; 8]
//! liquidity_amount: u128
//! token_max_a: u64
//! token_max_b: u64
//! remaining_accounts_info: Option<RemainingAccountsInfo>  // None = single 0u8
//! ```

use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};

use crate::constants::{ORCA_INCREASE_LIQUIDITY_V2_DISCRIMINATOR, ORCA_WHIRLPOOL_PROGRAM_ID};

#[allow(clippy::too_many_arguments)]
pub struct IncreaseLiquidityV2Cpi<'info> {
    pub whirlpool_program: AccountInfo<'info>,
    pub whirlpool: AccountInfo<'info>,
    pub token_program_a: AccountInfo<'info>,
    pub token_program_b: AccountInfo<'info>,
    pub memo_program: AccountInfo<'info>,
    /// Vault PDA — `position_authority` signer.
    pub vault: AccountInfo<'info>,
    pub position: AccountInfo<'info>,
    pub position_token_account: AccountInfo<'info>,
    pub token_mint_a: AccountInfo<'info>,
    pub token_mint_b: AccountInfo<'info>,
    pub vault_token_a: AccountInfo<'info>,
    pub vault_token_b: AccountInfo<'info>,
    pub token_vault_a: AccountInfo<'info>,
    pub token_vault_b: AccountInfo<'info>,
    pub tick_array_lower: AccountInfo<'info>,
    pub tick_array_upper: AccountInfo<'info>,
}

/// Pure constructor for the `increase_liquidity_v2` instruction bytes +
/// account metas. `remaining_accounts_info` is serialised as `None` (single
/// `0u8`) — transfer hooks are not wired; transfer-fee-only Token-2022 mints
/// need no hook accounts.
#[allow(clippy::too_many_arguments)]
pub fn build_increase_liquidity_v2_instruction_data(
    whirlpool: &Pubkey,
    token_program_a: &Pubkey,
    token_program_b: &Pubkey,
    memo_program: &Pubkey,
    position_authority: &Pubkey,
    position: &Pubkey,
    position_token_account: &Pubkey,
    token_mint_a: &Pubkey,
    token_mint_b: &Pubkey,
    token_owner_account_a: &Pubkey,
    token_owner_account_b: &Pubkey,
    token_vault_a: &Pubkey,
    token_vault_b: &Pubkey,
    tick_array_lower: &Pubkey,
    tick_array_upper: &Pubkey,
    liquidity_amount: u128,
    token_max_a: u64,
    token_max_b: u64,
) -> (Vec<u8>, Vec<AccountMeta>) {
    let mut data = Vec::with_capacity(8 + 16 + 8 + 8 + 1);
    data.extend_from_slice(&ORCA_INCREASE_LIQUIDITY_V2_DISCRIMINATOR);
    data.extend_from_slice(&liquidity_amount.to_le_bytes());
    data.extend_from_slice(&token_max_a.to_le_bytes());
    data.extend_from_slice(&token_max_b.to_le_bytes());
    data.push(0u8); // remaining_accounts_info = None

    let metas = vec![
        AccountMeta::new(*whirlpool, false),
        AccountMeta::new_readonly(*token_program_a, false),
        AccountMeta::new_readonly(*token_program_b, false),
        AccountMeta::new_readonly(*memo_program, false),
        AccountMeta::new_readonly(*position_authority, true),
        AccountMeta::new(*position, false),
        AccountMeta::new_readonly(*position_token_account, false),
        AccountMeta::new_readonly(*token_mint_a, false),
        AccountMeta::new_readonly(*token_mint_b, false),
        AccountMeta::new(*token_owner_account_a, false),
        AccountMeta::new(*token_owner_account_b, false),
        AccountMeta::new(*token_vault_a, false),
        AccountMeta::new(*token_vault_b, false),
        AccountMeta::new(*tick_array_lower, false),
        AccountMeta::new(*tick_array_upper, false),
    ];

    (data, metas)
}

pub fn invoke_increase_liquidity_v2(
    cpi: &IncreaseLiquidityV2Cpi<'_>,
    liquidity_amount: u128,
    token_max_a: u64,
    token_max_b: u64,
    vault_signer_seeds: &[&[u8]],
) -> Result<()> {
    require_keys_eq!(cpi.whirlpool_program.key(), ORCA_WHIRLPOOL_PROGRAM_ID);

    let (data, metas) = build_increase_liquidity_v2_instruction_data(
        &cpi.whirlpool.key(),
        &cpi.token_program_a.key(),
        &cpi.token_program_b.key(),
        &cpi.memo_program.key(),
        &cpi.vault.key(),
        &cpi.position.key(),
        &cpi.position_token_account.key(),
        &cpi.token_mint_a.key(),
        &cpi.token_mint_b.key(),
        &cpi.vault_token_a.key(),
        &cpi.vault_token_b.key(),
        &cpi.token_vault_a.key(),
        &cpi.token_vault_b.key(),
        &cpi.tick_array_lower.key(),
        &cpi.tick_array_upper.key(),
        liquidity_amount,
        token_max_a,
        token_max_b,
    );

    let ix = Instruction {
        program_id: ORCA_WHIRLPOOL_PROGRAM_ID,
        accounts: metas,
        data,
    };

    let infos = [
        cpi.whirlpool.clone(),
        cpi.token_program_a.clone(),
        cpi.token_program_b.clone(),
        cpi.memo_program.clone(),
        cpi.vault.clone(),
        cpi.position.clone(),
        cpi.position_token_account.clone(),
        cpi.token_mint_a.clone(),
        cpi.token_mint_b.clone(),
        cpi.vault_token_a.clone(),
        cpi.vault_token_b.clone(),
        cpi.token_vault_a.clone(),
        cpi.token_vault_b.clone(),
        cpi.tick_array_lower.clone(),
        cpi.tick_array_upper.clone(),
    ];

    invoke_signed(&ix, &infos, &[vault_signer_seeds]).map_err(Into::into)
}
