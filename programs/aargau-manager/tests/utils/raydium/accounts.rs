//! Tests for `src/utils/raydium/accounts.rs` — tick-array math and PDA
//! derivations.
//!
//! The tick-array seed is the wire-critical difference from Orca: Raydium
//! uses the big-endian i32 bytes of the start index. A wrong encoding
//! silently derives a different (uninitialised) PDA.

mod tick_array_math_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::raydium::accounts::tick_array_start_index;

    #[test]
    fn start_index_for_tick_spacing_one() {
        // span = 60 * 1 = 60.
        assert_eq!(tick_array_start_index(0, 1).unwrap(), 0);
        assert_eq!(tick_array_start_index(59, 1).unwrap(), 0);
        assert_eq!(tick_array_start_index(60, 1).unwrap(), 60);
        assert_eq!(tick_array_start_index(-1, 1).unwrap(), -60);
        assert_eq!(tick_array_start_index(-60, 1).unwrap(), -60);
        assert_eq!(tick_array_start_index(-61, 1).unwrap(), -120);
    }

    #[test]
    fn start_index_matches_mainnet_wsol_usdc_arrays() {
        // Pool 3ucNos4… (tick spacing 1), tick_current = -21161 lives in the
        // array starting at -21180; neighbours at -21240 and -21120.
        assert_eq!(tick_array_start_index(-21_161, 1).unwrap(), -21_180);
        assert_eq!(tick_array_start_index(-21_181, 1).unwrap(), -21_240);
        assert_eq!(tick_array_start_index(-21_120, 1).unwrap(), -21_120);
    }

    #[test]
    fn start_index_for_tick_spacing_ten() {
        // span = 600. USTM/USDT arrays at -39600 / -39000 / -38400.
        assert_eq!(tick_array_start_index(-39_000, 10).unwrap(), -39_000);
        assert_eq!(tick_array_start_index(-39_001, 10).unwrap(), -39_600);
        assert_eq!(tick_array_start_index(-38_401, 10).unwrap(), -39_000);
    }

    #[test]
    fn start_index_at_tick_bounds() {
        // Raydium MIN_TICK / MAX_TICK = ∓443636.
        assert_eq!(tick_array_start_index(443_636, 1).unwrap(), 443_580);
        assert_eq!(tick_array_start_index(-443_636, 1).unwrap(), -443_640);
    }

    #[test]
    fn start_index_overflow_is_an_error_not_a_panic() {
        // span = 60 * 65535; the floor of i32::MIN lands one span below
        // i32::MIN, so the multiplication overflows.
        assert!(tick_array_start_index(i32::MAX, u16::MAX).is_ok());
        let err = tick_array_start_index(i32::MIN, u16::MAX).unwrap_err();
        assert_eq!(err_code(&err), aargau_err_code(AargauError::Overflow));
    }

    #[test]
    fn zero_tick_spacing_is_rejected() {
        assert!(tick_array_start_index(0, 0).is_err());
    }
}

mod pda_derivation_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::constants::RAYDIUM_CLMM_PROGRAM_ID;
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::raydium::accounts::{
        derive_personal_position_pda, derive_tick_array_bitmap_extension_pda,
        derive_tick_array_pda, require_tick_array, require_tick_array_bitmap_extension,
    };
    use anchor_lang::prelude::Pubkey;
    use std::str::FromStr;

    fn fixed(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    #[test]
    fn personal_position_pda_uses_position_seed_and_mint() {
        let mint = fixed(0x11);
        let expected =
            Pubkey::find_program_address(&[b"position", mint.as_ref()], &RAYDIUM_CLMM_PROGRAM_ID).0;
        assert_eq!(derive_personal_position_pda(&mint), expected);
    }

    #[test]
    fn tick_array_pda_uses_big_endian_start_index() {
        let pool = fixed(0x22);
        let start: i32 = -21_180;
        let expected = Pubkey::find_program_address(
            &[b"tick_array", pool.as_ref(), &start.to_be_bytes()],
            &RAYDIUM_CLMM_PROGRAM_ID,
        )
        .0;
        assert_eq!(derive_tick_array_pda(&pool, start), expected);

        // Neither the LE form nor Orca's ASCII form may match.
        let le_form = Pubkey::find_program_address(
            &[b"tick_array", pool.as_ref(), &start.to_le_bytes()],
            &RAYDIUM_CLMM_PROGRAM_ID,
        )
        .0;
        let ascii_form = Pubkey::find_program_address(
            &[b"tick_array", pool.as_ref(), start.to_string().as_bytes()],
            &RAYDIUM_CLMM_PROGRAM_ID,
        )
        .0;
        assert_ne!(derive_tick_array_pda(&pool, start), le_form);
        assert_ne!(derive_tick_array_pda(&pool, start), ascii_form);
    }

    #[test]
    fn mainnet_wsol_usdc_derivations() {
        // Addresses recorded from mainnet for pool 3ucNos4…: its bitmap
        // extension and the tick array at start index -21180.
        let pool = Pubkey::from_str("3ucNos4NbumPLZNWztqGHNFFgkHeRMBQAVemeeomsUxv").unwrap();
        let bitmap = Pubkey::from_str("4NFvUKqknMpoe6CWTzK758B8ojVLzURL5pC6MtiaJ8TQ").unwrap();
        assert_eq!(derive_tick_array_bitmap_extension_pda(&pool), bitmap);
        let tick_array = derive_tick_array_pda(&pool, -21_180);
        assert!(tick_array.to_string().starts_with("GamghVsu"));
    }

    #[test]
    fn bitmap_extension_pda_uses_pool_seed() {
        let pool = fixed(0x33);
        let expected = Pubkey::find_program_address(
            &[b"pool_tick_array_bitmap_extension", pool.as_ref()],
            &RAYDIUM_CLMM_PROGRAM_ID,
        )
        .0;
        assert_eq!(derive_tick_array_bitmap_extension_pda(&pool), expected);
    }

    #[test]
    fn require_tick_array_accepts_covering_pda() {
        let pool = fixed(0x44);
        let passed = derive_tick_array_pda(&pool, -120);
        // tick -61 with spacing 1 → start -120.
        assert!(require_tick_array(&pool, -61, 1, &passed).is_ok());
    }

    #[test]
    fn require_tick_array_rejects_neighbour_array() {
        let pool = fixed(0x44);
        let neighbour = derive_tick_array_pda(&pool, -60);
        let err = require_tick_array(&pool, -61, 1, &neighbour).unwrap_err();
        assert_eq!(err_code(&err), aargau_err_code(AargauError::InvalidPool));
    }

    #[test]
    fn require_tick_array_rejects_other_pool() {
        let passed = derive_tick_array_pda(&fixed(0x55), 0);
        let err = require_tick_array(&fixed(0x44), 0, 1, &passed).unwrap_err();
        assert_eq!(err_code(&err), aargau_err_code(AargauError::InvalidPool));
    }

    #[test]
    fn require_tick_array_propagates_zero_spacing() {
        let err = require_tick_array(&fixed(0x44), 0, 0, &fixed(0x01)).unwrap_err();
        assert_eq!(err_code(&err), aargau_err_code(AargauError::DivisionByZero));
    }

    #[test]
    fn require_bitmap_extension_binds_to_pool() {
        let pool = fixed(0x66);
        let bitmap = derive_tick_array_bitmap_extension_pda(&pool);
        assert!(require_tick_array_bitmap_extension(&pool, &bitmap).is_ok());

        let other = derive_tick_array_bitmap_extension_pda(&fixed(0x67));
        let err = require_tick_array_bitmap_extension(&pool, &other).unwrap_err();
        assert_eq!(err_code(&err), aargau_err_code(AargauError::InvalidPool));
    }
}
