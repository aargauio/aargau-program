//! Token-2022 mint inspection shared by every protocol integration that
//! accepts Token-2022 pair or reward mints.
//!
//! Pure byte-level readers over raw mint account data, so they can be
//! exercised directly from the integration test crate without an SVM runtime.

use anchor_lang::prelude::*;

use crate::constants::TOKEN_2022_PROGRAM_ID;
use crate::errors::AargauError;

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
const TRANSFER_FEE_OLDER_MAX_FEE_OFFSET: usize = 80;
const TRANSFER_FEE_OLDER_BPS_OFFSET: usize = 88;
const TRANSFER_FEE_NEWER_EPOCH_OFFSET: usize = 90;
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
//                          append no transfer-hook accounts (Orca serialises
//                          `remaining_accounts_info = None`), so the hook
//                          program is never invoked and the token program
//                          rejects the transfer.
const NON_TRANSFERABLE_EXTENSION_TYPE: u16 = 9;
const TRANSFER_HOOK_EXTENSION_TYPE: u16 = 14;

/// Read the Token-2022 `TransferFeeConfig` (newer epoch fee) for a mint, if
/// present. SPL classic mints (≤165 bytes) and Token-2022 mints without the
/// extension return `None`. Never panics on malformed input.
///
/// The vault snapshots these values on `OpenPosition` so the fee-transfer
/// path can account for the deduction the token program applies on transfer.
pub fn read_transfer_fee_config(mint_data: &[u8]) -> Option<TransferFeeSnapshot> {
    let body = find_transfer_fee_config_body(mint_data)?;
    read_transfer_fee_at(
        body,
        TRANSFER_FEE_NEWER_MAX_FEE_OFFSET,
        TRANSFER_FEE_NEWER_BPS_OFFSET,
    )
}

/// Read the transfer fee Token-2022 charges on a transfer made at `epoch`:
/// the newer fee once `epoch >= newer.epoch`, the older one before that
/// (Token-2022 `TransferFeeConfig::get_epoch_fee`). SPL classic mints and
/// Token-2022 mints without the extension return `None`. Never panics on
/// malformed input.
///
/// Unlike [`read_transfer_fee_config`] (a conservative snapshot), this is the
/// exact fee applied inside the current transaction, so callers can convert a
/// gross transfer amount into what the recipient actually received.
pub fn read_epoch_transfer_fee(mint_data: &[u8], epoch: u64) -> Option<TransferFeeSnapshot> {
    let body = find_transfer_fee_config_body(mint_data)?;
    let newer_epoch = read_body_u64(body, TRANSFER_FEE_NEWER_EPOCH_OFFSET)?;
    if epoch >= newer_epoch {
        read_transfer_fee_at(
            body,
            TRANSFER_FEE_NEWER_MAX_FEE_OFFSET,
            TRANSFER_FEE_NEWER_BPS_OFFSET,
        )
    } else {
        read_transfer_fee_at(
            body,
            TRANSFER_FEE_OLDER_MAX_FEE_OFFSET,
            TRANSFER_FEE_OLDER_BPS_OFFSET,
        )
    }
}

/// Body of the first `TransferFeeConfig` TLV entry, if the mint carries one
/// and it fits inside the buffer.
fn find_transfer_fee_config_body(mint_data: &[u8]) -> Option<&[u8]> {
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
            return mint_data.get(data_start..data_end);
        }
        cursor = data_end;
    }
    None
}

/// One `TransferFee` entry (maximum fee + basis points) at the given body
/// offsets; `None` when the body is too short.
fn read_transfer_fee_at(
    body: &[u8],
    max_fee_offset: usize,
    bps_offset: usize,
) -> Option<TransferFeeSnapshot> {
    let bps_bytes = body.get(bps_offset..bps_offset.checked_add(2)?)?;
    Some(TransferFeeSnapshot {
        transfer_fee_bps: u16::from_le_bytes(bps_bytes.try_into().ok()?),
        maximum_fee: read_body_u64(body, max_fee_offset)?,
    })
}

fn read_body_u64(body: &[u8], offset: usize) -> Option<u64> {
    let bytes = body.get(offset..offset.checked_add(8)?)?;
    Some(u64::from_le_bytes(bytes.try_into().ok()?))
}

/// Account-facing form of [`require_v2_transferable_mint`]: SPL classic mints
/// pass without reading their data.
pub fn require_v2_transferable_mint_account(mint: &AccountInfo<'_>) -> Result<()> {
    if !is_token_2022(mint.owner) {
        return Ok(());
    }
    let data = mint.try_borrow_data()?;
    require_v2_transferable_mint(&data)
}

/// Account-facing form of [`read_transfer_fee_config`]; zero fee when the
/// mint has no `TransferFeeConfig`.
pub fn read_transfer_fee_snapshot(mint: &AccountInfo<'_>) -> Result<TransferFeeSnapshot> {
    let data = mint.try_borrow_data()?;
    Ok(read_transfer_fee_config(&data).unwrap_or_default())
}

/// Reject a mint that carries a Token-2022 extension incompatible with the v2
/// transfer path the vault uses for the protocols' `_v2` decrease-liquidity,
/// fee-collection and close CPIs (e.g. Orca `decrease_liquidity_v2` /
/// `collect_fees_v2` / `close_position`).
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
