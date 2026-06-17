//! Raw CPI builder for Meteora DLMM `rebalance_liquidity`.
//!
//! ### What it does
//!
//! `rebalance_liquidity` is an **atomic single-CPI** that closes the
//! position's old bin range and re-opens it across a new range, all inside
//! one Meteora handler invocation. On-chain traces of automated DLMM
//! managers (e.g. Hawksight) show this is the dominant rebalance primitive
//! on Meteora.
//!
//! For the Aargau vault this matters because Meteora is the **only** of the
//! three integrated DEXes that supports an atomic rebalance: Orca and
//! Raydium force a 2-tx (close+open) flow because the position NFT mint
//! changes. Meteora reuses the same `PositionV2` account, so there is no
//! NFT lifecycle to manage and no `PendingRebalance` state to write.
//!
//! ### Pre-condition: claim fees first
//!
//! The vault always runs `claim_fee2` *before* `rebalance_liquidity` and
//! routes the performance fee split to the treasury at that point. Passing
//! `should_claim_fee = true` here would re-credit the user with the fees we
//! already extracted (Meteora deposits the claimed fees into the position
//! owner's ATAs, which is the vault), effectively giving the user a free
//! second harvest of the same fees. The constant
//! `METEORA_REBALANCE_SHOULD_CLAIM_FEE` pins the value to `false` at every
//! call site; the helper takes it as a parameter only so the call site can
//! make the intent visible in code review.
//!
//! ### Account layout (best-effort, single source of truth pending)
//!
//! Most wallet-mode Meteora tooling does **not** implement
//! `rebalance_liquidity` — it composes
//! `remove_liquidity_by_range2 + add_liquidity_by_strategy2` as two separate
//! txs. The account list below is derived from the union of those two
//! instructions plus the public DLMM IDL:
//!
//! | idx | role                       | flags        |
//! |----:|----------------------------|--------------|
//! |  0  | position                   | writable     |
//! |  1  | lb_pair                    | writable     |
//! |  2  | bin_array_bitmap_extension | readonly     |  ← program_id placeholder
//! |  3  | user_token_x               | writable     |  ← vault ATA A
//! |  4  | user_token_y               | writable     |  ← vault ATA B
//! |  5  | reserve_x                  | writable     |
//! |  6  | reserve_y                  | writable     |
//! |  7  | token_x_mint               | readonly     |
//! |  8  | token_y_mint               | readonly     |
//! |  9  | sender (vault PDA)         | signer + w   |
//! | 10  | token_program_x            | readonly     |
//! | 11  | token_program_y            | readonly     |
//! | 12  | memo_program               | readonly     |
//! | 13  | event_authority            | readonly     |
//! | 14  | dlmm_program               | readonly     |
//! | 15  | bin_array_lower (old)      | writable     |
//! | 16  | bin_array_upper (old)      | writable     |
//! | 17  | bin_array_lower (new)      | writable     |
//! | 18  | bin_array_upper (new)      | writable     |
//!
//! The old and new bin-array pairs may overlap when the new range partially
//! covers the old one. Deduplication of overlapping `BinArray` PDAs is
//! **caller responsibility** in S4 — this helper only validates that the
//! lower PDAs derive from the matching lower bin ids.
//!
//! > **Verification gate.** This account list and the arg schema below were
//! > assembled from secondary sources (Hawksight on-chain traces, internal
//! > spec table). They MUST be cross-checked against a live `RebalanceLiquidity`
//! > transaction on devnet before this helper is wired into a user-callable
//! > instruction. The first integration test in S5 (mollusk-svm) will catch
//! > any divergence by replaying a known-good tx.
//!
//! ### Args (Borsh, best-effort)
//! ```text
//! discriminator: [u8; 8]            // METEORA_REBALANCE_LIQUIDITY_DISCRIMINATOR
//! new_lower_bin_id: i32
//! new_upper_bin_id: i32
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
//! should_claim_fee: bool            // ALWAYS false (see module doc)
//! remaining_accounts_info: { slices: [{0,0},{1,0}] }   // SPL-classic only
//! ```

use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};

use crate::constants::{METEORA_DLMM_PROGRAM_ID, METEORA_REBALANCE_LIQUIDITY_DISCRIMINATOR};

use super::accounts::{
    derive_bin_array_pair, empty_transfer_hook_remaining_accounts_info, EMPTY_TRANSFER_HOOK_RAI_LEN,
};

/// All `AccountInfo`s required by `rebalance_liquidity`. See module doc for
/// the per-slot semantics. Both BinArray pairs (old + new) are passed
/// explicitly; the caller is responsible for deduplicating them if the new
/// range overlaps the old one.
#[allow(clippy::too_many_arguments)]
pub struct RebalanceLiquidityCpi<'info> {
    pub dlmm_program: AccountInfo<'info>,
    pub position: AccountInfo<'info>,
    pub lb_pair: AccountInfo<'info>,
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
    pub memo_program: AccountInfo<'info>,
    pub event_authority: AccountInfo<'info>,
    pub bin_array_lower_old: AccountInfo<'info>,
    pub bin_array_upper_old: AccountInfo<'info>,
    pub bin_array_lower_new: AccountInfo<'info>,
    pub bin_array_upper_new: AccountInfo<'info>,
}

/// Arguments to `rebalance_liquidity`. Grouped in a struct because the
/// Borsh body has nine scalar fields plus a strategy sub-struct — passing
/// them as positional CPI args would be unreadable.
pub struct RebalanceLiquidityArgs {
    pub old_lower_bin_id: i32,
    pub new_lower_bin_id: i32,
    pub new_upper_bin_id: i32,
    pub amount_x: u64,
    pub amount_y: u64,
    pub active_id: i32,
    pub max_active_bin_slippage: i32,
    pub strategy_type: u8,
    /// MUST be `METEORA_REBALANCE_SHOULD_CLAIM_FEE` (= `false`) at every
    /// production call site. Exposed as a parameter so the constant is
    /// passed by name at the call, not buried inside the helper.
    pub should_claim_fee: bool,
}

/// Pure constructor for the raw `rebalance_liquidity` instruction bytes.
/// Extracted from `invoke_rebalance_liquidity` so the integration test crate
/// can assert the Borsh body byte-for-byte (the helper is the only place
/// the layout is encoded — there is no widely-published reference tx
/// builder for `rebalance_liquidity` to cross-validate against).
pub fn build_rebalance_liquidity_instruction_data(args: &RebalanceLiquidityArgs) -> Vec<u8> {
    let mut data = Vec::with_capacity(
        8 + 4 + 4 + 8 + 8 + 4 + 4 + 4 + 4 + 1 + 64 + 1 + EMPTY_TRANSFER_HOOK_RAI_LEN,
    );
    data.extend_from_slice(&METEORA_REBALANCE_LIQUIDITY_DISCRIMINATOR);
    data.extend_from_slice(&args.new_lower_bin_id.to_le_bytes());
    data.extend_from_slice(&args.new_upper_bin_id.to_le_bytes());
    data.extend_from_slice(&args.amount_x.to_le_bytes());
    data.extend_from_slice(&args.amount_y.to_le_bytes());
    data.extend_from_slice(&args.active_id.to_le_bytes());
    data.extend_from_slice(&args.max_active_bin_slippage.to_le_bytes());
    // strategy_parameters
    data.extend_from_slice(&args.new_lower_bin_id.to_le_bytes()); // min_bin_id
    data.extend_from_slice(&args.new_upper_bin_id.to_le_bytes()); // max_bin_id
    data.push(args.strategy_type);
    data.extend_from_slice(&[0u8; 64]);
    data.push(u8::from(args.should_claim_fee));
    let rai = empty_transfer_hook_remaining_accounts_info();
    data.extend_from_slice(&rai[..EMPTY_TRANSFER_HOOK_RAI_LEN]);
    data
}

/// Pure constructor for the raw `rebalance_liquidity` account metas in the
/// vault-mode layout documented at the top of this module. Exposed for the
/// same reason as the data builder.
///
/// The four BinArray slots (lower_old, upper_old, lower_new, upper_new) are
/// deduplicated before being appended. When the new range partially overlaps
/// the old one the caller passes the same PDA in multiple slots. Solana's
/// runtime rejects a CPI instruction where the same writable account key
/// appears more than once in the `accounts` list (`DuplicateAccountOutOfSync`).
/// Deduplication preserves slot-order priority: lower_old is always first,
/// then upper_old (if distinct), then lower_new and upper_new (each only if
/// not already present).
#[allow(clippy::too_many_arguments)]
pub fn build_rebalance_liquidity_account_metas(
    position: &Pubkey,
    lb_pair: &Pubkey,
    bin_array_bitmap_extension: &Pubkey,
    vault_token_x: &Pubkey,
    vault_token_y: &Pubkey,
    reserve_x: &Pubkey,
    reserve_y: &Pubkey,
    token_x_mint: &Pubkey,
    token_y_mint: &Pubkey,
    vault: &Pubkey,
    token_program_x: &Pubkey,
    token_program_y: &Pubkey,
    memo_program: &Pubkey,
    event_authority: &Pubkey,
    dlmm_program: &Pubkey,
    bin_array_lower_old: &Pubkey,
    bin_array_upper_old: &Pubkey,
    bin_array_lower_new: &Pubkey,
    bin_array_upper_new: &Pubkey,
) -> Vec<AccountMeta> {
    let mut metas = vec![
        AccountMeta::new(*position, false),
        AccountMeta::new(*lb_pair, false),
        AccountMeta::new_readonly(*bin_array_bitmap_extension, false),
        AccountMeta::new(*vault_token_x, false),
        AccountMeta::new(*vault_token_y, false),
        AccountMeta::new(*reserve_x, false),
        AccountMeta::new(*reserve_y, false),
        AccountMeta::new_readonly(*token_x_mint, false),
        AccountMeta::new_readonly(*token_y_mint, false),
        AccountMeta::new(*vault, true), // signer = vault PDA
        AccountMeta::new_readonly(*token_program_x, false),
        AccountMeta::new_readonly(*token_program_y, false),
        AccountMeta::new_readonly(*memo_program, false),
        AccountMeta::new_readonly(*event_authority, false),
        AccountMeta::new_readonly(*dlmm_program, false),
    ];

    // Dedup the four BinArray slots. Each unique PDA is pushed once in
    // slot-priority order: lower_old → upper_old → lower_new → upper_new.
    let mut seen: Vec<Pubkey> = Vec::with_capacity(4);
    for key in [
        bin_array_lower_old,
        bin_array_upper_old,
        bin_array_lower_new,
        bin_array_upper_new,
    ] {
        if !seen.contains(key) {
            seen.push(*key);
            metas.push(AccountMeta::new(*key, false));
        }
    }

    metas
}

/// Build & invoke `rebalance_liquidity`. `vault_signer_seeds` is the vault
/// PDA seed slice including the bump.
pub fn invoke_rebalance_liquidity(
    cpi: &RebalanceLiquidityCpi<'_>,
    args: &RebalanceLiquidityArgs,
    vault_signer_seeds: &[&[u8]],
) -> Result<()> {
    require_keys_eq!(cpi.dlmm_program.key(), METEORA_DLMM_PROGRAM_ID);

    // Sanity-check BinArray PDAs derive from the bin ids the caller declared.
    // We can't easily dedupe overlapping ranges here without also computing
    // the indices, so we just check that the *lower* PDA in each pair is
    // anchored at the right bin id; the helper trusts the caller for the
    // upper PDA (which is `lower_idx + 1` by Meteora convention — see
    // `derive_bin_array_pair`).
    let (expected_lower_old_ba, expected_upper_old_ba) = derive_bin_array_pair(
        &cpi.lb_pair.key(),
        args.old_lower_bin_id,
        &METEORA_DLMM_PROGRAM_ID,
    );
    require_keys_eq!(cpi.bin_array_lower_old.key(), expected_lower_old_ba);
    require_keys_eq!(cpi.bin_array_upper_old.key(), expected_upper_old_ba);

    let (expected_lower_new_ba, expected_upper_new_ba) = derive_bin_array_pair(
        &cpi.lb_pair.key(),
        args.new_lower_bin_id,
        &METEORA_DLMM_PROGRAM_ID,
    );
    require_keys_eq!(cpi.bin_array_lower_new.key(), expected_lower_new_ba);
    require_keys_eq!(cpi.bin_array_upper_new.key(), expected_upper_new_ba);

    let data = build_rebalance_liquidity_instruction_data(args);

    let metas = build_rebalance_liquidity_account_metas(
        &cpi.position.key(),
        &cpi.lb_pair.key(),
        &cpi.bin_array_bitmap_extension.key(),
        &cpi.vault_token_x.key(),
        &cpi.vault_token_y.key(),
        &cpi.reserve_x.key(),
        &cpi.reserve_y.key(),
        &cpi.token_x_mint.key(),
        &cpi.token_y_mint.key(),
        &cpi.vault.key(),
        &cpi.token_program_x.key(),
        &cpi.token_program_y.key(),
        &cpi.memo_program.key(),
        &cpi.event_authority.key(),
        &cpi.dlmm_program.key(),
        &cpi.bin_array_lower_old.key(),
        &cpi.bin_array_upper_old.key(),
        &cpi.bin_array_lower_new.key(),
        &cpi.bin_array_upper_new.key(),
    );

    let ix = Instruction {
        program_id: METEORA_DLMM_PROGRAM_ID,
        accounts: metas,
        data,
    };

    let mut infos = vec![
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
        cpi.memo_program.clone(),
        cpi.event_authority.clone(),
        cpi.dlmm_program.clone(),
    ];

    // Mirror the same BinArray dedup applied in `build_rebalance_liquidity_account_metas`
    // so the `account_infos` slice length and order match the `accounts` meta list exactly.
    let ba_candidates = [
        &cpi.bin_array_lower_old,
        &cpi.bin_array_upper_old,
        &cpi.bin_array_lower_new,
        &cpi.bin_array_upper_new,
    ];
    let mut seen_ba: Vec<Pubkey> = Vec::with_capacity(4);
    for ai in ba_candidates {
        if !seen_ba.contains(&ai.key()) {
            seen_ba.push(ai.key());
            infos.push(ai.clone());
        }
    }

    invoke_signed(&ix, &infos, &[vault_signer_seeds]).map_err(Into::into)
}
