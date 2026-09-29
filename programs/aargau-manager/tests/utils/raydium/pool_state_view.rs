//! Tests for `src/utils/raydium/pool_state_view.rs` — byte parsing of
//! `PoolState` and the pool ⇄ mint/vault bindings.

mod parse_pool_state_view_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::constants::{
        RAYDIUM_CLMM_PROGRAM_ID, RAYDIUM_POOL_REWARD_INFOS_OFFSET, RAYDIUM_POOL_REWARD_INFO_STRIDE,
        RAYDIUM_POOL_STATE_DISCRIMINATOR, RAYDIUM_POOL_TICK_SPACING_OFFSET,
        RAYDIUM_POOL_TOKEN_MINT_0_OFFSET, RAYDIUM_POOL_TOKEN_MINT_1_OFFSET,
        RAYDIUM_POOL_TOKEN_VAULT_0_OFFSET, RAYDIUM_POOL_TOKEN_VAULT_1_OFFSET,
        RAYDIUM_REWARD_INFO_TOKEN_MINT_RELATIVE_OFFSET,
        RAYDIUM_REWARD_INFO_TOKEN_VAULT_RELATIVE_OFFSET,
    };
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::raydium::pool_state_view::{
        parse_pool_state_view_from_bytes, read_pool_state_view, require_pool_state_bindings,
        RaydiumRewardSlot, POOL_STATE_REQUIRED_LEN,
    };
    use anchor_lang::prelude::{AccountInfo, Pubkey};

    /// Deployed `PoolState::LEN`.
    const POOL_STATE_LEN: usize = 1544;

    fn fixed(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn put(data: &mut [u8], offset: usize, key: Pubkey) {
        data[offset..offset + 32].copy_from_slice(key.as_ref());
    }

    fn put_reward(data: &mut [u8], slot: usize, mint: Pubkey, vault: Pubkey) {
        let base = RAYDIUM_POOL_REWARD_INFOS_OFFSET + slot * RAYDIUM_POOL_REWARD_INFO_STRIDE;
        put(
            data,
            base + RAYDIUM_REWARD_INFO_TOKEN_MINT_RELATIVE_OFFSET,
            mint,
        );
        put(
            data,
            base + RAYDIUM_REWARD_INFO_TOKEN_VAULT_RELATIVE_OFFSET,
            vault,
        );
    }

    /// Synthetic full-size `PoolState` with tick spacing 10 and rewards in
    /// slots 0 and 1; slot 2 is uninitialized.
    fn build_pool_state() -> Vec<u8> {
        let mut data = vec![0u8; POOL_STATE_LEN];
        data[0..8].copy_from_slice(&RAYDIUM_POOL_STATE_DISCRIMINATOR);
        put(&mut data, RAYDIUM_POOL_TOKEN_MINT_0_OFFSET, fixed(0xa1));
        put(&mut data, RAYDIUM_POOL_TOKEN_MINT_1_OFFSET, fixed(0xb1));
        put(&mut data, RAYDIUM_POOL_TOKEN_VAULT_0_OFFSET, fixed(0xa2));
        put(&mut data, RAYDIUM_POOL_TOKEN_VAULT_1_OFFSET, fixed(0xb2));
        data[RAYDIUM_POOL_TICK_SPACING_OFFSET..RAYDIUM_POOL_TICK_SPACING_OFFSET + 2]
            .copy_from_slice(&10u16.to_le_bytes());
        put_reward(&mut data, 0, fixed(0xc1), fixed(0xc2));
        put_reward(&mut data, 1, fixed(0xd1), fixed(0xd2));
        data
    }

    #[test]
    fn required_len_ends_at_last_reward_vault() {
        assert_eq!(POOL_STATE_REQUIRED_LEN, 397 + 2 * 169 + 89 + 32);
        const _: () = assert!(POOL_STATE_REQUIRED_LEN <= POOL_STATE_LEN);
    }

    #[test]
    fn parses_all_fields() {
        let view = parse_pool_state_view_from_bytes(&build_pool_state()).unwrap();
        assert_eq!(view.token_mint_0, fixed(0xa1));
        assert_eq!(view.token_mint_1, fixed(0xb1));
        assert_eq!(view.token_vault_0, fixed(0xa2));
        assert_eq!(view.token_vault_1, fixed(0xb2));
        assert_eq!(view.tick_spacing, 10);
        assert_eq!(
            view.rewards[0],
            RaydiumRewardSlot {
                token_mint: fixed(0xc1),
                token_vault: fixed(0xc2),
            }
        );
        assert_eq!(view.rewards[1].token_vault, fixed(0xd2));
        assert_eq!(view.rewards[2], RaydiumRewardSlot::default());
    }

    #[test]
    fn initialized_rewards_skips_default_mint_in_slot_order() {
        let view = parse_pool_state_view_from_bytes(&build_pool_state()).unwrap();
        let mints: Vec<Pubkey> = view.initialized_rewards().map(|s| s.token_mint).collect();
        assert_eq!(mints, vec![fixed(0xc1), fixed(0xd1)]);
    }

    #[test]
    fn slot_is_initialized_by_mint_even_without_emissions() {
        // Ended rewards keep their mint and stay initialized.
        let slot = RaydiumRewardSlot {
            token_mint: fixed(0x01),
            token_vault: fixed(0x02),
        };
        assert!(slot.is_initialized());
        assert!(!RaydiumRewardSlot::default().is_initialized());
    }

    #[test]
    fn parses_buffer_of_exactly_required_len() {
        let mut data = build_pool_state();
        data.truncate(POOL_STATE_REQUIRED_LEN);
        assert!(parse_pool_state_view_from_bytes(&data).is_ok());
    }

    #[test]
    fn rejects_short_buffer() {
        let mut data = build_pool_state();
        data.truncate(POOL_STATE_REQUIRED_LEN - 1);
        let err = parse_pool_state_view_from_bytes(&data).unwrap_err();
        assert_eq!(err_code(&err), aargau_err_code(AargauError::InvalidPool));
    }

    #[test]
    fn rejects_empty_buffer() {
        let err = parse_pool_state_view_from_bytes(&[]).unwrap_err();
        assert_eq!(err_code(&err), aargau_err_code(AargauError::InvalidPool));
    }

    #[test]
    fn rejects_wrong_discriminator() {
        let mut data = build_pool_state();
        data[0] ^= 0xff;
        let err = parse_pool_state_view_from_bytes(&data).unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidPoolDiscriminator)
        );
    }

    #[test]
    fn account_info_reader_accepts_clmm_owned_pool() {
        let key = fixed(0x77);
        let mut lamports = 0u64;
        let mut data = build_pool_state();
        let owner = RAYDIUM_CLMM_PROGRAM_ID;
        let info = AccountInfo::new(&key, false, true, &mut lamports, &mut data, &owner, false);
        assert_eq!(read_pool_state_view(&info).unwrap().tick_spacing, 10);
    }

    #[test]
    fn account_info_reader_rejects_foreign_owner() {
        let key = fixed(0x77);
        let mut lamports = 0u64;
        let mut data = build_pool_state();
        let owner = fixed(0xEE);
        let info = AccountInfo::new(&key, false, true, &mut lamports, &mut data, &owner, false);
        let err = read_pool_state_view(&info).unwrap_err();
        assert_eq!(err_code(&err), aargau_err_code(AargauError::InvalidPool));
    }

    #[test]
    fn bindings_accept_matching_keys() {
        let view = parse_pool_state_view_from_bytes(&build_pool_state()).unwrap();
        assert!(require_pool_state_bindings(
            &view,
            &fixed(0xa1),
            &fixed(0xb1),
            &fixed(0xa2),
            &fixed(0xb2),
        )
        .is_ok());
    }

    #[test]
    fn bindings_reject_swapped_mints() {
        let view = parse_pool_state_view_from_bytes(&build_pool_state()).unwrap();
        let err = require_pool_state_bindings(
            &view,
            &fixed(0xb1),
            &fixed(0xa1),
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
    fn bindings_reject_substituted_mint_1() {
        let view = parse_pool_state_view_from_bytes(&build_pool_state()).unwrap();
        let err = require_pool_state_bindings(
            &view,
            &fixed(0xa1),
            &fixed(0xff),
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
    fn bindings_reject_substituted_vault_0() {
        // A caller-owned token account in place of the pool vault would fake
        // the fee outflow measurement.
        let view = parse_pool_state_view_from_bytes(&build_pool_state()).unwrap();
        let err = require_pool_state_bindings(
            &view,
            &fixed(0xa1),
            &fixed(0xb1),
            &fixed(0xff),
            &fixed(0xb2),
        )
        .unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidPoolReserve)
        );
    }

    #[test]
    fn bindings_reject_substituted_vault_1() {
        let view = parse_pool_state_view_from_bytes(&build_pool_state()).unwrap();
        let err = require_pool_state_bindings(
            &view,
            &fixed(0xa1),
            &fixed(0xb1),
            &fixed(0xa2),
            &fixed(0xff),
        )
        .unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidPoolReserve)
        );
    }
}
