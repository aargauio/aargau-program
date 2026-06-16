//! Tests for `src/utils/orca/accounts.rs` — tick-array math, PDA derivation,
//! per-mint token-program detection and the Token-2022 transfer-fee reader.
//!
//! These pin the wire-critical derivations: a wrong `floor_div` for negative
//! ticks or LE-vs-ASCII tick-array seed would silently yield a different PDA
//! and the CPI would fail against the deployed Whirlpools program.

mod tick_array_math_tests {
    use aargau_manager::constants::{TICK_ARRAY_SIZE, TOKEN_2022_PROGRAM_ID};
    use aargau_manager::utils::orca::accounts::{floor_div, is_token_2022, tick_array_start_index};
    use anchor_lang::prelude::Pubkey;

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

    #[test]
    fn is_token_2022_matches_only_the_token_2022_program() {
        assert!(is_token_2022(&TOKEN_2022_PROGRAM_ID));
        let spl_token: Pubkey = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
            .parse()
            .unwrap();
        assert!(!is_token_2022(&spl_token));
        assert!(!is_token_2022(&Pubkey::default()));
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

mod transfer_fee_reader_tests {
    use aargau_manager::utils::orca::accounts::read_transfer_fee_config;

    #[test]
    fn classic_spl_mint_has_no_config() {
        // Classic SPL mint = 82 bytes, no extension area.
        let data = vec![0u8; 82];
        assert!(read_transfer_fee_config(&data).is_none());
    }

    #[test]
    fn token_2022_without_transfer_fee_extension_returns_none() {
        // 166 bytes base + a NonTransferable (type 9, len 0) extension only.
        let mut data = vec![0u8; 166];
        data.push(9); // type LE low
        data.push(0); // type LE high
        data.push(0); // len LE low
        data.push(0); // len LE high
        assert!(read_transfer_fee_config(&data).is_none());
    }

    #[test]
    fn reads_newer_transfer_fee_bps_and_max() {
        // Build a Token-2022 mint with a TransferFeeConfig extension (type 1).
        // Body layout (relevant fields):
        //   [98..106]  newer.maximum_fee: u64
        //   [106..108] newer.transfer_fee_basis_points: u16
        // TransferFeeConfig body is 108 bytes total.
        let body_len = 108usize;
        let mut body = vec![0u8; body_len];
        let max_fee: u64 = 12_345;
        let bps: u16 = 250;
        body[98..106].copy_from_slice(&max_fee.to_le_bytes());
        body[106..108].copy_from_slice(&bps.to_le_bytes());

        let mut data = vec![0u8; 166];
        data.push(1); // ext type = 1 (TransferFeeConfig), LE low
        data.push(0); // LE high
        data.extend_from_slice(&(body_len as u16).to_le_bytes());
        data.extend_from_slice(&body);

        let cfg = read_transfer_fee_config(&data).expect("should parse config");
        assert_eq!(cfg.transfer_fee_bps, bps);
        assert_eq!(cfg.maximum_fee, max_fee);
    }

    #[test]
    fn skips_a_preceding_extension_before_the_transfer_fee_config() {
        // First a NonTransferable (type 9, len 0), then TransferFeeConfig.
        let body_len = 108usize;
        let mut body = vec![0u8; body_len];
        let max_fee: u64 = 7;
        let bps: u16 = 33;
        body[98..106].copy_from_slice(&max_fee.to_le_bytes());
        body[106..108].copy_from_slice(&bps.to_le_bytes());

        let mut data = vec![0u8; 166];
        // NonTransferable extension (skipped).
        data.extend_from_slice(&9u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        // TransferFeeConfig.
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&(body_len as u16).to_le_bytes());
        data.extend_from_slice(&body);

        let cfg = read_transfer_fee_config(&data).expect("should parse config");
        assert_eq!(cfg.transfer_fee_bps, bps);
        assert_eq!(cfg.maximum_fee, max_fee);
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
