//! Raw CPI builder for Raydium CLMM `close_position`.
//!
//! Burns the position NFT from the vault PDA's Token-2022 ATA, closes that
//! ATA, the NFT mint and `PersonalPositionState`. Raydium requires the
//! position to be empty — zero liquidity, zero fees owed, zero rewards owed —
//! so the caller must drain it with `decrease_liquidity_v2` first.
//!
//! **All reclaimed rent goes to `nft_owner`, i.e. the vault PDA** (Raydium has
//! no separate receiver, unlike Orca). The caller must sweep the vault's
//! excess lamports back to the user in the same instruction
//! (`vault_ops::sweep_excess_vault_lamports`).
//!
//! If either pool mint's freeze authority made Raydium freeze the NFT account
//! at open, Raydium thaws it before the burn and needs the pool (the NFT
//! mint's freeze authority) as `remaining_accounts[0]`. `invoke_close_position`
//! reads the NFT account and appends the pool only in that case.
//!
//! Account order mirrors the on-chain `#[derive(Accounts)]` `ClosePosition`:
//!
//! | idx | account              | flags             | binding                              |
//! |----:|----------------------|-------------------|--------------------------------------|
//! |  0  | nft_owner            | writable + signer | **vault PDA** (signs, receives rent) |
//! |  1  | position_nft_mint    | writable          | == personal_position.nft_mint        |
//! |  2  | position_nft_account | writable          | vault NFT ATA (amount == 1)          |
//! |  3  | personal_position    | writable          | PDA [b"position", nft_mint], closed  |
//! |  4  | system_program       | readonly          |                                      |
//! |  5  | token_program        | readonly          | == TOKEN_2022_PROGRAM_ID (NFT)       |
//! |  6  | pool_state           | readonly          | remaining[0], only if NFT is frozen  |
//!
//! Args (Borsh): discriminator only — no further body.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};

use crate::constants::{
    RAYDIUM_CLMM_PROGRAM_ID, RAYDIUM_CLOSE_POSITION_DISCRIMINATOR, TOKEN_2022_PROGRAM_ID,
};
use crate::utils::token_account::read_token_account_view;

pub struct ClosePositionCpi<'info> {
    pub clmm_program: AccountInfo<'info>,
    /// Vault PDA — `nft_owner` signer; receives every reclaimed lamport.
    pub vault: AccountInfo<'info>,
    pub position_nft_mint: AccountInfo<'info>,
    pub position_nft_account: AccountInfo<'info>,
    pub personal_position: AccountInfo<'info>,
    pub system_program: AccountInfo<'info>,
    pub token_program_2022: AccountInfo<'info>,
    /// Forwarded only when the NFT account is frozen.
    pub pool_state: AccountInfo<'info>,
}

/// Account keys for [`build_close_position_instruction_data`].
pub struct ClosePositionKeys {
    pub nft_owner: Pubkey,
    pub position_nft_mint: Pubkey,
    pub position_nft_account: Pubkey,
    pub personal_position: Pubkey,
    pub system_program: Pubkey,
    pub token_program: Pubkey,
    /// `Some(pool_state)` iff the NFT account is frozen.
    pub pool_state_for_thaw: Option<Pubkey>,
}

/// Pure constructor for the `close_position` instruction bytes + account
/// metas.
pub fn build_close_position_instruction_data(
    keys: &ClosePositionKeys,
) -> (Vec<u8>, Vec<AccountMeta>) {
    let data = RAYDIUM_CLOSE_POSITION_DISCRIMINATOR.to_vec();
    let mut metas = vec![
        AccountMeta::new(keys.nft_owner, true),
        AccountMeta::new(keys.position_nft_mint, false),
        AccountMeta::new(keys.position_nft_account, false),
        AccountMeta::new(keys.personal_position, false),
        AccountMeta::new_readonly(keys.system_program, false),
        AccountMeta::new_readonly(keys.token_program, false),
    ];
    if let Some(pool_state) = keys.pool_state_for_thaw {
        metas.push(AccountMeta::new_readonly(pool_state, false));
    }
    (data, metas)
}

pub fn invoke_close_position(
    cpi: &ClosePositionCpi<'_>,
    vault_signer_seeds: &[&[u8]],
) -> Result<()> {
    require_keys_eq!(cpi.clmm_program.key(), RAYDIUM_CLMM_PROGRAM_ID);
    require_keys_eq!(cpi.token_program_2022.key(), TOKEN_2022_PROGRAM_ID);

    let is_nft_frozen = read_token_account_view(&cpi.position_nft_account)?.is_frozen;
    let keys = ClosePositionKeys {
        nft_owner: cpi.vault.key(),
        position_nft_mint: cpi.position_nft_mint.key(),
        position_nft_account: cpi.position_nft_account.key(),
        personal_position: cpi.personal_position.key(),
        system_program: cpi.system_program.key(),
        token_program: cpi.token_program_2022.key(),
        pool_state_for_thaw: is_nft_frozen.then(|| cpi.pool_state.key()),
    };
    let (data, metas) = build_close_position_instruction_data(&keys);

    let ix = Instruction {
        program_id: RAYDIUM_CLMM_PROGRAM_ID,
        accounts: metas,
        data,
    };

    let mut infos = vec![
        cpi.vault.clone(),
        cpi.position_nft_mint.clone(),
        cpi.position_nft_account.clone(),
        cpi.personal_position.clone(),
        cpi.system_program.clone(),
        cpi.token_program_2022.clone(),
    ];
    if is_nft_frozen {
        infos.push(cpi.pool_state.clone());
    }

    invoke_signed(&ix, &infos, &[vault_signer_seeds]).map_err(Into::into)
}
