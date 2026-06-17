//! Address derivation and per-mint token-program detection shared by the
//! Orca CPI builders.
//!
//! Extracted into a standalone module (mirroring `utils/meteora/accounts.rs`)
//! so the pure helpers can be exercised directly from the integration test
//! crate without an SVM runtime.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::pubkey::Pubkey;

use crate::constants::{
    ORCA_POSITION_DISCRIMINATOR, ORCA_POSITION_SEED, ORCA_TICK_ARRAY_SEED,
    ORCA_WHIRLPOOL_PROGRAM_ID, TICK_ARRAY_SIZE, TOKEN_2022_PROGRAM_ID,
};
use crate::errors::AargauError;

/// True floor division toward negative infinity.
///
/// Rust's `/` truncates toward zero, which yields the wrong `TickArray`
/// start index for negative ticks (e.g. `-1 / 88 == 0`, but the array
/// covering tick `-1` starts at `-88`). The Whirlpools program computes the
/// start index with floor division, so we must match it exactly. Mirrors the
/// backend `tx/helpers.rs::floor_div`.
pub fn floor_div(a: i32, b: i32) -> i32 {
    let quotient = a / b;
    let remainder = a % b;
    if (remainder != 0) && ((remainder < 0) != (b < 0)) {
        quotient - 1
    } else {
        quotient
    }
}

/// Start tick index of the `TickArray` covering `tick` for the given
/// `tick_spacing`.
///
/// `start = floor_div(tick, TICK_ARRAY_SIZE * tick_spacing) * (TICK_ARRAY_SIZE * tick_spacing)`.
///
/// Returns `Err` when `tick_spacing == 0` (would divide by zero).
pub fn tick_array_start_index(tick: i32, tick_spacing: u16) -> Result<i32> {
    let span = TICK_ARRAY_SIZE
        .checked_mul(i32::from(tick_spacing))
        .ok_or(AargauError::Overflow)?;
    require!(span != 0, AargauError::DivisionByZero);
    let start = floor_div(tick, span)
        .checked_mul(span)
        .ok_or(AargauError::Overflow)?;
    Ok(start)
}

/// Derive the `TickArray` PDA for a whirlpool and a start tick index.
///
/// Seeds: `[b"tick_array", whirlpool, start_tick_index.to_string().as_bytes()]`.
/// The third seed is the **ASCII decimal string** of the start index — not
/// its little-endian bytes. Getting this wrong silently yields a different
/// (uninitialised) PDA and the CPI fails downstream, so the binding is
/// validated against the caller-supplied account.
pub fn derive_tick_array_pda(whirlpool: &Pubkey, start_tick_index: i32) -> Pubkey {
    let start_str = start_tick_index.to_string();
    let (pda, _) = Pubkey::find_program_address(
        &[
            ORCA_TICK_ARRAY_SEED,
            whirlpool.as_ref(),
            start_str.as_bytes(),
        ],
        &ORCA_WHIRLPOOL_PROGRAM_ID,
    );
    pda
}

/// Derive the Orca `Position` PDA from the position NFT mint.
///
/// Seeds: `[b"position", position_mint]`.
pub fn derive_position_pda(position_mint: &Pubkey) -> Pubkey {
    let (pda, _) = Pubkey::find_program_address(
        &[ORCA_POSITION_SEED, position_mint.as_ref()],
        &ORCA_WHIRLPOOL_PROGRAM_ID,
    );
    pda
}

/// Pure byte-level validation that `data` is an Orca `Position` account.
/// Certifies only the account *type* (discriminator) — full layout parsing
/// is out of scope. Never panics on malformed input.
pub fn validate_position_discriminator(data: &[u8]) -> Result<()> {
    require!(
        data.len() >= ORCA_POSITION_DISCRIMINATOR.len(),
        AargauError::InvalidPositionDiscriminator
    );
    require!(
        data[..8] == ORCA_POSITION_DISCRIMINATOR,
        AargauError::InvalidPositionDiscriminator
    );
    Ok(())
}

/// `AccountInfo`-facing guard: the `position` account must be owned by the
/// Whirlpools program AND carry the `Position` discriminator. The key-equality
/// check against `vault.position_address` alone is necessary but not
/// sufficient — an account closed outside the vault could otherwise slip
/// through. Mirrors `meteora::require_position_v2`.
pub fn require_orca_position(account: &AccountInfo<'_>) -> Result<()> {
    require_keys_eq!(
        *account.owner,
        ORCA_WHIRLPOOL_PROGRAM_ID,
        AargauError::InvalidPool
    );
    let data = account.try_borrow_data()?;
    validate_position_discriminator(&data)
}

/// Custody bind for Orca reward collection: the destination that
/// `collect_reward_v2` pays into must be the vault PDA's own associated token
/// account for the reward mint. The destination is supplied via
/// `remaining_accounts` (which Anchor does not validate), so without this check
/// a caller could route rewards into an arbitrary account it controls.
///
/// Uses the program-id-aware ATA derivation so an SPL-classic and a Token-2022
/// reward mint each resolve to their correct ATA. Returns `InvalidRewardOwner`
/// on mismatch.
pub fn require_reward_owner_is_vault_ata(
    reward_owner_account: &Pubkey,
    vault: &Pubkey,
    reward_mint: &Pubkey,
    reward_token_program: &Pubkey,
) -> Result<()> {
    let expected = anchor_spl::associated_token::get_associated_token_address_with_program_id(
        vault,
        reward_mint,
        reward_token_program,
    );
    require_keys_eq!(
        *reward_owner_account,
        expected,
        AargauError::InvalidRewardOwner
    );
    Ok(())
}

/// Returns `true` when a mint account is owned by the Token-2022 program.
///
/// Per-mint detection drives which token program is forwarded to each `_v2`
/// CPI and whether a transfer-fee config is snapshotted on the vault. SPL
/// classic mints return `false`.
pub fn is_token_2022(mint_owner: &Pubkey) -> bool {
    *mint_owner == TOKEN_2022_PROGRAM_ID
}

/// Parsed Token-2022 `TransferFeeConfig` snapshot for a single mint.
///
/// Only the fields the vault persists are surfaced: the *newer* transfer-fee
/// (the one applied to transfers at the current epoch is bounded by it, and
/// snapshotting the newer config is conservative for fee accounting).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TransferFeeSnapshot {
    pub transfer_fee_bps: u16,
    pub maximum_fee: u64,
}

// Token-2022 mint extension layout (matches `check_no_non_transferable_extension`):
//   [0..82]    base MintState
//   [82..165]  padding / reserved
//   [165]      AccountType discriminator (2 = Mint with extensions)
//   [166..]    TLV: [type: u16 LE][length: u16 LE][data: length bytes]
//
// TransferFeeConfig extension: type == 1. Its data layout (relevant tail):
//   [0..32]   transfer_fee_config_authority (COption<Pubkey> packed as 32 bytes)
//   [32..64]  withdraw_withheld_authority
//   [64..72]  withheld_amount: u64
//   ── older TransferFee ──
//   [72..80]   older.epoch: u64
//   [80..88]   older.maximum_fee: u64
//   [88..90]   older.transfer_fee_basis_points: u16
//   ── newer TransferFee ──
//   [90..98]   newer.epoch: u64
//   [98..106]  newer.maximum_fee: u64
//   [106..108] newer.transfer_fee_basis_points: u16
const MINT_EXTENSION_TLV_START: usize = 166;
const TRANSFER_FEE_CONFIG_EXTENSION_TYPE: u16 = 1;
const TRANSFER_FEE_NEWER_MAX_FEE_OFFSET: usize = 98;
const TRANSFER_FEE_NEWER_BPS_OFFSET: usize = 106;

// Token-2022 mint-side extension type IDs (SPL Token-2022 `ExtensionType`,
// numbered from 0 = Uninitialized). Verified against the spl-token-2022
// `ExtensionType` enum ordering. Only the two that block the v2 transfer path
// are relevant here:
//   NonTransferable = 9  → the token program rejects every transfer, so the
//                          position could be opened but never decreased,
//                          fee-collected, or closed (funds locked).
//   TransferHook    = 14 → the transfer requires extra accounts; our v2 CPIs
//                          serialise `remaining_accounts_info = None` and
//                          append nothing, so the hook program is never
//                          invoked and the token program rejects the transfer.
const NON_TRANSFERABLE_EXTENSION_TYPE: u16 = 9;
const TRANSFER_HOOK_EXTENSION_TYPE: u16 = 14;

/// Read the Token-2022 `TransferFeeConfig` (newer epoch fee) for a mint, if
/// present. SPL classic mints (≤165 bytes) and Token-2022 mints without the
/// extension return `None`. Never panics on malformed input.
///
/// The vault snapshots these values on `OpenPosition` so the fee-transfer
/// path can account for the deduction the token program applies on transfer.
pub fn read_transfer_fee_config(mint_data: &[u8]) -> Option<TransferFeeSnapshot> {
    if mint_data.len() <= 165 {
        return None;
    }
    let mut cursor = MINT_EXTENSION_TLV_START;
    while cursor + 4 <= mint_data.len() {
        let ext_type = u16::from_le_bytes([mint_data[cursor], mint_data[cursor + 1]]);
        // `ExtensionType::Uninitialized` (0) is the TLV terminator in the
        // canonical spl-token-2022 reader: trailing account padding reads back
        // as zero type+length, so stop here instead of over-scanning it.
        if ext_type == 0 {
            break;
        }
        let ext_len = u16::from_le_bytes([mint_data[cursor + 2], mint_data[cursor + 3]]) as usize;
        let data_start = cursor + 4;
        let data_end = data_start.checked_add(ext_len)?;
        if data_end > mint_data.len() {
            return None;
        }
        if ext_type == TRANSFER_FEE_CONFIG_EXTENSION_TYPE {
            let body = &mint_data[data_start..data_end];
            let max_fee_end = TRANSFER_FEE_NEWER_MAX_FEE_OFFSET + 8;
            let bps_end = TRANSFER_FEE_NEWER_BPS_OFFSET + 2;
            if body.len() < bps_end {
                return None;
            }
            let maximum_fee = u64::from_le_bytes(
                body[TRANSFER_FEE_NEWER_MAX_FEE_OFFSET..max_fee_end]
                    .try_into()
                    .ok()?,
            );
            let transfer_fee_bps = u16::from_le_bytes(
                body[TRANSFER_FEE_NEWER_BPS_OFFSET..bps_end]
                    .try_into()
                    .ok()?,
            );
            return Some(TransferFeeSnapshot {
                transfer_fee_bps,
                maximum_fee,
            });
        }
        cursor = data_end;
    }
    None
}

/// Reject a mint that carries a Token-2022 extension incompatible with the v2
/// transfer path the vault uses for `decrease_liquidity_v2` /
/// `collect_fees_v2` / `close_position`.
///
/// `NonTransferable` mints can never be moved at all; `TransferHook` mints
/// require extra accounts that our CPIs do not assemble
/// (`remaining_accounts_info = None`). Either would let a position be opened
/// but never wound down — locking funds. Enforced at `OpenPosition` so the
/// vault never enters that state.
///
/// SPL-classic mints (≤165 bytes, no TLV) and Token-2022 mints carrying only
/// supported extensions (e.g. transfer-fee-only) pass. Models the cursor walk
/// on `read_transfer_fee_config`: bounds-checked, monotonic, never panics.
pub fn require_v2_transferable_mint(mint_data: &[u8]) -> Result<()> {
    if mint_data.len() <= 165 {
        return Ok(());
    }
    let mut cursor = MINT_EXTENSION_TLV_START;
    while cursor + 4 <= mint_data.len() {
        let ext_type = u16::from_le_bytes([mint_data[cursor], mint_data[cursor + 1]]);
        // `ExtensionType::Uninitialized` (0) terminates the TLV region in the
        // canonical reader — trailing zero padding is not a real extension.
        if ext_type == 0 {
            break;
        }
        let ext_len = u16::from_le_bytes([mint_data[cursor + 2], mint_data[cursor + 3]]) as usize;
        let data_start = cursor + 4;
        let data_end = match data_start.checked_add(ext_len) {
            Some(end) => end,
            None => return Ok(()),
        };
        if data_end > mint_data.len() {
            return Ok(());
        }
        require!(
            ext_type != NON_TRANSFERABLE_EXTENSION_TYPE,
            AargauError::NonTransferableMint
        );
        require!(
            ext_type != TRANSFER_HOOK_EXTENSION_TYPE,
            AargauError::Token2022NotSupported
        );
        cursor = data_end;
    }
    Ok(())
}
