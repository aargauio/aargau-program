//! Tests for `src/utils/account_bytes.rs` — bounds-checked LE readers.

mod account_bytes_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::account_bytes::{
        read_bytes, read_i32_le, read_pubkey, read_u128_le, read_u16_le, read_u64_le,
    };
    use anchor_lang::prelude::Pubkey;

    #[test]
    fn reads_little_endian_values_at_offset() {
        let mut data = vec![0u8; 64];
        data[1..3].copy_from_slice(&0xBEEFu16.to_le_bytes());
        data[3..7].copy_from_slice(&(-42i32).to_le_bytes());
        data[7..15].copy_from_slice(&u64::MAX.to_le_bytes());
        data[15..31].copy_from_slice(&(u128::MAX - 1).to_le_bytes());
        data[31..63].copy_from_slice(&[0x7Au8; 32]);

        let err = AargauError::InvalidPool;
        assert_eq!(read_u16_le(&data, 1, err).unwrap(), 0xBEEF);
        assert_eq!(read_i32_le(&data, 3, err).unwrap(), -42);
        assert_eq!(read_u64_le(&data, 7, err).unwrap(), u64::MAX);
        assert_eq!(read_u128_le(&data, 15, err).unwrap(), u128::MAX - 1);
        assert_eq!(
            read_pubkey(&data, 31, err).unwrap(),
            Pubkey::new_from_array([0x7A; 32])
        );
    }

    #[test]
    fn read_ending_exactly_at_buffer_end_succeeds() {
        let data = [1u8, 2, 3, 4];
        assert_eq!(
            read_bytes::<4>(&data, 0, AargauError::InvalidPool).unwrap(),
            [1, 2, 3, 4]
        );
    }

    #[test]
    fn read_past_end_returns_caller_error() {
        let data = [0u8; 8];
        let err = read_u64_le(&data, 1, AargauError::InvalidTokenAccount).unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidTokenAccount)
        );
    }

    #[test]
    fn offset_overflow_returns_caller_error_instead_of_panicking() {
        let data = [0u8; 8];
        let err = read_pubkey(&data, usize::MAX, AargauError::InvalidPool).unwrap_err();
        assert_eq!(err_code(&err), aargau_err_code(AargauError::InvalidPool));
    }

    #[test]
    fn empty_buffer_is_rejected() {
        let err = read_u16_le(&[], 0, AargauError::InvalidPool).unwrap_err();
        assert_eq!(err_code(&err), aargau_err_code(AargauError::InvalidPool));
    }
}
