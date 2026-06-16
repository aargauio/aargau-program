//! Raw CPI builder for Orca `decrease_liquidity_v2` (ModifyLiquidityV2).
//!
//! Shares the ModifyLiquidityV2 account context with `increase_liquidity_v2`
//! (see that module for the full account table). The only differences are the
//! discriminator and the args: `decrease` takes `token_min_a` / `token_min_b`
//! as slippage floors instead of maxima, and tokens flow from the position
//! back into the vault ATAs.
//!
//! Args (Borsh):
//! ```text
//! discriminator: [u8; 8]
//! liquidity_amount: u128
//! token_min_a: u64
//! token_min_b: u64
//! remaining_accounts_info: Option<RemainingAccountsInfo>  // None = single 0u8
//! ```

use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};

use crate::constants::{ORCA_DECREASE_LIQUIDITY_V2_DISCRIMINATOR, ORCA_WHIRLPOOL_PROGRAM_ID};

use super::increase_liquidity::IncreaseLiquidityV2Cpi;

/// `decrease_liquidity_v2` reuses the ModifyLiquidityV2 account context.
pub type DecreaseLiquidityV2Cpi<'info> = IncreaseLiquidityV2Cpi<'info>;

/// Pure constructor for the `decrease_liquidity_v2` instruction bytes +
/// account metas. Account ordering is identical to ModifyLiquidityV2; only the
/// discriminator and the arg semantics differ. `remaining_accounts_info` is
/// serialised as `None` (single `0u8`).
#[allow(clippy::too_many_arguments)]
pub fn build_decrease_liquidity_v2_instruction_data(
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
    token_min_a: u64,
    token_min_b: u64,
) -> (Vec<u8>, Vec<AccountMeta>) {
    let mut data = Vec::with_capacity(8 + 16 + 8 + 8 + 1);
    data.extend_from_slice(&ORCA_DECREASE_LIQUIDITY_V2_DISCRIMINATOR);
    data.extend_from_slice(&liquidity_amount.to_le_bytes());
    data.extend_from_slice(&token_min_a.to_le_bytes());
    data.extend_from_slice(&token_min_b.to_le_bytes());
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

pub fn invoke_decrease_liquidity_v2(
    cpi: &DecreaseLiquidityV2Cpi<'_>,
    liquidity_amount: u128,
    token_min_a: u64,
    token_min_b: u64,
    vault_signer_seeds: &[&[u8]],
) -> Result<()> {
    require_keys_eq!(cpi.whirlpool_program.key(), ORCA_WHIRLPOOL_PROGRAM_ID);

    let (data, metas) = build_decrease_liquidity_v2_instruction_data(
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
        token_min_a,
        token_min_b,
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
