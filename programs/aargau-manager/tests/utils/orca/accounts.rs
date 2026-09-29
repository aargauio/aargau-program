//! Tests for `src/utils/orca/accounts.rs` — tick-array math, PDA derivation
//! and the `Position` discriminator guard.
//!
//! These pin the wire-critical derivations: a wrong `floor_div` for negative
//! ticks or LE-vs-ASCII tick-array seed would silently yield a different PDA
//! and the CPI would fail against the deployed Whirlpools program.

mod tick_array_math_tests {
    use aargau_manager::constants::TICK_ARRAY_SIZE;
    use aargau_manager::utils::orca::accounts::{floor_div, tick_array_start_index};

    #[test]
    fn floor_div_rounds_toward_negative_infinity() {
        assert_eq!(floor_div(0, 88), 0);
        assert_eq!(floor_div(87, 88), 0);
        assert_eq!(floor_div(88, 88), 1);
        // Truncating `/` would give 0 here; floor must give -1.
        assert_eq!(floor_div(-1, 88), -1);
        assert_eq!(floor_div(-88, 88), -1);
        assert_eq!(floor_div(-89, 88), -2);
    }

    #[test]
    fn start_index_for_tick_spacing_one() {
        // span = 88 * 1 = 88.
        assert_eq!(tick_array_start_index(0, 1).unwrap(), 0);
        assert_eq!(tick_array_start_index(87, 1).unwrap(), 0);
        assert_eq!(tick_array_start_index(88, 1).unwrap(), 88);
        assert_eq!(tick_array_start_index(-1, 1).unwrap(), -88);
    }

    #[test]
    fn start_index_for_tick_spacing_sixtyfour() {
        // span = 88 * 64 = 5632.
        let span = TICK_ARRAY_SIZE * 64;
        assert_eq!(span, 5632);
        assert_eq!(tick_array_start_index(0, 64).unwrap(), 0);
        assert_eq!(tick_array_start_index(5631, 64).unwrap(), 0);
        assert_eq!(tick_array_start_index(5632, 64).unwrap(), 5632);
        // Negative tick lands in the array starting one full span below zero.
        assert_eq!(tick_array_start_index(-1, 64).unwrap(), -5632);
    }

    #[test]
    fn zero_tick_spacing_is_rejected() {
        assert!(tick_array_start_index(0, 0).is_err());
    }
}

mod pda_derivation_tests {
    use aargau_manager::constants::ORCA_WHIRLPOOL_PROGRAM_ID;
    use aargau_manager::utils::orca::accounts::{derive_position_pda, derive_tick_array_pda};
    use anchor_lang::prelude::Pubkey;

    fn fixed(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    #[test]
    fn position_pda_uses_position_seed_and_mint() {
        let mint = fixed(0x11);
        let expected =
            Pubkey::find_program_address(&[b"position", mint.as_ref()], &ORCA_WHIRLPOOL_PROGRAM_ID)
                .0;
        assert_eq!(derive_position_pda(&mint), expected);
    }

    #[test]
    fn tick_array_pda_uses_ascii_decimal_start_index() {
        // The third seed is the ASCII decimal string of the start index, NOT
        // its little-endian bytes. Reconstruct with the string form to prove
        // the derivation matches.
        let whirlpool = fixed(0x22);
        let start: i32 = -5632;
        let expected = Pubkey::find_program_address(
            &[
                b"tick_array",
                whirlpool.as_ref(),
                start.to_string().as_bytes(),
            ],
            &ORCA_WHIRLPOOL_PROGRAM_ID,
        )
        .0;
        assert_eq!(derive_tick_array_pda(&whirlpool, start), expected);

        // Sanity: the LE-bytes form would derive a DIFFERENT address. Guards
        // against an accidental regression to numeric seeds.
        let le_form = Pubkey::find_program_address(
            &[b"tick_array", whirlpool.as_ref(), &start.to_le_bytes()],
            &ORCA_WHIRLPOOL_PROGRAM_ID,
        )
        .0;
        assert_ne!(derive_tick_array_pda(&whirlpool, start), le_form);
    }
}

mod position_discriminator_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::constants::ORCA_POSITION_DISCRIMINATOR;
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::orca::accounts::validate_position_discriminator;

    #[test]
    fn accepts_correct_discriminator() {
        let mut data = ORCA_POSITION_DISCRIMINATOR.to_vec();
        data.extend_from_slice(&[0u8; 32]);
        assert!(validate_position_discriminator(&data).is_ok());
    }

    #[test]
    fn rejects_wrong_discriminator() {
        let data = vec![0u8; 64];
        let err = validate_position_discriminator(&data).unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidPositionDiscriminator),
        );
    }

    #[test]
    fn rejects_short_buffer() {
        let data = vec![0u8; 4];
        assert!(validate_position_discriminator(&data).is_err());
    }
}
