//! Tests for `src/utils/meteora/lb_pair_view.rs` — pure byte parsing of the
//! Meteora `LbPair` account and the cross-checks that bind the pool to the
//! caller-supplied mints / reserves.
//!
//! These exercise wire-format invariants (offsets, discriminator, field
//! ordering) and the rejection paths that protect against substituted
//! mint/reserve accounts. Anything that needs a live `Context` belongs in a
//! Bankrun harness, not here.

mod parse_lb_pair_view_from_bytes_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::constants::METEORA_LB_PAIR_DISCRIMINATOR;
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::meteora::lb_pair_view::{
        parse_lb_pair_view_from_bytes, LB_PAIR_ACTIVE_ID_OFFSET, LB_PAIR_DISCRIMINATOR_LEN,
        LB_PAIR_REQUIRED_LEN, LB_PAIR_RESERVE_X_OFFSET, LB_PAIR_RESERVE_Y_OFFSET,
        LB_PAIR_TOKEN_X_MINT_OFFSET, LB_PAIR_TOKEN_Y_MINT_OFFSET,
    };
    use anchor_lang::prelude::Pubkey;

    /// Build a synthetic `LbPair` data buffer with all required offsets
    /// populated. Fields default to deterministic test pubkeys / values.
    fn build_lb_pair_buffer(
        active_id: i32,
        token_x_mint: Pubkey,
        token_y_mint: Pubkey,
        reserve_x: Pubkey,
        reserve_y: Pubkey,
        discriminator: [u8; 8],
    ) -> Vec<u8> {
        let mut buf = vec![0u8; LB_PAIR_REQUIRED_LEN];
        buf[0..LB_PAIR_DISCRIMINATOR_LEN].copy_from_slice(&discriminator);
        buf[LB_PAIR_ACTIVE_ID_OFFSET..LB_PAIR_ACTIVE_ID_OFFSET + 4]
            .copy_from_slice(&active_id.to_le_bytes());
        buf[LB_PAIR_TOKEN_X_MINT_OFFSET..LB_PAIR_TOKEN_X_MINT_OFFSET + 32]
            .copy_from_slice(&token_x_mint.to_bytes());
        buf[LB_PAIR_TOKEN_Y_MINT_OFFSET..LB_PAIR_TOKEN_Y_MINT_OFFSET + 32]
            .copy_from_slice(&token_y_mint.to_bytes());
        buf[LB_PAIR_RESERVE_X_OFFSET..LB_PAIR_RESERVE_X_OFFSET + 32]
            .copy_from_slice(&reserve_x.to_bytes());
        buf[LB_PAIR_RESERVE_Y_OFFSET..LB_PAIR_RESERVE_Y_OFFSET + 32]
            .copy_from_slice(&reserve_y.to_bytes());
        buf
    }

    #[test]
    fn happy_path_returns_expected_fields() {
        let token_x = Pubkey::new_unique();
        let token_y = Pubkey::new_unique();
        let reserve_x = Pubkey::new_unique();
        let reserve_y = Pubkey::new_unique();
        let buf = build_lb_pair_buffer(
            1234,
            token_x,
            token_y,
            reserve_x,
            reserve_y,
            METEORA_LB_PAIR_DISCRIMINATOR,
        );
        let view = parse_lb_pair_view_from_bytes(&buf).expect("parse should succeed");
        assert_eq!(view.active_id, 1234);
        assert_eq!(view.token_x_mint, token_x);
        assert_eq!(view.token_y_mint, token_y);
        assert_eq!(view.reserve_x, reserve_x);
        assert_eq!(view.reserve_y, reserve_y);
    }

    #[test]
    fn happy_path_negative_active_id_decodes_correctly() {
        // active_id is `i32`; ensure two's-complement round-trip works.
        let buf = build_lb_pair_buffer(
            -987,
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            METEORA_LB_PAIR_DISCRIMINATOR,
        );
        let view = parse_lb_pair_view_from_bytes(&buf).expect("parse should succeed");
        assert_eq!(view.active_id, -987);
    }

    #[test]
    fn discriminator_mismatch_returns_invalid_pool_discriminator() {
        let mut wrong = METEORA_LB_PAIR_DISCRIMINATOR;
        wrong[0] = wrong[0].wrapping_add(1);
        let buf = build_lb_pair_buffer(
            0,
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            wrong,
        );
        let err = parse_lb_pair_view_from_bytes(&buf).expect_err("should reject");
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidPoolDiscriminator)
        );
    }

    #[test]
    fn truncated_buffer_one_byte_short_returns_invalid_pool() {
        // 215 bytes — one short of `RESERVE_Y_OFFSET + 32 = 216`.
        let buf = vec![0u8; LB_PAIR_REQUIRED_LEN - 1];
        let err = parse_lb_pair_view_from_bytes(&buf).expect_err("should reject");
        assert_eq!(err_code(&err), aargau_err_code(AargauError::InvalidPool));
    }

    #[test]
    fn empty_buffer_returns_invalid_pool_no_panic() {
        let err = parse_lb_pair_view_from_bytes(&[]).expect_err("should reject");
        assert_eq!(err_code(&err), aargau_err_code(AargauError::InvalidPool));
    }

    #[test]
    fn buffer_shorter_than_discriminator_returns_invalid_pool() {
        // 4 bytes — far smaller than the 8-byte discriminator. Length
        // check must fire before we ever index into the buffer.
        let err = parse_lb_pair_view_from_bytes(&[1, 2, 3, 4]).expect_err("should reject");
        assert_eq!(err_code(&err), aargau_err_code(AargauError::InvalidPool));
    }

    #[test]
    fn exactly_minimum_length_succeeds() {
        let buf = build_lb_pair_buffer(
            0,
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            METEORA_LB_PAIR_DISCRIMINATOR,
        );
        assert_eq!(buf.len(), LB_PAIR_REQUIRED_LEN);
        assert!(parse_lb_pair_view_from_bytes(&buf).is_ok());
    }
}

mod require_lb_pair_bindings_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::meteora::lb_pair_view::{require_lb_pair_bindings, LbPairView};
    use anchor_lang::prelude::Pubkey;

    fn fixture() -> (LbPairView, Pubkey, Pubkey, Pubkey, Pubkey) {
        let token_x = Pubkey::new_unique();
        let token_y = Pubkey::new_unique();
        let reserve_x = Pubkey::new_unique();
        let reserve_y = Pubkey::new_unique();
        let view = LbPairView {
            active_id: 0,
            token_x_mint: token_x,
            token_y_mint: token_y,
            reserve_x,
            reserve_y,
        };
        (view, token_x, token_y, reserve_x, reserve_y)
    }

    #[test]
    fn all_keys_match_returns_ok() {
        let (view, mint_a, mint_b, reserve_x, reserve_y) = fixture();
        assert!(require_lb_pair_bindings(&view, &mint_a, &mint_b, &reserve_x, &reserve_y).is_ok());
    }

    #[test]
    fn mint_a_mismatch_returns_invalid_pool_mint() {
        let (view, _, mint_b, reserve_x, reserve_y) = fixture();
        let bad_mint_a = Pubkey::new_unique();
        let err = require_lb_pair_bindings(&view, &bad_mint_a, &mint_b, &reserve_x, &reserve_y)
            .expect_err("should reject");
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidPoolMint)
        );
    }

    #[test]
    fn mint_b_mismatch_returns_invalid_pool_mint() {
        let (view, mint_a, _, reserve_x, reserve_y) = fixture();
        let bad_mint_b = Pubkey::new_unique();
        let err = require_lb_pair_bindings(&view, &mint_a, &bad_mint_b, &reserve_x, &reserve_y)
            .expect_err("should reject");
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidPoolMint)
        );
    }

    #[test]
    fn reserve_x_mismatch_returns_invalid_pool_reserve() {
        let (view, mint_a, mint_b, _, reserve_y) = fixture();
        let bad_reserve_x = Pubkey::new_unique();
        let err = require_lb_pair_bindings(&view, &mint_a, &mint_b, &bad_reserve_x, &reserve_y)
            .expect_err("should reject");
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidPoolReserve)
        );
    }

    #[test]
    fn reserve_y_mismatch_returns_invalid_pool_reserve() {
        let (view, mint_a, mint_b, reserve_x, _) = fixture();
        let bad_reserve_y = Pubkey::new_unique();
        let err = require_lb_pair_bindings(&view, &mint_a, &mint_b, &reserve_x, &bad_reserve_y)
            .expect_err("should reject");
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidPoolReserve)
        );
    }
}

mod require_spl_classic_mints_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::meteora::lb_pair_view::require_spl_classic_mints;
    use anchor_lang::prelude::Pubkey;

    #[test]
    fn both_mints_owned_by_classic_token_program_returns_ok() {
        let token_program = Pubkey::new_unique();
        assert!(require_spl_classic_mints(&token_program, &token_program, &token_program).is_ok());
    }

    #[test]
    fn mint_a_owned_by_token_2022_returns_token_2022_not_supported() {
        let token_program = Pubkey::new_unique();
        let token_2022 = Pubkey::new_unique();
        let err = require_spl_classic_mints(&token_2022, &token_program, &token_program)
            .expect_err("should reject");
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::Token2022NotSupported)
        );
    }

    #[test]
    fn mint_b_owned_by_token_2022_returns_token_2022_not_supported() {
        let token_program = Pubkey::new_unique();
        let token_2022 = Pubkey::new_unique();
        let err = require_spl_classic_mints(&token_program, &token_2022, &token_program)
            .expect_err("should reject");
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::Token2022NotSupported)
        );
    }

    #[test]
    fn both_mints_owned_by_token_2022_returns_token_2022_not_supported() {
        let token_program = Pubkey::new_unique();
        let token_2022 = Pubkey::new_unique();
        let err = require_spl_classic_mints(&token_2022, &token_2022, &token_program)
            .expect_err("should reject");
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::Token2022NotSupported)
        );
    }
}
