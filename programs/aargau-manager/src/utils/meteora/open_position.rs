//! Raw CPI builder for Meteora DLMM `initialize_position`.
//!
//! Creates a fresh `PositionV2` account whose `owner` is the Aargau vault PDA.
//! The position itself is a transaction-level ephemeral keypair generated
//! off-chain by the client (decision C1) — it co-signs the outer transaction
//! but is **not** a PDA, so the program only forwards the `AccountInfo` and
//! does not derive seeds for it.
//!
//! Wallet vs vault account-flag delta:
//!
//! | idx | account            | wallet-mode flags     | vault-mode flags                 |
//! |----:|--------------------|-----------------------|-----------------------------------|
//! |  0  | payer              | writable + signer     | writable + signer (user_authority) |
//! |  1  | position           | writable + signer     | writable + signer (ephemeral kp)  |
//! |  2  | lb_pair            | readonly              | readonly                          |
//! |  3  | owner              | readonly + signer     | **readonly + signer (vault PDA)** |
//! |  4  | system_program     | readonly              | readonly                          |
//! |  5  | sysvar_rent        | readonly              | readonly                          |
//! |  6  | event_authority    | readonly              | readonly                          |
//! |  7  | dlmm_program       | readonly              | readonly                          |
//!
//! In wallet-mode the `wallet` pubkey is reused at slots [0] and [3]: the
//! wallet is both payer and owner. In vault-mode they diverge — the **user**
//! pays (so excess rent returns to the user on close) and the **vault PDA**
//! owns, so the program has unilateral authority to add/remove liquidity
//! and finally close the position.
//!
//! Args (Borsh):
//! ```text
//! discriminator: [u8; 8]   // METEORA_INITIALIZE_POSITION_DISCRIMINATOR
//! lower_bin_id: i32
//! width: i32               // = upper_bin_id - lower_bin_id + 1
//! ```
//!
//! `width` follows Meteora's own convention (`initialize_position` takes a
//! width, not an upper id). Keeping the helper signature in (lower, upper)
//! and converting internally avoids leaking that quirk to callers.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};

use crate::{
    constants::{METEORA_DLMM_PROGRAM_ID, METEORA_INITIALIZE_POSITION_DISCRIMINATOR},
    errors::AargauError,
};

/// All `AccountInfo`s required by `initialize_position` in vault-mode.
///
/// `vault` is the Aargau vault PDA (acts as `owner`); `user_authority` is the
/// wallet that funds the position rent and signs as `payer`. `position` is
/// the ephemeral `PositionV2` keypair signed off-chain by the client.
pub struct InitializePositionCpi<'info> {
    pub dlmm_program: AccountInfo<'info>,
    pub user_authority: AccountInfo<'info>,
    pub position: AccountInfo<'info>,
    pub lb_pair: AccountInfo<'info>,
    pub vault: AccountInfo<'info>,
    pub system_program: AccountInfo<'info>,
    pub rent_sysvar: AccountInfo<'info>,
    pub event_authority: AccountInfo<'info>,
}

/// Build & invoke `initialize_position(lower_bin_id, width)` with the vault
/// PDA acting as `owner`.
///
/// `vault_signer_seeds` is the vault PDA seed slice including the bump.
/// `width` is derived from `(lower_bin_id, upper_bin_id)`; callers MUST have
/// already enforced the bin-count cap (`MAX_METEORA_BINS`) — this helper
/// rejects malformed ranges defensively but does not surface a bin-cap error.
/// Pure constructor for the raw `initialize_position` instruction bytes +
/// account metas. Extracted so the integration test crate can assert the
/// wire format without spinning up an SVM runtime (the `invoke_signed` path
/// is exercised separately by the mollusk-backed tests).
///
/// Returns `(ix_data, account_metas)`. The caller (`invoke_initialize_position`)
/// wraps this into an `Instruction` + `invoke_signed` call.
#[allow(clippy::too_many_arguments)]
pub fn build_initialize_position_instruction_data(
    user_authority: &Pubkey,
    position: &Pubkey,
    lb_pair: &Pubkey,
    vault: &Pubkey,
    system_program: &Pubkey,
    rent_sysvar: &Pubkey,
    event_authority: &Pubkey,
    dlmm_program: &Pubkey,
    lower_bin_id: i32,
    upper_bin_id: i32,
) -> Result<(Vec<u8>, Vec<AccountMeta>)> {
    require!(
        upper_bin_id >= lower_bin_id,
        AargauError::InvalidActionPayload
    );

    let width: i32 = upper_bin_id
        .checked_sub(lower_bin_id)
        .and_then(|delta| delta.checked_add(1))
        .ok_or(AargauError::Overflow)?;

    let mut data = Vec::with_capacity(8 + 4 + 4);
    data.extend_from_slice(&METEORA_INITIALIZE_POSITION_DISCRIMINATOR);
    data.extend_from_slice(&lower_bin_id.to_le_bytes());
    data.extend_from_slice(&width.to_le_bytes());

    let metas = vec![
        // payer = user wallet (writable + signer)
        AccountMeta::new(*user_authority, true),
        // position = ephemeral keypair (writable + signer)
        AccountMeta::new(*position, true),
        AccountMeta::new_readonly(*lb_pair, false),
        // owner = vault PDA (readonly + signer)
        AccountMeta::new_readonly(*vault, true),
        AccountMeta::new_readonly(*system_program, false),
        AccountMeta::new_readonly(*rent_sysvar, false),
        AccountMeta::new_readonly(*event_authority, false),
        AccountMeta::new_readonly(*dlmm_program, false),
    ];

    Ok((data, metas))
}

pub fn invoke_initialize_position(
    cpi: &InitializePositionCpi<'_>,
    lower_bin_id: i32,
    upper_bin_id: i32,
    vault_signer_seeds: &[&[u8]],
) -> Result<()> {
    require_keys_eq!(cpi.dlmm_program.key(), METEORA_DLMM_PROGRAM_ID);

    let (data, metas) = build_initialize_position_instruction_data(
        &cpi.user_authority.key(),
        &cpi.position.key(),
        &cpi.lb_pair.key(),
        &cpi.vault.key(),
        &cpi.system_program.key(),
        &cpi.rent_sysvar.key(),
        &cpi.event_authority.key(),
        &cpi.dlmm_program.key(),
        lower_bin_id,
        upper_bin_id,
    )?;

    let ix = Instruction {
        program_id: METEORA_DLMM_PROGRAM_ID,
        accounts: metas,
        data,
    };

    let infos = [
        cpi.user_authority.clone(),
        cpi.position.clone(),
        cpi.lb_pair.clone(),
        cpi.vault.clone(),
        cpi.system_program.clone(),
        cpi.rent_sysvar.clone(),
        cpi.event_authority.clone(),
        cpi.dlmm_program.clone(),
    ];

    invoke_signed(&ix, &infos, &[vault_signer_seeds]).map_err(Into::into)
}
