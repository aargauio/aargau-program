//! Raw CPI builder for Orca `open_position_with_token_extensions`.
//!
//! Mints a Token-2022 position NFT (decimals 0, supply 1) into an ATA owned
//! by the **vault PDA** and initialises the `Position` PDA
//! (`[b"position", position_mint]`). The NFT is custodied by the vault — the
//! user wallet never holds it — so holding the NFT is the control invariant
//! for the position.
//!
//! Account order mirrors the on-chain Whirlpools `#[derive(Accounts)]` for
//! `open_position_with_token_extensions`:
//!
//! | idx | account                 | flags             | binding                         |
//! |----:|-------------------------|-------------------|----------------------------------|
//! |  0  | funder                  | writable + signer | user wallet (pays rent)         |
//! |  1  | owner                   | readonly          | **vault PDA**                   |
//! |  2  | position                | writable          | PDA [b"position", position_mint]|
//! |  3  | position_mint           | writable + signer | ephemeral Token-2022 mint kp    |
//! |  4  | position_token_account  | writable          | ATA(vault, position_mint, T22)  |
//! |  5  | whirlpool               | readonly          | pool                            |
//! |  6  | token_2022_program      | readonly          | == TOKEN_2022_PROGRAM_ID        |
//! |  7  | system_program          | readonly          |                                 |
//! |  8  | associated_token_program| readonly          |                                 |
//! |  9  | metadata_update_auth    | readonly          | == ORCA_METADATA_UPDATE_AUTH    |
//!
//! Args (Borsh):
//! ```text
//! discriminator: [u8; 8]
//! tick_lower_index: i32
//! tick_upper_index: i32
//! with_token_metadata_extension: bool   // pinned false (metadata-pointer only)
//! ```
//!
//! `funder` pays so excess rent returns to the user on close. `owner` is the
//! vault PDA — funder ≠ owner here (the user funds, the vault owns), which is
//! the custodial divergence relative to a wallet-mode open.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};

use crate::constants::{
    ORCA_METADATA_UPDATE_AUTH, ORCA_OPEN_POSITION_WITH_TOKEN_EXTENSIONS_DISCRIMINATOR,
    ORCA_WHIRLPOOL_PROGRAM_ID, TOKEN_2022_PROGRAM_ID,
};
use crate::errors::AargauError;

/// The position-NFT metadata extension is not created (metadata-pointer only).
/// Matches the backend `orca/txs/open.rs`.
pub const ORCA_WITH_TOKEN_METADATA_EXTENSION: bool = false;

#[allow(clippy::too_many_arguments)]
pub struct OpenPositionWithTokenExtensionsCpi<'info> {
    pub whirlpool_program: AccountInfo<'info>,
    /// User wallet — funder + signer; pays the NFT/position rent.
    pub funder: AccountInfo<'info>,
    /// Vault PDA — owner of the position + NFT.
    pub vault: AccountInfo<'info>,
    pub position: AccountInfo<'info>,
    /// Ephemeral Token-2022 mint keypair (co-signed off-chain).
    pub position_mint: AccountInfo<'info>,
    pub position_token_account: AccountInfo<'info>,
    pub whirlpool: AccountInfo<'info>,
    pub token_2022_program: AccountInfo<'info>,
    pub system_program: AccountInfo<'info>,
    pub associated_token_program: AccountInfo<'info>,
    pub metadata_update_auth: AccountInfo<'info>,
}

/// Pure constructor for the `open_position_with_token_extensions` instruction
/// bytes + account metas. Extracted so the test crate can assert the wire
/// format without an SVM runtime.
#[allow(clippy::too_many_arguments)]
pub fn build_open_position_instruction_data(
    funder: &Pubkey,
    owner: &Pubkey,
    position: &Pubkey,
    position_mint: &Pubkey,
    position_token_account: &Pubkey,
    whirlpool: &Pubkey,
    token_2022_program: &Pubkey,
    system_program: &Pubkey,
    associated_token_program: &Pubkey,
    metadata_update_auth: &Pubkey,
    tick_lower_index: i32,
    tick_upper_index: i32,
    with_token_metadata_extension: bool,
) -> Result<(Vec<u8>, Vec<AccountMeta>)> {
    require!(
        tick_upper_index > tick_lower_index,
        AargauError::InvalidActionPayload
    );

    let mut data = Vec::with_capacity(8 + 4 + 4 + 1);
    data.extend_from_slice(&ORCA_OPEN_POSITION_WITH_TOKEN_EXTENSIONS_DISCRIMINATOR);
    data.extend_from_slice(&tick_lower_index.to_le_bytes());
    data.extend_from_slice(&tick_upper_index.to_le_bytes());
    data.push(u8::from(with_token_metadata_extension));

    let metas = vec![
        AccountMeta::new(*funder, true),
        AccountMeta::new_readonly(*owner, false),
        AccountMeta::new(*position, false),
        AccountMeta::new(*position_mint, true),
        AccountMeta::new(*position_token_account, false),
        AccountMeta::new_readonly(*whirlpool, false),
        AccountMeta::new_readonly(*token_2022_program, false),
        AccountMeta::new_readonly(*system_program, false),
        AccountMeta::new_readonly(*associated_token_program, false),
        AccountMeta::new_readonly(*metadata_update_auth, false),
    ];

    Ok((data, metas))
}

pub fn invoke_open_position_with_token_extensions(
    cpi: &OpenPositionWithTokenExtensionsCpi<'_>,
    tick_lower_index: i32,
    tick_upper_index: i32,
    vault_signer_seeds: &[&[u8]],
) -> Result<()> {
    require_keys_eq!(cpi.whirlpool_program.key(), ORCA_WHIRLPOOL_PROGRAM_ID);
    require_keys_eq!(cpi.token_2022_program.key(), TOKEN_2022_PROGRAM_ID);
    require_keys_eq!(cpi.metadata_update_auth.key(), ORCA_METADATA_UPDATE_AUTH);

    let (data, metas) = build_open_position_instruction_data(
        &cpi.funder.key(),
        &cpi.vault.key(),
        &cpi.position.key(),
        &cpi.position_mint.key(),
        &cpi.position_token_account.key(),
        &cpi.whirlpool.key(),
        &cpi.token_2022_program.key(),
        &cpi.system_program.key(),
        &cpi.associated_token_program.key(),
        &cpi.metadata_update_auth.key(),
        tick_lower_index,
        tick_upper_index,
        ORCA_WITH_TOKEN_METADATA_EXTENSION,
    )?;

    let ix = Instruction {
        program_id: ORCA_WHIRLPOOL_PROGRAM_ID,
        accounts: metas,
        data,
    };

    let infos = [
        cpi.funder.clone(),
        cpi.vault.clone(),
        cpi.position.clone(),
        cpi.position_mint.clone(),
        cpi.position_token_account.clone(),
        cpi.whirlpool.clone(),
        cpi.token_2022_program.clone(),
        cpi.system_program.clone(),
        cpi.associated_token_program.clone(),
        cpi.metadata_update_auth.clone(),
    ];

    invoke_signed(&ix, &infos, &[vault_signer_seeds]).map_err(Into::into)
}
