//! Raw CPI builder for Orca `close_position_with_token_extensions`.
//!
//! Burns the Token-2022 position NFT from the vault PDA's ATA, closes the mint,
//! and refunds the rent to `receiver` (the user wallet). The Whirlpools program
//! **requires the position to be fully empty** — the caller must have drained
//! all liquidity (`DecreaseLiquidity` to zero) and swept fees/rewards first, or
//! the program aborts.
//!
//! Account order mirrors the on-chain Whirlpools `#[derive(Accounts)]`:
//!
//! | idx | account              | flags             | binding                       |
//! |----:|----------------------|-------------------|-------------------------------|
//! |  0  | position_authority   | readonly + signer | **vault PDA** (signs)         |
//! |  1  | receiver             | writable          | user wallet (gets rent)       |
//! |  2  | position             | writable          | closed → receiver             |
//! |  3  | position_mint        | writable          | == position.position_mint     |
//! |  4  | position_token_account | writable        | vault NFT ATA (amount==1)     |
//! |  5  | token_2022_program   | readonly          | == TOKEN_2022_PROGRAM_ID      |
//!
//! Args (Borsh): discriminator only — no further body.
//!
//! `position_authority` (vault PDA) and `receiver` (user) diverge: the vault
//! signs because it owns the NFT, but the rent flows back to the user who
//! funded it on open. Stranding rent inside the PDA is anti-UX and there is no
//! on-chain reason to do so.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};

use crate::constants::{
    ORCA_CLOSE_POSITION_WITH_TOKEN_EXTENSIONS_DISCRIMINATOR, ORCA_WHIRLPOOL_PROGRAM_ID,
    TOKEN_2022_PROGRAM_ID,
};

pub struct ClosePositionWithTokenExtensionsCpi<'info> {
    pub whirlpool_program: AccountInfo<'info>,
    /// Vault PDA — `position_authority` signer.
    pub vault: AccountInfo<'info>,
    /// User wallet — receives the reclaimed rent.
    pub receiver: AccountInfo<'info>,
    pub position: AccountInfo<'info>,
    pub position_mint: AccountInfo<'info>,
    pub position_token_account: AccountInfo<'info>,
    pub token_2022_program: AccountInfo<'info>,
}

/// Pure constructor for the `close_position_with_token_extensions` instruction
/// bytes + account metas. Extracted so the test crate can assert the wire
/// format without an SVM runtime.
pub fn build_close_position_instruction_data(
    position_authority: &Pubkey,
    receiver: &Pubkey,
    position: &Pubkey,
    position_mint: &Pubkey,
    position_token_account: &Pubkey,
    token_2022_program: &Pubkey,
) -> (Vec<u8>, Vec<AccountMeta>) {
    let data = ORCA_CLOSE_POSITION_WITH_TOKEN_EXTENSIONS_DISCRIMINATOR.to_vec();
    let metas = vec![
        AccountMeta::new_readonly(*position_authority, true),
        AccountMeta::new(*receiver, false),
        AccountMeta::new(*position, false),
        AccountMeta::new(*position_mint, false),
        AccountMeta::new(*position_token_account, false),
        AccountMeta::new_readonly(*token_2022_program, false),
    ];
    (data, metas)
}

pub fn invoke_close_position_with_token_extensions(
    cpi: &ClosePositionWithTokenExtensionsCpi<'_>,
    vault_signer_seeds: &[&[u8]],
) -> Result<()> {
    require_keys_eq!(cpi.whirlpool_program.key(), ORCA_WHIRLPOOL_PROGRAM_ID);
    require_keys_eq!(cpi.token_2022_program.key(), TOKEN_2022_PROGRAM_ID);

    let (data, metas) = build_close_position_instruction_data(
        &cpi.vault.key(),
        &cpi.receiver.key(),
        &cpi.position.key(),
        &cpi.position_mint.key(),
        &cpi.position_token_account.key(),
        &cpi.token_2022_program.key(),
    );

    let ix = Instruction {
        program_id: ORCA_WHIRLPOOL_PROGRAM_ID,
        accounts: metas,
        data,
    };

    let infos = [
        cpi.vault.clone(),
        cpi.receiver.clone(),
        cpi.position.clone(),
        cpi.position_mint.clone(),
        cpi.position_token_account.clone(),
        cpi.token_2022_program.clone(),
    ];

    invoke_signed(&ix, &infos, &[vault_signer_seeds]).map_err(Into::into)
}
