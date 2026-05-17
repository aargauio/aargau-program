//! Raw CPI builder for Meteora DLMM `claim_fee2`.
//!
//! Account layout (mirrors the Meteora DLMM IDL):
//!
//! | idx | role             | flags         |
//! |----:|------------------|---------------|
//! |  0  | lb_pair          | writable      |
//! |  1  | position         | writable      |
//! |  2  | sender           | signer + w    |  ← vault PDA
//! |  3  | reserve_x        | writable      |
//! |  4  | reserve_y        | writable      |
//! |  5  | user_token_x     | writable      |  ← vault ATA A
//! |  6  | user_token_y     | writable      |  ← vault ATA B
//! |  7  | token_x_mint     | readonly      |
//! |  8  | token_y_mint     | readonly      |
//! |  9  | token_program_x  | readonly      |
//! | 10  | token_program_y  | readonly      |
//! | 11  | memo_program     | readonly      |
//! | 12  | event_authority  | readonly      |
//! | 13  | dlmm_program     | readonly      |
//! | 14  | bin_array_lower  | writable      |
//! | 15  | bin_array_upper  | writable      |
//!
//! Args (Borsh):
//! ```text
//! discriminator: [u8; 8]
//! min_bin_id: i32
//! max_bin_id: i32
//! remaining_accounts_info: { slices: [{0,0},{1,0}] }
//! ```

use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};

use crate::constants::{METEORA_CLAIM_FEE2_DISCRIMINATOR, METEORA_DLMM_PROGRAM_ID};

use super::accounts::{
    derive_bin_array_pair, empty_transfer_hook_remaining_accounts_info, EMPTY_TRANSFER_HOOK_RAI_LEN,
};

/// All `AccountInfo`s required by `claim_fee2`. The vault PDA acts as the
/// `sender` (signer) — its seeds are passed in `vault_signer_seeds`.
#[allow(clippy::too_many_arguments)]
pub struct ClaimFee2Cpi<'info> {
    pub dlmm_program: AccountInfo<'info>,
    pub lb_pair: AccountInfo<'info>,
    pub position: AccountInfo<'info>,
    pub vault: AccountInfo<'info>,
    pub reserve_x: AccountInfo<'info>,
    pub reserve_y: AccountInfo<'info>,
    pub vault_token_x: AccountInfo<'info>,
    pub vault_token_y: AccountInfo<'info>,
    pub token_x_mint: AccountInfo<'info>,
    pub token_y_mint: AccountInfo<'info>,
    pub token_program_x: AccountInfo<'info>,
    pub token_program_y: AccountInfo<'info>,
    pub memo_program: AccountInfo<'info>,
    pub event_authority: AccountInfo<'info>,
    pub bin_array_lower: AccountInfo<'info>,
    pub bin_array_upper: AccountInfo<'info>,
}

/// Build & invoke `claim_fee2(min_bin_id, max_bin_id)`.
///
/// `vault_signer_seeds` is the vault PDA seed slice including the bump,
/// e.g. `[b"vault", user.as_ref(), pool.as_ref(), &[bump]]`.
pub fn invoke_claim_fee2(
    cpi: &ClaimFee2Cpi<'_>,
    min_bin_id: i32,
    max_bin_id: i32,
    vault_signer_seeds: &[&[u8]],
) -> Result<()> {
    require_keys_eq!(cpi.dlmm_program.key(), METEORA_DLMM_PROGRAM_ID);

    // Sanity-check the BinArray PDAs we were handed match the position range.
    let (expected_lower_ba, expected_upper_ba) =
        derive_bin_array_pair(&cpi.lb_pair.key(), min_bin_id, &METEORA_DLMM_PROGRAM_ID);
    require_keys_eq!(cpi.bin_array_lower.key(), expected_lower_ba);
    require_keys_eq!(cpi.bin_array_upper.key(), expected_upper_ba);

    // ── Borsh args ───────────────────────────────────────────────────────────
    let mut data = Vec::with_capacity(8 + 4 + 4 + EMPTY_TRANSFER_HOOK_RAI_LEN);
    data.extend_from_slice(&METEORA_CLAIM_FEE2_DISCRIMINATOR);
    data.extend_from_slice(&min_bin_id.to_le_bytes());
    data.extend_from_slice(&max_bin_id.to_le_bytes());
    let rai = empty_transfer_hook_remaining_accounts_info();
    data.extend_from_slice(&rai[..EMPTY_TRANSFER_HOOK_RAI_LEN]);

    let metas = vec![
        AccountMeta::new(cpi.lb_pair.key(), false),
        AccountMeta::new(cpi.position.key(), false),
        AccountMeta::new(cpi.vault.key(), true), // signer = vault PDA
        AccountMeta::new(cpi.reserve_x.key(), false),
        AccountMeta::new(cpi.reserve_y.key(), false),
        AccountMeta::new(cpi.vault_token_x.key(), false),
        AccountMeta::new(cpi.vault_token_y.key(), false),
        AccountMeta::new_readonly(cpi.token_x_mint.key(), false),
        AccountMeta::new_readonly(cpi.token_y_mint.key(), false),
        AccountMeta::new_readonly(cpi.token_program_x.key(), false),
        AccountMeta::new_readonly(cpi.token_program_y.key(), false),
        AccountMeta::new_readonly(cpi.memo_program.key(), false),
        AccountMeta::new_readonly(cpi.event_authority.key(), false),
        AccountMeta::new_readonly(cpi.dlmm_program.key(), false),
        AccountMeta::new(cpi.bin_array_lower.key(), false),
        AccountMeta::new(cpi.bin_array_upper.key(), false),
    ];

    let ix = Instruction {
        program_id: METEORA_DLMM_PROGRAM_ID,
        accounts: metas,
        data,
    };

    let infos = [
        cpi.lb_pair.clone(),
        cpi.position.clone(),
        cpi.vault.clone(),
        cpi.reserve_x.clone(),
        cpi.reserve_y.clone(),
        cpi.vault_token_x.clone(),
        cpi.vault_token_y.clone(),
        cpi.token_x_mint.clone(),
        cpi.token_y_mint.clone(),
        cpi.token_program_x.clone(),
        cpi.token_program_y.clone(),
        cpi.memo_program.clone(),
        cpi.event_authority.clone(),
        cpi.dlmm_program.clone(),
        cpi.bin_array_lower.clone(),
        cpi.bin_array_upper.clone(),
    ];

    invoke_signed(&ix, &infos, &[vault_signer_seeds]).map_err(Into::into)
}
