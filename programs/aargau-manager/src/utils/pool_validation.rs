use crate::{constants::*, errors::AargauError, state::Protocol};
use anchor_lang::prelude::*;

/// Validate the `pool` UncheckedAccount in 4 layers:
///
/// 1. Owner check — pool.owner must equal the expected protocol program ID
/// 2. Discriminator check — first 8 bytes of data match expected value
/// 3. Mint check — mints embedded in pool data match mint_a / mint_b accounts
/// 4. NonTransferable extension check on both mints
///
/// Only layers 1 and 2 are enforced here; layers 3 and 4 are delegated to
/// the handler which has access to the mint AccountInfo objects.
pub fn validate_pool_owner_and_discriminator(pool: &AccountInfo, protocol: Protocol) -> Result<()> {
    // Layer 1: owner check
    let expected_owner = match protocol {
        Protocol::Orca => &ORCA_WHIRLPOOL_PROGRAM_ID,
        Protocol::Raydium => &RAYDIUM_CLMM_PROGRAM_ID,
        Protocol::Meteora => &METEORA_DLMM_PROGRAM_ID,
    };
    require!(pool.owner == expected_owner, AargauError::InvalidPool);

    // Layer 2: discriminator check
    let data = pool.try_borrow_data()?;
    require!(data.len() >= 8, AargauError::InvalidPool);
    let disc = &data[0..8];
    let expected_disc = match protocol {
        Protocol::Orca => &ORCA_WHIRLPOOL_DISCRIMINATOR,
        Protocol::Raydium => &RAYDIUM_POOL_STATE_DISCRIMINATOR,
        Protocol::Meteora => &METEORA_LB_PAIR_DISCRIMINATOR,
    };
    require!(disc == expected_disc, AargauError::InvalidPoolDiscriminator);

    Ok(())
}

/// Resolve a mint offset constant to an absolute index into the raw account
/// data buffer.
///
/// Orca/Raydium offsets are stored as post-discriminator (relative to byte 8);
/// Meteora offsets are stored as absolute. The asymmetry is documented next
/// to the constants in `constants.rs`.
fn absolute_mint_offset(protocol: Protocol, offset: usize) -> usize {
    match protocol {
        Protocol::Orca | Protocol::Raydium => 8 + offset,
        Protocol::Meteora => offset,
    }
}

/// Read the mint_a Pubkey from raw pool account data (including the 8-byte
/// Anchor discriminator). Returns an error if data is too short.
pub fn read_mint_a(data: &[u8], protocol: Protocol) -> Result<Pubkey> {
    let offset = match protocol {
        Protocol::Orca => ORCA_MINT_A_OFFSET,
        Protocol::Raydium => RAYDIUM_MINT_A_OFFSET,
        Protocol::Meteora => METEORA_MINT_A_OFFSET,
    };
    let raw_offset = absolute_mint_offset(protocol, offset);
    require!(data.len() >= raw_offset + 32, AargauError::InvalidPool);
    let bytes: [u8; 32] = data[raw_offset..raw_offset + 32]
        .try_into()
        .map_err(|_| AargauError::InvalidPool)?;
    Ok(Pubkey::from(bytes))
}

/// Read the mint_b Pubkey from raw pool account data (including the 8-byte
/// Anchor discriminator). Returns an error if data is too short.
pub fn read_mint_b(data: &[u8], protocol: Protocol) -> Result<Pubkey> {
    let offset = match protocol {
        Protocol::Orca => ORCA_MINT_B_OFFSET,
        Protocol::Raydium => RAYDIUM_MINT_B_OFFSET,
        Protocol::Meteora => METEORA_MINT_B_OFFSET,
    };
    let raw_offset = absolute_mint_offset(protocol, offset);
    require!(data.len() >= raw_offset + 32, AargauError::InvalidPool);
    let bytes: [u8; 32] = data[raw_offset..raw_offset + 32]
        .try_into()
        .map_err(|_| AargauError::InvalidPool)?;
    Ok(Pubkey::from(bytes))
}

/// Check that a mint does NOT have the Token-2022 NonTransferable extension.
/// This extension would prevent the program from moving tokens on behalf of users.
///
/// Token-2022 mint layout:
///   [0..82]   Base mint data (MintState)
///   [82..165] Padding / reserved (83 bytes)
///   [165]     AccountType discriminator (1 byte; 2 = Mint with extensions)
///   [166..]   Extension TLV area: each entry = [type: u16 LE][length: u16 LE][data: length bytes]
///
/// NonTransferable extension: type = 9 (u16 LE → [0x09, 0x00]), length = 0 ([0x00, 0x00]).
///
/// IMPORTANT: this parser walks extension entries at their correct 4-byte-header boundaries
/// to avoid false positives from [9, 0] appearing inside other extension data fields.
/// A false positive rejects a valid pool (usability bug). A false negative admits a
/// non-transferable mint (CPI would fail anyway — correctness not affected, but defense
/// is broken). Proper boundary walking prevents both.
pub fn check_no_non_transferable_extension(mint_info: &AccountInfo) -> Result<()> {
    let data = mint_info.try_borrow_data()?;
    // Classic SPL Token mints are exactly 82 bytes — no extensions possible.
    if data.len() <= 165 {
        return Ok(());
    }
    // Byte 165 is the AccountType discriminator; extensions start at byte 166.
    // Walk TLV entries: [type: u16 LE][length: u16 LE][data: length bytes]
    let mut cursor = 166usize;
    while cursor + 4 <= data.len() {
        let ext_type = u16::from_le_bytes([data[cursor], data[cursor + 1]]);
        let ext_len = u16::from_le_bytes([data[cursor + 2], data[cursor + 3]]) as usize;
        cursor += 4;
        // ExtensionType::NonTransferable == 9
        if ext_type == 9 {
            return err!(AargauError::NonTransferableMint);
        }
        // Advance past this extension's data; guard against malformed data
        cursor = cursor.saturating_add(ext_len);
    }
    Ok(())
}
