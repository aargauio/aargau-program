//! Raw CPI builder for Meteora DLMM `close_position_if_empty`.
//!
//! ### Why `close_position_if_empty` (and not `close_position2`)
//!
//! Meteora exposes two ways to retire a `PositionV2`:
//!
//! - **`close_position2`** — unconditional close. It will succeed even if
//!   the position still holds liquidity shares; that liquidity becomes
//!   permanently inaccessible to the owner (the position account is gone
//!   and the bin shares are orphaned). Useful for "abandon" semantics, but
//!   catastrophic if invoked by mistake.
//! - **`close_position_if_empty`** — the on-chain handler checks every
//!   per-bin `liquidity_share` is zero and aborts otherwise. This makes the
//!   step idempotent and self-protecting: the caller can be confident that
//!   a successful close means no value was destroyed.
//!
//! The Aargau vault always pairs `remove_liquidity_by_range2(bps = 10_000)`
//! with the close call, so the position is empty when we get here.
//! `close_position_if_empty` is therefore the safe default — if a bug ever
//! left dust shares in a bin the close would fail loudly instead of silently
//! burning user funds.
//!
//! ### Account layout
//!
//! | idx | role             | flags        | vault-mode binding              |
//! |----:|------------------|--------------|---------------------------------|
//! |  0  | position         | writable     | PositionV2 account              |
//! |  1  | sender           | writable + s | **vault PDA** (signs via seeds) |
//! |  2  | rent_receiver    | writable     | user_authority (receives rent)  |
//! |  3  | event_authority  | readonly     | DLMM event authority PDA        |
//! |  4  | dlmm_program     | readonly     | program self-ref                |
//!
//! In wallet-mode slots [1] and [2] collapse onto the user wallet. In
//! vault-mode they diverge: the vault PDA owns the position (so it must
//! sign), while the rent flows back to the user (so the user wallet is the
//! receiver, not the PDA — keeping rent stranded inside a PDA forever is
//! anti-UX and there is no on-chain reason to do so).
//!
//! ### Args (Borsh)
//! ```text
//! discriminator: [u8; 8]   // METEORA_CLOSE_POSITION_IF_EMPTY_DISCRIMINATOR
//! ```
//! No further args — empty body.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};

use crate::constants::{METEORA_CLOSE_POSITION_IF_EMPTY_DISCRIMINATOR, METEORA_DLMM_PROGRAM_ID};

pub struct ClosePositionIfEmptyCpi<'info> {
    pub dlmm_program: AccountInfo<'info>,
    pub position: AccountInfo<'info>,
    pub vault: AccountInfo<'info>,
    pub rent_receiver: AccountInfo<'info>,
    pub event_authority: AccountInfo<'info>,
}

/// Build & invoke `close_position_if_empty` with the vault PDA acting as
/// `sender`. `vault_signer_seeds` is the vault PDA seed slice including the
/// bump.
/// Pure constructor for the raw `close_position_if_empty` instruction
/// bytes + account metas. Extracted so integration tests can validate the
/// wire format without an SVM runtime.
pub fn build_close_position_if_empty_instruction_data(
    position: &Pubkey,
    vault: &Pubkey,
    rent_receiver: &Pubkey,
    event_authority: &Pubkey,
    dlmm_program: &Pubkey,
) -> (Vec<u8>, Vec<AccountMeta>) {
    let data = METEORA_CLOSE_POSITION_IF_EMPTY_DISCRIMINATOR.to_vec();
    let metas = vec![
        AccountMeta::new(*position, false),
        AccountMeta::new(*vault, true), // signer = vault PDA
        AccountMeta::new(*rent_receiver, false),
        AccountMeta::new_readonly(*event_authority, false),
        AccountMeta::new_readonly(*dlmm_program, false),
    ];
    (data, metas)
}

pub fn invoke_close_position_if_empty(
    cpi: &ClosePositionIfEmptyCpi<'_>,
    vault_signer_seeds: &[&[u8]],
) -> Result<()> {
    require_keys_eq!(cpi.dlmm_program.key(), METEORA_DLMM_PROGRAM_ID);

    let (data, metas) = build_close_position_if_empty_instruction_data(
        &cpi.position.key(),
        &cpi.vault.key(),
        &cpi.rent_receiver.key(),
        &cpi.event_authority.key(),
        &cpi.dlmm_program.key(),
    );

    let ix = Instruction {
        program_id: METEORA_DLMM_PROGRAM_ID,
        accounts: metas,
        data,
    };

    let infos = [
        cpi.position.clone(),
        cpi.vault.clone(),
        cpi.rent_receiver.clone(),
        cpi.event_authority.clone(),
        cpi.dlmm_program.clone(),
    ];

    invoke_signed(&ix, &infos, &[vault_signer_seeds]).map_err(Into::into)
}
