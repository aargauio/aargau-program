//! Helpers shared by Meteora CPI builders.
//!
//! Meteora's `claim_fee2`, `add_liquidity_by_strategy2` and
//! `remove_liquidity_by_range2` all consume the same trailing Borsh blob:
//!
//! ```text
//! remaining_accounts_info {
//!     slices: Vec<{ accounts_type: u8, length: u8 }>
//! }
//! ```
//!
//! For SPL-classic mints (no transfer hooks) the blob is the same 11-byte
//! tail every time, factored out below.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::pubkey::Pubkey;

use crate::constants::METEORA_BINS_PER_ARRAY;

/// Borsh-serialised `RemainingAccountsInfo` with two empty slices
/// (`TransferHookX` = variant 0, `TransferHookY` = variant 1).
///
/// Layout (11 bytes):
///   `slices.len()` as `u32 LE` = 2
///   slice[0]: type = 0, len = 0
///   slice[1]: type = 1, len = 0
pub fn empty_transfer_hook_remaining_accounts_info() -> [u8; 11] {
    let mut bytes = [0u8; 11];
    // u32 LE = 2
    bytes[0..4].copy_from_slice(&2u32.to_le_bytes());
    bytes[4] = 0; // TransferHookX variant tag
    bytes[5] = 0; // length
    bytes[6] = 1; // TransferHookY variant tag
    bytes[7] = 0; // length
                  // Trailing 3 zero bytes are not part of the schema; callers must use
                  // [..EMPTY_TRANSFER_HOOK_RAI_LEN] to slice the actual payload. Kept as
                  // a fixed-size array purely as a stack-allocated scratch buffer.
    bytes
}

/// Returns the actual byte length of the empty transfer-hook payload (8).
pub const EMPTY_TRANSFER_HOOK_RAI_LEN: usize = 8;

/// Derive the `[bin_array_index, bin_array_index + 1]` pair of `BinArray` PDAs
/// covering the bin range starting at `lower_bin_id`.
///
/// Meteora's CPIs always expect exactly two consecutive `BinArray` accounts
/// per chunk regardless of the chunk's upper bound — this matches the SDK
/// helper `get_bin_array_indexes_bound_by_chunk` (always `upper = lower + 1`).
///
/// Returns `(lower_bin_array_pda, upper_bin_array_pda)`.
pub fn derive_bin_array_pair(
    lb_pair: &Pubkey,
    lower_bin_id: i32,
    program_id: &Pubkey,
) -> (Pubkey, Pubkey) {
    let lower_idx: i64 = (lower_bin_id as i64).div_euclid(METEORA_BINS_PER_ARRAY);
    let upper_idx: i64 = lower_idx + 1;

    let lower_idx_bytes = lower_idx.to_le_bytes();
    let upper_idx_bytes = upper_idx.to_le_bytes();
    let (lower_pda, _) = Pubkey::find_program_address(
        &[b"bin_array", lb_pair.as_ref(), &lower_idx_bytes],
        program_id,
    );
    let (upper_pda, _) = Pubkey::find_program_address(
        &[b"bin_array", lb_pair.as_ref(), &upper_idx_bytes],
        program_id,
    );
    (lower_pda, upper_pda)
}
