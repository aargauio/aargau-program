//! Tests for `src/utils/orca/whirlpool_view.rs` — byte parsing of the
//! `Whirlpool` account and the pool ⇄ mint/vault/reward bindings.

mod parse_whirlpool_view_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::constants::ORCA_WHIRLPOOL_DISCRIMINATOR;
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::orca::whirlpool_view::{
        parse_whirlpool_view_from_bytes, require_whirlpool_bindings, WHIRLPOOL_REQUIRED_LEN,
        WHIRLPOOL_REWARD_INFOS_OFFSET, WHIRLPOOL_REWARD_INFO_STRIDE, WHIRLPOOL_SQRT_PRICE_OFFSET,
        WHIRLPOOL_TICK_CURRENT_OFFSET, WHIRLPOOL_TICK_SPACING_OFFSET,
        WHIRLPOOL_TOKEN_MINT_A_OFFSET, WHIRLPOOL_TOKEN_MINT_B_OFFSET,
        WHIRLPOOL_TOKEN_VAULT_A_OFFSET, WHIRLPOOL_TOKEN_VAULT_B_OFFSET,
    };
    use anchor_lang::prelude::Pubkey;

    fn fixed(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    /// Build a synthetic `Whirlpool` buffer with the given fields. Reward slot
    /// 0 is active; slots 1 and 2 are left as `Pubkey::default()` (inactive).
    fn build_whirlpool() -> Vec<u8> {
        let mut data = vec![0u8; WHIRLPOOL_REQUIRED_LEN];
        data[0..8].copy_from_slice(&ORCA_WHIRLPOOL_DISCRIMINATOR);
        data[WHIRLPOOL_TICK_SPACING_OFFSET..WHIRLPOOL_TICK_SPACING_OFFSET + 2]
            .copy_from_slice(&64u16.to_le_bytes());
        data[WHIRLPOOL_SQRT_PRICE_OFFSET..WHIRLPOOL_SQRT_PRICE_OFFSET + 16]
            .copy_from_slice(&(1u128 << 64).to_le_bytes());
        data[WHIRLPOOL_TICK_CURRENT_OFFSET..WHIRLPOOL_TICK_CURRENT_OFFSET + 4]
            .copy_from_slice(&(-128i32).to_le_bytes());
        data[WHIRLPOOL_TOKEN_MINT_A_OFFSET..WHIRLPOOL_TOKEN_MINT_A_OFFSET + 32]
            .copy_from_slice(fixed(0xa1).as_ref());
        data[WHIRLPOOL_TOKEN_VAULT_A_OFFSET..WHIRLPOOL_TOKEN_VAULT_A_OFFSET + 32]
            .copy_from_slice(fixed(0xa2).as_ref());
        data[WHIRLPOOL_TOKEN_MINT_B_OFFSET..WHIRLPOOL_TOKEN_MINT_B_OFFSET + 32]
            .copy_from_slice(fixed(0xb1).as_ref());
        data[WHIRLPOOL_TOKEN_VAULT_B_OFFSET..WHIRLPOOL_TOKEN_VAULT_B_OFFSET + 32]
            .copy_from_slice(fixed(0xb2).as_ref());
        // Reward slot 0: mint + vault active.
        let r0 = WHIRLPOOL_REWARD_INFOS_OFFSET;
        data[r0..r0 + 32].copy_from_slice(fixed(0xc1).as_ref());
        data[r0 + 32..r0 + 64].copy_from_slice(fixed(0xc2).as_ref());
        data
    }

    #[test]
    fn parses_all_fields() {
        let data = build_whirlpool();
        let view = parse_whirlpool_view_from_bytes(&data).unwrap();
        assert_eq!(view.tick_spacing, 64);
        assert_eq!(view.sqrt_price, 1u128 << 64);
        assert_eq!(view.tick_current_index, -128);
        assert_eq!(view.token_mint_a, fixed(0xa1));
        assert_eq!(view.token_vault_a, fixed(0xa2));
        assert_eq!(view.token_mint_b, fixed(0xb1));
        assert_eq!(view.token_vault_b, fixed(0xb2));
        // Reward slot 0 active, 1 and 2 inactive (default).
        assert_eq!(view.rewards[0].mint, fixed(0xc1));
        assert_eq!(view.rewards[0].vault, fixed(0xc2));
        assert_eq!(view.rewards[1].vault, Pubkey::default());
        assert_eq!(view.rewards[2].vault, Pubkey::default());
    }

    #[test]
    fn reward_slot_stride_is_128() {
        // Lock the stride against an accidental layout drift.
        assert_eq!(WHIRLPOOL_REWARD_INFO_STRIDE, 128);
    }

    #[test]
    fn rejects_short_buffer() {
        let data = vec![0u8; WHIRLPOOL_REQUIRED_LEN - 1];
        let err = parse_whirlpool_view_from_bytes(&data).unwrap_err();
        assert_eq!(err_code(&err), aargau_err_code(AargauError::InvalidPool));
    }

    #[test]
    fn rejects_wrong_discriminator() {
        let mut data = build_whirlpool();
        data[0] ^= 0xff;
        let err = parse_whirlpool_view_from_bytes(&data).unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidPoolDiscriminator),
        );
    }

    #[test]
    fn bindings_accept_matching_keys() {
        let view = parse_whirlpool_view_from_bytes(&build_whirlpool()).unwrap();
        assert!(require_whirlpool_bindings(
            &view,
            &fixed(0xa1),
            &fixed(0xb1),
            &fixed(0xa2),
            &fixed(0xb2),
        )
        .is_ok());
    }

    #[test]
    fn bindings_reject_substituted_mint() {
        let view = parse_whirlpool_view_from_bytes(&build_whirlpool()).unwrap();
        let err = require_whirlpool_bindings(
            &view,
            &fixed(0xff), // wrong mint_a
            &fixed(0xb1),
            &fixed(0xa2),
            &fixed(0xb2),
        )
        .unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidPoolMint)
        );
    }

    #[test]
    fn bindings_reject_substituted_vault() {
        let view = parse_whirlpool_view_from_bytes(&build_whirlpool()).unwrap();
        let err = require_whirlpool_bindings(
            &view,
            &fixed(0xa1),
            &fixed(0xb1),
            &fixed(0xa2),
            &fixed(0xff), // wrong vault_b
        )
        .unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidPoolReserve),
        );
    }
}
