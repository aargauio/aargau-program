//! Tests for `src/utils/token_account.rs` — raw SPL / Token-2022 token-account
//! reader used for pool-vault balances and the position-NFT frozen state.

mod token_account_view_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::constants::{SPL_TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID};
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::token_account::{
        is_token_program, parse_token_account_view_from_bytes, read_token_account_view,
    };
    use anchor_lang::prelude::{AccountInfo, Pubkey};

    const STATE_UNINITIALIZED: u8 = 0;
    const STATE_INITIALIZED: u8 = 1;
    const STATE_FROZEN: u8 = 2;

    fn fixed(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    /// A 165-byte SPL token account.
    fn token_account(amount: u64, state: u8) -> Vec<u8> {
        let mut data = vec![0u8; 165];
        data[0..32].copy_from_slice(fixed(0xA1).as_ref());
        data[32..64].copy_from_slice(fixed(0xB2).as_ref());
        data[64..72].copy_from_slice(&amount.to_le_bytes());
        data[108] = state;
        data
    }

    /// A Token-2022 token account with one extension (ImmutableOwner-sized).
    fn extended_token_account(amount: u64, state: u8, account_type: u8) -> Vec<u8> {
        let mut data = token_account(amount, state);
        data.push(account_type);
        data.extend_from_slice(&[7, 0, 0, 0]); // ImmutableOwner TLV, len 0
        data
    }

    fn assert_invalid_token_account(result: anchor_lang::Result<impl std::fmt::Debug>) {
        let err = result.unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidTokenAccount)
        );
    }

    #[test]
    fn parses_initialized_spl_account() {
        let view =
            parse_token_account_view_from_bytes(&token_account(1234, STATE_INITIALIZED)).unwrap();
        assert_eq!(view.mint, fixed(0xA1));
        assert_eq!(view.authority, fixed(0xB2));
        assert_eq!(view.amount, 1234);
        assert!(!view.is_frozen);
    }

    #[test]
    fn reports_frozen_state() {
        let view = parse_token_account_view_from_bytes(&token_account(1, STATE_FROZEN)).unwrap();
        assert!(view.is_frozen);
        assert_eq!(view.amount, 1);
    }

    #[test]
    fn amount_edges_zero_and_max() {
        for amount in [0u64, 1, u64::MAX] {
            let view =
                parse_token_account_view_from_bytes(&token_account(amount, STATE_INITIALIZED))
                    .unwrap();
            assert_eq!(view.amount, amount);
        }
    }

    #[test]
    fn parses_extended_token_2022_account() {
        let data = extended_token_account(55, STATE_FROZEN, 2);
        let view = parse_token_account_view_from_bytes(&data).unwrap();
        assert_eq!(view.amount, 55);
        assert!(view.is_frozen);
    }

    #[test]
    fn rejects_extended_mint() {
        // AccountType 1 = Mint: an extended Token-2022 mint is not a token account.
        let data = extended_token_account(55, STATE_INITIALIZED, 1);
        assert_invalid_token_account(parse_token_account_view_from_bytes(&data));
    }

    #[test]
    fn rejects_uninitialized_account() {
        let data = token_account(0, STATE_UNINITIALIZED);
        assert_invalid_token_account(parse_token_account_view_from_bytes(&data));
    }

    #[test]
    fn rejects_unknown_state_byte() {
        let data = token_account(0, 3);
        assert_invalid_token_account(parse_token_account_view_from_bytes(&data));
    }

    #[test]
    fn rejects_short_buffer_such_as_a_classic_mint() {
        assert_invalid_token_account(parse_token_account_view_from_bytes(&[0u8; 82]));
        assert_invalid_token_account(parse_token_account_view_from_bytes(&[0u8; 164]));
    }

    #[test]
    fn rejects_multisig_length() {
        let mut data = token_account(0, STATE_INITIALIZED);
        data.resize(355, 0);
        data[165] = 2;
        assert_invalid_token_account(parse_token_account_view_from_bytes(&data));
    }

    #[test]
    fn token_program_predicate() {
        assert!(is_token_program(&SPL_TOKEN_PROGRAM_ID));
        assert!(is_token_program(&TOKEN_2022_PROGRAM_ID));
        assert!(!is_token_program(&fixed(0x01)));
        assert!(!is_token_program(&Pubkey::default()));
    }

    #[test]
    fn account_info_reader_accepts_spl_owner() {
        let key = fixed(0x10);
        let mut lamports = 0u64;
        let mut data = token_account(9, STATE_INITIALIZED);

        let spl_owned = SPL_TOKEN_PROGRAM_ID;
        let info = AccountInfo::new(
            &key,
            false,
            false,
            &mut lamports,
            &mut data,
            &spl_owned,
            false,
        );
        assert_eq!(read_token_account_view(&info).unwrap().amount, 9);
    }

    #[test]
    fn account_info_reader_accepts_token_2022_owner() {
        let key = fixed(0x11);
        let mut lamports = 0u64;
        let mut data = extended_token_account(10, STATE_INITIALIZED, 2);
        let owner = TOKEN_2022_PROGRAM_ID;
        let info = AccountInfo::new(&key, false, false, &mut lamports, &mut data, &owner, false);
        assert_eq!(read_token_account_view(&info).unwrap().amount, 10);
    }

    #[test]
    fn account_info_reader_rejects_foreign_owner() {
        // Right bytes, wrong program: a forged account must not be trusted.
        let key = fixed(0x12);
        let mut lamports = 0u64;
        let mut data = token_account(9, STATE_INITIALIZED);
        let forged_owner = fixed(0xEE);
        let info = AccountInfo::new(
            &key,
            false,
            false,
            &mut lamports,
            &mut data,
            &forged_owner,
            false,
        );
        assert_invalid_token_account(read_token_account_view(&info));
    }
}
