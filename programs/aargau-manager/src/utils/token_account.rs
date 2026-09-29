//! Raw reader for SPL Token and Token-2022 token accounts.
//!
//! Used where a token account is only reachable as an `UncheckedAccount`
//! (protocol pool vaults, position-NFT accounts) and the handler needs its
//! balance or frozen state. The owner program is checked first, then the
//! buffer is certified as a *token account* (not a mint) before any field is
//! read.
//!
//! Layout (identical for both programs; Token-2022 appends extensions):
//!   [0..32]    mint
//!   [32..64]   owner (authority)
//!   [64..72]   amount: u64
//!   [72..108]  delegate: COption<Pubkey>
//!   [108]      state: 0 = Uninitialized, 1 = Initialized, 2 = Frozen
//!   [109..165] is_native, delegated_amount, close_authority
//!   [165]      Token-2022 only, when extended: AccountType (1 = Mint, 2 = Account)
//!
//! A 355-byte buffer is a multisig, never a token account (both programs'
//! own unpackers reject it), so it is excluded before the type byte is read.

use anchor_lang::prelude::*;

use crate::constants::{SPL_TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID};
use crate::errors::AargauError;
use crate::utils::account_bytes::{read_pubkey, read_u64_le};

const TOKEN_ACCOUNT_BASE_LEN: usize = 165;
const MULTISIG_LEN: usize = 355;
const TOKEN_ACCOUNT_MINT_OFFSET: usize = 0;
const TOKEN_ACCOUNT_AUTHORITY_OFFSET: usize = 32;
const TOKEN_ACCOUNT_AMOUNT_OFFSET: usize = 64;
const TOKEN_ACCOUNT_STATE_OFFSET: usize = 108;
const TOKEN_ACCOUNT_TYPE_OFFSET: usize = 165;

const ACCOUNT_STATE_INITIALIZED: u8 = 1;
const ACCOUNT_STATE_FROZEN: u8 = 2;
const EXTENDED_ACCOUNT_TYPE_ACCOUNT: u8 = 2;

/// Fields read from a token account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenAccountView {
    pub mint: Pubkey,
    /// The token-level owner (transfer authority), not the owning program.
    pub authority: Pubkey,
    pub amount: u64,
    pub is_frozen: bool,
}

/// Returns `true` for the two token programs whose account layout this module
/// understands.
pub fn is_token_program(program_id: &Pubkey) -> bool {
    *program_id == SPL_TOKEN_PROGRAM_ID || *program_id == TOKEN_2022_PROGRAM_ID
}

/// Pure byte parser. Rejects buffers shorter than a token account, multisigs,
/// extended Token-2022 buffers whose `AccountType` is not `Account` (e.g. an
/// extended mint) and uninitialized accounts. Never panics on malformed input.
pub fn parse_token_account_view_from_bytes(data: &[u8]) -> Result<TokenAccountView> {
    require!(
        data.len() >= TOKEN_ACCOUNT_BASE_LEN,
        AargauError::InvalidTokenAccount
    );
    require!(data.len() != MULTISIG_LEN, AargauError::InvalidTokenAccount);
    if data.len() > TOKEN_ACCOUNT_BASE_LEN {
        require!(
            data.get(TOKEN_ACCOUNT_TYPE_OFFSET) == Some(&EXTENDED_ACCOUNT_TYPE_ACCOUNT),
            AargauError::InvalidTokenAccount
        );
    }

    let state = *data
        .get(TOKEN_ACCOUNT_STATE_OFFSET)
        .ok_or(AargauError::InvalidTokenAccount)?;
    require!(
        state == ACCOUNT_STATE_INITIALIZED || state == ACCOUNT_STATE_FROZEN,
        AargauError::InvalidTokenAccount
    );

    Ok(TokenAccountView {
        mint: read_pubkey(
            data,
            TOKEN_ACCOUNT_MINT_OFFSET,
            AargauError::InvalidTokenAccount,
        )?,
        authority: read_pubkey(
            data,
            TOKEN_ACCOUNT_AUTHORITY_OFFSET,
            AargauError::InvalidTokenAccount,
        )?,
        amount: read_u64_le(
            data,
            TOKEN_ACCOUNT_AMOUNT_OFFSET,
            AargauError::InvalidTokenAccount,
        )?,
        is_frozen: state == ACCOUNT_STATE_FROZEN,
    })
}

/// `AccountInfo`-facing reader: the account must be owned by SPL Token or
/// Token-2022 before its bytes are trusted.
pub fn read_token_account_view(account: &AccountInfo<'_>) -> Result<TokenAccountView> {
    require!(
        is_token_program(account.owner),
        AargauError::InvalidTokenAccount
    );
    let data = account.try_borrow_data()?;
    parse_token_account_view_from_bytes(&data)
}
