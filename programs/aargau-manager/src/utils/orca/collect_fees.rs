//! Raw CPI builders for Orca `collect_fees_v2` (CollectFeesV2) and
//! `collect_reward_v2` (CollectRewardV2).
//!
//! `CollectFees` (the keeper action) sweeps accrued LP fees into the vault
//! ATAs via `collect_fees_v2`, then runs `collect_reward_v2` once per active
//! reward slot (Orca positions carry up to 3). The vault PDA is the
//! `position_authority` and signs both CPIs via `invoke_signed`. The v2
//! variants accept Token-2022 mints (per-side token program + memo program).
//!
//! ### `collect_fees_v2` account order (CollectFeesV2 `#[derive(Accounts)]`)
//!
//! | idx | account              | flags             |
//! |----:|----------------------|-------------------|
//! |  0  | whirlpool            | readonly          |
//! |  1  | position_authority   | readonly + signer | ← vault PDA
//! |  2  | position             | writable          |
//! |  3  | position_token_account | readonly        | ← vault NFT ATA (amount==1)
//! |  4  | token_mint_a         | readonly          |
//! |  5  | token_mint_b         | readonly          |
//! |  6  | token_owner_account_a| writable          | ← vault ATA A
//! |  7  | token_vault_a        | writable          | ← pool vault A
//! |  8  | token_owner_account_b| writable          | ← vault ATA B
//! |  9  | token_vault_b        | writable          | ← pool vault B
//! | 10  | token_program_a      | readonly          |
//! | 11  | token_program_b      | readonly          |
//! | 12  | memo_program         | readonly          |
//! | ... | transfer-hook remaining accounts (none for transfer-fee-only mints) |
//!
//! Args (Borsh): `remaining_accounts_info: Option<RemainingAccountsInfo>` → `None` (0u8).
//!
//! ### `collect_reward_v2` account order (CollectRewardV2 `#[derive(Accounts)]`)
//!
//! | idx | account              | flags             |
//! |----:|----------------------|-------------------|
//! |  0  | whirlpool            | readonly          |
//! |  1  | position_authority   | readonly + signer | ← vault PDA
//! |  2  | position             | writable          |
//! |  3  | position_token_account | readonly        | ← vault NFT ATA (amount==1)
//! |  4  | reward_owner_account | writable          | ← vault reward ATA
//! |  5  | reward_mint          | readonly          |
//! |  6  | reward_vault         | writable          | ← pool reward vault
//! |  7  | reward_token_program | readonly          |
//! |  8  | memo_program         | readonly          |
//! | ... | transfer-hook remaining accounts (none for transfer-fee-only mints) |
//!
//! Args (Borsh): `reward_index: u8`, then `remaining_accounts_info: Option<...>` → `None` (0u8).

use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};

use crate::constants::{
    ORCA_COLLECT_FEES_V2_DISCRIMINATOR, ORCA_COLLECT_REWARD_V2_DISCRIMINATOR,
    ORCA_WHIRLPOOL_PROGRAM_ID,
};

// ── collect_fees_v2 ─────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub struct CollectFeesV2Cpi<'info> {
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
}

/// Pure constructor for the `collect_fees_v2` instruction bytes + account
/// metas. `remaining_accounts_info` is serialised as `None` (single `0u8`).
#[allow(clippy::too_many_arguments)]
pub fn build_collect_fees_v2_instruction_data(
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
) -> (Vec<u8>, Vec<AccountMeta>) {
    let mut data = Vec::with_capacity(8 + 1);
    data.extend_from_slice(&ORCA_COLLECT_FEES_V2_DISCRIMINATOR);
    data.push(0u8); // remaining_accounts_info = None

    // Order taken verbatim from CollectFeesV2 `#[derive(Accounts)]`
    // (orca-so/whirlpools programs/whirlpool/src/instructions/v2/collect_fees.rs):
    // whirlpool is readonly (Box<Account>, no `mut`); token programs + memo come
    // LAST, not first (that front placement is the ModifyLiquidityV2 layout). The
    // A-pair (owner, vault) and B-pair (owner, vault) are interleaved, not grouped.
    let metas = vec![
        AccountMeta::new_readonly(*whirlpool, false),
        AccountMeta::new_readonly(*position_authority, true),
        AccountMeta::new(*position, false),
        AccountMeta::new_readonly(*position_token_account, false),
        AccountMeta::new_readonly(*token_mint_a, false),
        AccountMeta::new_readonly(*token_mint_b, false),
        AccountMeta::new(*token_owner_account_a, false),
        AccountMeta::new(*token_vault_a, false),
        AccountMeta::new(*token_owner_account_b, false),
        AccountMeta::new(*token_vault_b, false),
        AccountMeta::new_readonly(*token_program_a, false),
        AccountMeta::new_readonly(*token_program_b, false),
        AccountMeta::new_readonly(*memo_program, false),
    ];

    (data, metas)
}

pub fn invoke_collect_fees_v2(
    cpi: &CollectFeesV2Cpi<'_>,
    vault_signer_seeds: &[&[u8]],
) -> Result<()> {
    require_keys_eq!(cpi.whirlpool_program.key(), ORCA_WHIRLPOOL_PROGRAM_ID);

    let (data, metas) = build_collect_fees_v2_instruction_data(
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
    );

    let ix = Instruction {
        program_id: ORCA_WHIRLPOOL_PROGRAM_ID,
        accounts: metas,
        data,
    };

    // Must mirror build_collect_fees_v2_instruction_data meta order exactly.
    let infos = [
        cpi.whirlpool.clone(),
        cpi.vault.clone(),
        cpi.position.clone(),
        cpi.position_token_account.clone(),
        cpi.token_mint_a.clone(),
        cpi.token_mint_b.clone(),
        cpi.vault_token_a.clone(),
        cpi.token_vault_a.clone(),
        cpi.vault_token_b.clone(),
        cpi.token_vault_b.clone(),
        cpi.token_program_a.clone(),
        cpi.token_program_b.clone(),
        cpi.memo_program.clone(),
    ];

    invoke_signed(&ix, &infos, &[vault_signer_seeds]).map_err(Into::into)
}

// ── collect_reward_v2 ───────────────────────────────────────────────────────

pub struct CollectRewardV2Cpi<'info> {
    pub whirlpool_program: AccountInfo<'info>,
    pub whirlpool: AccountInfo<'info>,
    /// Vault PDA — `position_authority` signer.
    pub vault: AccountInfo<'info>,
    pub position: AccountInfo<'info>,
    pub position_token_account: AccountInfo<'info>,
    /// Vault ATA receiving the reward token.
    pub reward_owner_account: AccountInfo<'info>,
    pub reward_mint: AccountInfo<'info>,
    pub reward_vault: AccountInfo<'info>,
    pub reward_token_program: AccountInfo<'info>,
    pub memo_program: AccountInfo<'info>,
}

/// Pure constructor for the `collect_reward_v2` instruction bytes + account
/// metas. Args: `reward_index: u8`, then `remaining_accounts_info = None`
/// (single `0u8`).
#[allow(clippy::too_many_arguments)]
pub fn build_collect_reward_v2_instruction_data(
    whirlpool: &Pubkey,
    position_authority: &Pubkey,
    position: &Pubkey,
    position_token_account: &Pubkey,
    reward_owner_account: &Pubkey,
    reward_mint: &Pubkey,
    reward_vault: &Pubkey,
    reward_token_program: &Pubkey,
    memo_program: &Pubkey,
    reward_index: u8,
) -> (Vec<u8>, Vec<AccountMeta>) {
    let mut data = Vec::with_capacity(8 + 1 + 1);
    data.extend_from_slice(&ORCA_COLLECT_REWARD_V2_DISCRIMINATOR);
    data.push(reward_index);
    data.push(0u8); // remaining_accounts_info = None

    // Order from CollectRewardV2 `#[derive(Accounts)]`
    // (orca-so/whirlpools programs/whirlpool/src/instructions/v2/collect_reward.rs):
    // whirlpool is readonly (Box<Account>, no `mut`).
    let metas = vec![
        AccountMeta::new_readonly(*whirlpool, false),
        AccountMeta::new_readonly(*position_authority, true),
        AccountMeta::new(*position, false),
        AccountMeta::new_readonly(*position_token_account, false),
        AccountMeta::new(*reward_owner_account, false),
        AccountMeta::new_readonly(*reward_mint, false),
        AccountMeta::new(*reward_vault, false),
        AccountMeta::new_readonly(*reward_token_program, false),
        AccountMeta::new_readonly(*memo_program, false),
    ];

    (data, metas)
}

pub fn invoke_collect_reward_v2(
    cpi: &CollectRewardV2Cpi<'_>,
    reward_index: u8,
    vault_signer_seeds: &[&[u8]],
) -> Result<()> {
    require_keys_eq!(cpi.whirlpool_program.key(), ORCA_WHIRLPOOL_PROGRAM_ID);

    let (data, metas) = build_collect_reward_v2_instruction_data(
        &cpi.whirlpool.key(),
        &cpi.vault.key(),
        &cpi.position.key(),
        &cpi.position_token_account.key(),
        &cpi.reward_owner_account.key(),
        &cpi.reward_mint.key(),
        &cpi.reward_vault.key(),
        &cpi.reward_token_program.key(),
        &cpi.memo_program.key(),
        reward_index,
    );

    let ix = Instruction {
        program_id: ORCA_WHIRLPOOL_PROGRAM_ID,
        accounts: metas,
        data,
    };

    let infos = [
        cpi.whirlpool.clone(),
        cpi.vault.clone(),
        cpi.position.clone(),
        cpi.position_token_account.clone(),
        cpi.reward_owner_account.clone(),
        cpi.reward_mint.clone(),
        cpi.reward_vault.clone(),
        cpi.reward_token_program.clone(),
        cpi.memo_program.clone(),
    ];

    invoke_signed(&ix, &infos, &[vault_signer_seeds]).map_err(Into::into)
}
