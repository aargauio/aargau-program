//! Wire-format tests for `src/utils/raydium/increase_liquidity.rs`
//! (`increase_liquidity_v2`).

mod build_increase_liquidity_v2_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::constants::RAYDIUM_INCREASE_LIQUIDITY_V2_DISCRIMINATOR;
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::raydium::increase_liquidity::{
        build_increase_liquidity_v2_instruction_data, IncreaseLiquidityV2Keys,
    };
    use anchor_lang::prelude::Pubkey;

    fn fixed(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    const NFT_OWNER: u8 = 0x01;
    const NFT_ACCOUNT: u8 = 0x02;
    const POOL_STATE: u8 = 0x03;
    const PROTOCOL_POSITION: u8 = 0x04;
    const PERSONAL_POSITION: u8 = 0x05;
    const TICK_ARRAY_LOWER: u8 = 0x06;
    const TICK_ARRAY_UPPER: u8 = 0x07;
    const TOKEN_ACCOUNT_0: u8 = 0x08;
    const TOKEN_ACCOUNT_1: u8 = 0x09;
    const TOKEN_VAULT_0: u8 = 0x0a;
    const TOKEN_VAULT_1: u8 = 0x0b;
    const TOKEN_PROGRAM: u8 = 0x0c;
    const TOKEN_PROGRAM_2022: u8 = 0x0d;
    const VAULT_0_MINT: u8 = 0x0e;
    const VAULT_1_MINT: u8 = 0x0f;
    const TICK_ARRAY_BITMAP_EXTENSION: u8 = 0x10;

    fn keys() -> IncreaseLiquidityV2Keys {
        IncreaseLiquidityV2Keys {
            nft_owner: fixed(NFT_OWNER),
            nft_account: fixed(NFT_ACCOUNT),
            pool_state: fixed(POOL_STATE),
            protocol_position: fixed(PROTOCOL_POSITION),
            personal_position: fixed(PERSONAL_POSITION),
            tick_array_lower: fixed(TICK_ARRAY_LOWER),
            tick_array_upper: fixed(TICK_ARRAY_UPPER),
            token_account_0: fixed(TOKEN_ACCOUNT_0),
            token_account_1: fixed(TOKEN_ACCOUNT_1),
            token_vault_0: fixed(TOKEN_VAULT_0),
            token_vault_1: fixed(TOKEN_VAULT_1),
            token_program: fixed(TOKEN_PROGRAM),
            token_program_2022: fixed(TOKEN_PROGRAM_2022),
            vault_0_mint: fixed(VAULT_0_MINT),
            vault_1_mint: fixed(VAULT_1_MINT),
            tick_array_bitmap_extension: fixed(TICK_ARRAY_BITMAP_EXTENSION),
        }
    }

    #[test]
    fn body_is_disc_u128_u64_u64_none() {
        let (data, _) = build_increase_liquidity_v2_instruction_data(&keys(), 999, 5, 6).unwrap();
        assert_eq!(data.len(), 8 + 16 + 8 + 8 + 1);
        assert_eq!(&data[..8], &RAYDIUM_INCREASE_LIQUIDITY_V2_DISCRIMINATOR);
        assert_eq!(u128::from_le_bytes(data[8..24].try_into().unwrap()), 999);
        assert_eq!(u64::from_le_bytes(data[24..32].try_into().unwrap()), 5);
        assert_eq!(u64::from_le_bytes(data[32..40].try_into().unwrap()), 6);
        assert_eq!(data[40], 0, "base_flag must be None");
    }

    #[test]
    fn arg_edges_one_and_max() {
        let (data, _) =
            build_increase_liquidity_v2_instruction_data(&keys(), 1, 0, u64::MAX).unwrap();
        assert_eq!(u128::from_le_bytes(data[8..24].try_into().unwrap()), 1);
        assert_eq!(u64::from_le_bytes(data[24..32].try_into().unwrap()), 0);
        assert_eq!(
            u64::from_le_bytes(data[32..40].try_into().unwrap()),
            u64::MAX
        );

        let (data, _) =
            build_increase_liquidity_v2_instruction_data(&keys(), u128::MAX, 1, 1).unwrap();
        assert_eq!(&data[8..24], &u128::MAX.to_le_bytes());
    }

    #[test]
    fn rejects_zero_liquidity() {
        // With base_flag = None Raydium would fail with MissingBaseFlag.
        let err = build_increase_liquidity_v2_instruction_data(&keys(), 0, 1, 1).unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidActionPayload)
        );
    }

    #[test]
    fn account_meta_order_and_flags_match_increase_liquidity_v2() {
        // (role, is_writable, is_signer) transcribed from Raydium's
        // `IncreaseLiquidityV2`; the bitmap extension is remaining[0].
        let expected: [(u8, bool, bool); 16] = [
            (NFT_OWNER, false, true),
            (NFT_ACCOUNT, false, false),
            (POOL_STATE, true, false),
            (PROTOCOL_POSITION, false, false),
            (PERSONAL_POSITION, true, false),
            (TICK_ARRAY_LOWER, true, false),
            (TICK_ARRAY_UPPER, true, false),
            (TOKEN_ACCOUNT_0, true, false),
            (TOKEN_ACCOUNT_1, true, false),
            (TOKEN_VAULT_0, true, false),
            (TOKEN_VAULT_1, true, false),
            (TOKEN_PROGRAM, false, false),
            (TOKEN_PROGRAM_2022, false, false),
            (VAULT_0_MINT, false, false),
            (VAULT_1_MINT, false, false),
            (TICK_ARRAY_BITMAP_EXTENSION, true, false),
        ];

        let (_, metas) = build_increase_liquidity_v2_instruction_data(&keys(), 1, 1, 1).unwrap();
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
}
