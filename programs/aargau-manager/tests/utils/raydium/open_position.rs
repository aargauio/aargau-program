//! Wire-format tests for `src/utils/raydium/open_position.rs`
//! (`open_position_with_token22_nft`).
//!
//! Pins the Borsh body byte-for-byte — including the hard-coded empty-open
//! fields — and the account order / flags against Raydium's
//! `OpenPositionWithToken22Nft` accounts struct.

mod build_open_position_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::constants::RAYDIUM_OPEN_POSITION_WITH_TOKEN22_NFT_DISCRIMINATOR;
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::raydium::open_position::{
        build_open_position_instruction_data, OpenPositionKeys, OpenPositionRange,
    };
    use anchor_lang::prelude::Pubkey;

    fn fixed(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    // Role markers — one unique byte per account, in wire order.
    const PAYER: u8 = 0x01;
    const POSITION_NFT_OWNER: u8 = 0x02;
    const POSITION_NFT_MINT: u8 = 0x03;
    const POSITION_NFT_ACCOUNT: u8 = 0x04;
    const POOL_STATE: u8 = 0x05;
    const PROTOCOL_POSITION: u8 = 0x06;
    const TICK_ARRAY_LOWER: u8 = 0x07;
    const TICK_ARRAY_UPPER: u8 = 0x08;
    const PERSONAL_POSITION: u8 = 0x09;
    const TOKEN_ACCOUNT_0: u8 = 0x0a;
    const TOKEN_ACCOUNT_1: u8 = 0x0b;
    const TOKEN_VAULT_0: u8 = 0x0c;
    const TOKEN_VAULT_1: u8 = 0x0d;
    const RENT: u8 = 0x0e;
    const SYSTEM_PROGRAM: u8 = 0x0f;
    const TOKEN_PROGRAM: u8 = 0x10;
    const ASSOCIATED_TOKEN_PROGRAM: u8 = 0x11;
    const TOKEN_PROGRAM_2022: u8 = 0x12;
    const VAULT_0_MINT: u8 = 0x13;
    const VAULT_1_MINT: u8 = 0x14;
    const TICK_ARRAY_BITMAP_EXTENSION: u8 = 0x15;

    fn keys() -> OpenPositionKeys {
        OpenPositionKeys {
            payer: fixed(PAYER),
            position_nft_owner: fixed(POSITION_NFT_OWNER),
            position_nft_mint: fixed(POSITION_NFT_MINT),
            position_nft_account: fixed(POSITION_NFT_ACCOUNT),
            pool_state: fixed(POOL_STATE),
            protocol_position: fixed(PROTOCOL_POSITION),
            tick_array_lower: fixed(TICK_ARRAY_LOWER),
            tick_array_upper: fixed(TICK_ARRAY_UPPER),
            personal_position: fixed(PERSONAL_POSITION),
            token_account_0: fixed(TOKEN_ACCOUNT_0),
            token_account_1: fixed(TOKEN_ACCOUNT_1),
            token_vault_0: fixed(TOKEN_VAULT_0),
            token_vault_1: fixed(TOKEN_VAULT_1),
            rent: fixed(RENT),
            system_program: fixed(SYSTEM_PROGRAM),
            token_program: fixed(TOKEN_PROGRAM),
            associated_token_program: fixed(ASSOCIATED_TOKEN_PROGRAM),
            token_program_2022: fixed(TOKEN_PROGRAM_2022),
            vault_0_mint: fixed(VAULT_0_MINT),
            vault_1_mint: fixed(VAULT_1_MINT),
            tick_array_bitmap_extension: fixed(TICK_ARRAY_BITMAP_EXTENSION),
        }
    }

    fn range(lower: i32, upper: i32, lower_start: i32, upper_start: i32) -> OpenPositionRange {
        OpenPositionRange {
            tick_lower: lower,
            tick_upper: upper,
            tick_array_lower_start: lower_start,
            tick_array_upper_start: upper_start,
        }
    }

    #[test]
    fn body_encodes_ticks_and_pins_an_empty_open() {
        let (data, _) = build_open_position_instruction_data(
            &keys(),
            &range(-21_161, -21_100, -21_180, -21_120),
        )
        .unwrap();
        assert_eq!(data.len(), 8 + 4 * 4 + 16 + 8 + 8 + 1 + 1);
        assert_eq!(
            &data[..8],
            &RAYDIUM_OPEN_POSITION_WITH_TOKEN22_NFT_DISCRIMINATOR
        );
        assert_eq!(i32::from_le_bytes(data[8..12].try_into().unwrap()), -21_161);
        assert_eq!(
            i32::from_le_bytes(data[12..16].try_into().unwrap()),
            -21_100
        );
        assert_eq!(
            i32::from_le_bytes(data[16..20].try_into().unwrap()),
            -21_180
        );
        assert_eq!(
            i32::from_le_bytes(data[20..24].try_into().unwrap()),
            -21_120
        );
        // liquidity = 0, amount_0_max = 0, amount_1_max = 0: no tokens move.
        assert_eq!(u128::from_le_bytes(data[24..40].try_into().unwrap()), 0);
        assert_eq!(u64::from_le_bytes(data[40..48].try_into().unwrap()), 0);
        assert_eq!(u64::from_le_bytes(data[48..56].try_into().unwrap()), 0);
        // with_metadata = false, base_flag = None.
        assert_eq!(data[56], 0);
        assert_eq!(data[57], 0);
    }

    #[test]
    fn tick_edges_encode_little_endian() {
        let (data, _) =
            build_open_position_instruction_data(&keys(), &range(i32::MIN, i32::MAX, -1, 1))
                .unwrap();
        assert_eq!(&data[8..12], &i32::MIN.to_le_bytes());
        assert_eq!(&data[12..16], &i32::MAX.to_le_bytes());
        assert_eq!(&data[16..20], &(-1i32).to_le_bytes());
        assert_eq!(&data[20..24], &1i32.to_le_bytes());
    }

    #[test]
    fn rejects_upper_not_greater_than_lower() {
        for (lower, upper) in [(50, 50), (51, 50)] {
            let err = build_open_position_instruction_data(&keys(), &range(lower, upper, 0, 0))
                .unwrap_err();
            assert_eq!(
                err_code(&err),
                aargau_err_code(AargauError::InvalidActionPayload)
            );
        }
    }

    #[test]
    fn range_derives_tick_array_starts_from_spacing() {
        let r = OpenPositionRange::new(-61, 61, 1).unwrap();
        assert_eq!(r.tick_array_lower_start, -120);
        assert_eq!(r.tick_array_upper_start, 60);
        assert!(OpenPositionRange::new(0, 1, 0).is_err());
    }

    #[test]
    fn account_meta_order_and_flags_match_open_position_with_token22_nft() {
        // (role, is_writable, is_signer) transcribed from Raydium's
        // `OpenPositionWithToken22Nft`; the bitmap extension is remaining[0].
        let expected: [(u8, bool, bool); 21] = [
            (PAYER, true, true),
            (POSITION_NFT_OWNER, false, false),
            (POSITION_NFT_MINT, true, true),
            (POSITION_NFT_ACCOUNT, true, false),
            (POOL_STATE, true, false),
            (PROTOCOL_POSITION, false, false),
            (TICK_ARRAY_LOWER, true, false),
            (TICK_ARRAY_UPPER, true, false),
            (PERSONAL_POSITION, true, false),
            (TOKEN_ACCOUNT_0, true, false),
            (TOKEN_ACCOUNT_1, true, false),
            (TOKEN_VAULT_0, true, false),
            (TOKEN_VAULT_1, true, false),
            (RENT, false, false),
            (SYSTEM_PROGRAM, false, false),
            (TOKEN_PROGRAM, false, false),
            (ASSOCIATED_TOKEN_PROGRAM, false, false),
            (TOKEN_PROGRAM_2022, false, false),
            (VAULT_0_MINT, false, false),
            (VAULT_1_MINT, false, false),
            (TICK_ARRAY_BITMAP_EXTENSION, true, false),
        ];

        let (_, metas) = build_open_position_instruction_data(&keys(), &range(0, 1, 0, 0)).unwrap();
        assert_eq!(metas.len(), expected.len());
        for (idx, (role, writable, signer)) in expected.iter().enumerate() {
            assert_eq!(metas[idx].pubkey, fixed(*role), "pubkey at slot {idx}");
            assert_eq!(
                metas[idx].is_writable, *writable,
                "is_writable at slot {idx}"
            );
            assert_eq!(metas[idx].is_signer, *signer, "is_signer at slot {idx}");
        }
    }

    #[test]
    fn vault_nft_owner_is_neither_signer_nor_writable() {
        // The vault only receives the NFT; granting it signer or writable
        // privileges to Raydium's open would be unnecessary authority.
        let (_, metas) = build_open_position_instruction_data(&keys(), &range(0, 1, 0, 0)).unwrap();
        assert!(!metas[1].is_signer && !metas[1].is_writable);
    }
}
