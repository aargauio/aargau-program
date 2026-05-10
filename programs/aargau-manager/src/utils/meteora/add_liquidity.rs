//! Raw CPI builder for Meteora DLMM `add_liquidity_by_strategy2`.
//!
//! Account layout (mirrors `aargau-backend/.../meteora/txs/add.rs`):
//!
//! | idx | role                        | flags        |
//! |----:|-----------------------------|--------------|
//! |  0  | position                    | writable     |
//! |  1  | lb_pair                     | writable     |
//! |  2  | bin_array_bitmap_extension  | readonly     |  ← program_id placeholder
//! |  3  | user_token_x                | writable     |  ← vault ATA A
//! |  4  | user_token_y                | writable     |  ← vault ATA B
//! |  5  | reserve_x                   | writable     |
//! |  6  | reserve_y                   | writable     |
//! |  7  | token_x_mint                | readonly     |
//! |  8  | token_y_mint                | readonly     |
//! |  9  | sender                      | signer + w   |  ← vault PDA
//! | 10  | token_program_x             | readonly     |
//! | 11  | token_program_y             | readonly     |
//! | 12  | event_authority             | readonly     |
//! | 13  | dlmm_program                | readonly     |
//! | 14  | bin_array_lower             | writable     |
//! | 15  | bin_array_upper             | writable     |
//!
//! Args (Borsh):
//! ```text
//! discriminator: [u8; 8]
//! amount_x: u64
//! amount_y: u64
//! active_id: i32
//! max_active_bin_slippage: i32
//! strategy_parameters {
//!     min_bin_id: i32
//!     max_bin_id: i32
//!     strategy_type: u8
//!     parameteres: [u8; 64]
//! }
//! remaining_accounts_info: { slices: [{0,0},{1,0}] }
//! ```

use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};

use crate::constants::{
    MAX_ACTIVE_BIN_SLIPPAGE, METEORA_ADD_LIQUIDITY_BY_STRATEGY2_DISCRIMINATOR,
    METEORA_DLMM_PROGRAM_ID,
};

use super::accounts::{
    derive_bin_array_pair, empty_transfer_hook_remaining_accounts_info, EMPTY_TRANSFER_HOOK_RAI_LEN,
};

/// Meteora DLMM strategy types (StrategyParameters.strategy_type).
/// `SpotImBalanced` (6) is the canonical choice for asymmetric ranges; it is
/// what the backend uses for keeper-driven adds.
pub const METEORA_STRATEGY_SPOT_IMBALANCED: u8 = 6;

#[allow(clippy::too_many_arguments)]
pub struct AddLiquidityByStrategy2Cpi<'info> {
    pub dlmm_program: AccountInfo<'info>,
    pub position: AccountInfo<'info>,
    pub lb_pair: AccountInfo<'info>,
    /// `bin_array_bitmap_extension` placeholder — must equal `dlmm_program` when
    /// the pool's bitmap fits in the LbPair itself (the common case).
    pub bin_array_bitmap_extension: AccountInfo<'info>,
    pub vault: AccountInfo<'info>,
    pub vault_token_x: AccountInfo<'info>,
    pub vault_token_y: AccountInfo<'info>,
    pub reserve_x: AccountInfo<'info>,
    pub reserve_y: AccountInfo<'info>,
    pub token_x_mint: AccountInfo<'info>,
    pub token_y_mint: AccountInfo<'info>,
    pub token_program_x: AccountInfo<'info>,
    pub token_program_y: AccountInfo<'info>,
    pub event_authority: AccountInfo<'info>,
    pub bin_array_lower: AccountInfo<'info>,
    pub bin_array_upper: AccountInfo<'info>,
}

#[allow(clippy::too_many_arguments)]
pub fn invoke_add_liquidity_by_strategy2(
    cpi: &AddLiquidityByStrategy2Cpi<'_>,
    amount_x: u64,
    amount_y: u64,
    active_id: i32,
    min_bin_id: i32,
    max_bin_id: i32,
    strategy_type: u8,
    vault_signer_seeds: &[&[u8]],
) -> Result<()> {
    require_keys_eq!(cpi.dlmm_program.key(), METEORA_DLMM_PROGRAM_ID);

    let (expected_lower_ba, expected_upper_ba) =
        derive_bin_array_pair(&cpi.lb_pair.key(), min_bin_id, &METEORA_DLMM_PROGRAM_ID);
    require_keys_eq!(cpi.bin_array_lower.key(), expected_lower_ba);
    require_keys_eq!(cpi.bin_array_upper.key(), expected_upper_ba);

    // ── Borsh args ───────────────────────────────────────────────────────────
    // 8 disc + 8 amount_x + 8 amount_y + 4 active_id + 4 slippage
    // + 4 min_bin + 4 max_bin + 1 strategy_type + 64 strategy_params + 8 RAI
    let mut data =
        Vec::with_capacity(8 + 8 + 8 + 4 + 4 + 4 + 4 + 1 + 64 + EMPTY_TRANSFER_HOOK_RAI_LEN);
    data.extend_from_slice(&METEORA_ADD_LIQUIDITY_BY_STRATEGY2_DISCRIMINATOR);
    data.extend_from_slice(&amount_x.to_le_bytes());
    data.extend_from_slice(&amount_y.to_le_bytes());
    data.extend_from_slice(&active_id.to_le_bytes());
    data.extend_from_slice(&MAX_ACTIVE_BIN_SLIPPAGE.to_le_bytes());
    data.extend_from_slice(&min_bin_id.to_le_bytes());
    data.extend_from_slice(&max_bin_id.to_le_bytes());
    data.push(strategy_type);
    data.extend_from_slice(&[0u8; 64]);
    let rai = empty_transfer_hook_remaining_accounts_info();
    data.extend_from_slice(&rai[..EMPTY_TRANSFER_HOOK_RAI_LEN]);

    let metas = vec![
        AccountMeta::new(cpi.position.key(), false),
        AccountMeta::new(cpi.lb_pair.key(), false),
        AccountMeta::new_readonly(cpi.bin_array_bitmap_extension.key(), false),
        AccountMeta::new(cpi.vault_token_x.key(), false),
        AccountMeta::new(cpi.vault_token_y.key(), false),
        AccountMeta::new(cpi.reserve_x.key(), false),
        AccountMeta::new(cpi.reserve_y.key(), false),
        AccountMeta::new_readonly(cpi.token_x_mint.key(), false),
        AccountMeta::new_readonly(cpi.token_y_mint.key(), false),
        AccountMeta::new(cpi.vault.key(), true), // signer = vault PDA
        AccountMeta::new_readonly(cpi.token_program_x.key(), false),
        AccountMeta::new_readonly(cpi.token_program_y.key(), false),
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
        cpi.position.clone(),
        cpi.lb_pair.clone(),
        cpi.bin_array_bitmap_extension.clone(),
        cpi.vault_token_x.clone(),
        cpi.vault_token_y.clone(),
        cpi.reserve_x.clone(),
        cpi.reserve_y.clone(),
        cpi.token_x_mint.clone(),
        cpi.token_y_mint.clone(),
        cpi.vault.clone(),
        cpi.token_program_x.clone(),
        cpi.token_program_y.clone(),
        cpi.event_authority.clone(),
        cpi.dlmm_program.clone(),
        cpi.bin_array_lower.clone(),
        cpi.bin_array_upper.clone(),
    ];

    invoke_signed(&ix, &infos, &[vault_signer_seeds]).map_err(Into::into)
}
