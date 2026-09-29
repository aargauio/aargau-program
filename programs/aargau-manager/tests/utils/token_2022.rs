//! Tests for `src/utils/token_2022.rs` — per-mint token-program detection,
//! the Token-2022 transfer-fee reader and the v2-transferable mint gate.

mod token_program_detection_tests {
    use aargau_manager::constants::TOKEN_2022_PROGRAM_ID;
    use aargau_manager::utils::token_2022::is_token_2022;
    use anchor_lang::prelude::Pubkey;

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

mod transfer_fee_reader_tests {
    use aargau_manager::utils::token_2022::read_transfer_fee_config;

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

mod v2_transferable_gate_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::token_2022::require_v2_transferable_mint;

    // Build a Token-2022 mint buffer (166-byte base) with a single extension
    // of the given type and a zero-length body.
    fn mint_with_extension(ext_type: u16) -> Vec<u8> {
        let mut data = vec![0u8; 166];
        data.extend_from_slice(&ext_type.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data
    }

    #[test]
    fn classic_spl_mint_passes() {
        // Classic SPL mint = 82 bytes, no extension area.
        assert!(require_v2_transferable_mint(&vec![0u8; 82]).is_ok());
    }

    #[test]
    fn transfer_fee_only_mint_passes() {
        // TransferFeeConfig (type 1) is the supported Token-2022 extension.
        let body_len = 108usize;
        let mut data = vec![0u8; 166];
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&(body_len as u16).to_le_bytes());
        data.extend_from_slice(&vec![0u8; body_len]);
        assert!(require_v2_transferable_mint(&data).is_ok());
    }

    #[test]
    fn transfer_hook_mint_is_rejected() {
        // TransferHook = type 14.
        let data = mint_with_extension(14);
        let err = require_v2_transferable_mint(&data).unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::Token2022NotSupported),
        );
    }

    #[test]
    fn non_transferable_mint_is_rejected() {
        // NonTransferable = type 9.
        let data = mint_with_extension(9);
        let err = require_v2_transferable_mint(&data).unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::NonTransferableMint),
        );
    }

    #[test]
    fn transfer_hook_after_a_preceding_extension_is_rejected() {
        // TransferFeeConfig (skipped) then TransferHook — the cursor must walk
        // past the first extension and still catch the hook.
        let body_len = 108usize;
        let mut data = vec![0u8; 166];
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&(body_len as u16).to_le_bytes());
        data.extend_from_slice(&vec![0u8; body_len]);
        data.extend_from_slice(&14u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        let err = require_v2_transferable_mint(&data).unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::Token2022NotSupported),
        );
    }

    #[test]
    fn trailing_zero_padding_after_base_is_treated_as_terminator() {
        // 166-byte base + a block of zero bytes: extension-type 0
        // (`Uninitialized`) is the TLV terminator, so the walk stops cleanly
        // instead of over-scanning padding. A mint with no real extension is
        // transferable.
        let mut data = vec![0u8; 166];
        data.extend_from_slice(&vec![0u8; 64]);
        assert!(require_v2_transferable_mint(&data).is_ok());
    }

    #[test]
    fn extension_after_a_zero_terminator_is_not_reached() {
        // Canonical TLV semantics: a type-0 terminator stops the walk. Bytes
        // after it are trailing padding, not a parsable extension — so a hook
        // accidentally encoded past the terminator is not read. (Real
        // Token-2022 mints never place a live extension after the terminator;
        // this pins that the walk stops at type 0 rather than scanning on.)
        let mut data = vec![0u8; 166];
        data.extend_from_slice(&0u16.to_le_bytes()); // terminator
        data.extend_from_slice(&0u16.to_le_bytes()); // (len)
        data.extend_from_slice(&14u16.to_le_bytes()); // TransferHook past terminator
        data.extend_from_slice(&0u16.to_le_bytes());
        assert!(require_v2_transferable_mint(&data).is_ok());
    }

    // The Orca `CollectFees` reward loop (`collect_rewards`) is best-effort:
    // before issuing `collect_reward_v2` for a Token-2022 reward mint it runs
    // this same predicate and SKIPS the slot (emitting `RewardCollectionSkipped`)
    // when it returns `Err`, instead of letting the CPI revert the whole tx.
    // These two tests pin the exact skip/collect partition that loop relies on:
    // incompatible reward mints are skipped, compatible ones are collected.
    #[test]
    fn reward_skip_predicate_rejects_incompatible_reward_mints() {
        // A reward mint with a TransferHook or NonTransferable extension would
        // revert `collect_reward_v2` (serialised with no remaining-accounts
        // info) — the loop skips these slots.
        assert!(require_v2_transferable_mint(&mint_with_extension(14)).is_err());
        assert!(require_v2_transferable_mint(&mint_with_extension(9)).is_err());
    }

    #[test]
    fn reward_skip_predicate_collects_compatible_reward_mints() {
        // Classic SPL and transfer-fee-only Token-2022 reward mints are
        // serviceable by the v2 CPI — the loop collects these slots normally.
        let body_len = 108usize;
        let mut fee_only = vec![0u8; 166];
        fee_only.extend_from_slice(&1u16.to_le_bytes());
        fee_only.extend_from_slice(&(body_len as u16).to_le_bytes());
        fee_only.extend_from_slice(&vec![0u8; body_len]);
        assert!(require_v2_transferable_mint(&vec![0u8; 82]).is_ok());
        assert!(require_v2_transferable_mint(&fee_only).is_ok());
    }
}
