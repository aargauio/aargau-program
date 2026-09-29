//! Tests for `src/utils/raydium/personal_position_view.rs` — byte parsing of
//! `PersonalPositionState` and its binding to the vault.

mod parse_personal_position_view_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::constants::{
        RAYDIUM_CLMM_PROGRAM_ID, RAYDIUM_PERSONAL_POSITION_DISCRIMINATOR,
        RAYDIUM_PERSONAL_POSITION_FEES_OWED_0_OFFSET, RAYDIUM_PERSONAL_POSITION_FEES_OWED_1_OFFSET,
        RAYDIUM_PERSONAL_POSITION_LEN, RAYDIUM_PERSONAL_POSITION_LIQUIDITY_OFFSET,
        RAYDIUM_PERSONAL_POSITION_NFT_MINT_OFFSET, RAYDIUM_PERSONAL_POSITION_POOL_ID_OFFSET,
        RAYDIUM_PERSONAL_POSITION_REWARD_OWED_OFFSETS, RAYDIUM_PERSONAL_POSITION_TICK_LOWER_OFFSET,
        RAYDIUM_PERSONAL_POSITION_TICK_UPPER_OFFSET,
    };
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::raydium::personal_position_view::{
        parse_personal_position_view_from_bytes, read_personal_position_view,
        require_personal_position_bindings,
    };
    use anchor_lang::prelude::{AccountInfo, Pubkey};

    fn fixed(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn build_position(liquidity: u128) -> Vec<u8> {
        let mut data = vec![0u8; RAYDIUM_PERSONAL_POSITION_LEN];
        data[0..8].copy_from_slice(&RAYDIUM_PERSONAL_POSITION_DISCRIMINATOR);
        data[8] = 254; // bump
        let mint = RAYDIUM_PERSONAL_POSITION_NFT_MINT_OFFSET;
        data[mint..mint + 32].copy_from_slice(fixed(0x11).as_ref());
        let pool = RAYDIUM_PERSONAL_POSITION_POOL_ID_OFFSET;
        data[pool..pool + 32].copy_from_slice(fixed(0x22).as_ref());
        let lower = RAYDIUM_PERSONAL_POSITION_TICK_LOWER_OFFSET;
        data[lower..lower + 4].copy_from_slice(&(-21_240i32).to_le_bytes());
        let upper = RAYDIUM_PERSONAL_POSITION_TICK_UPPER_OFFSET;
        data[upper..upper + 4].copy_from_slice(&(-21_120i32).to_le_bytes());
        let liq = RAYDIUM_PERSONAL_POSITION_LIQUIDITY_OFFSET;
        data[liq..liq + 16].copy_from_slice(&liquidity.to_le_bytes());
        // Fee-growth snapshots sit between liquidity and fees owed; fill them
        // with noise to prove the parser does not read them as owed amounts.
        data[97..129].fill(0xEE);
        let fee0 = RAYDIUM_PERSONAL_POSITION_FEES_OWED_0_OFFSET;
        data[fee0..fee0 + 8].copy_from_slice(&7u64.to_le_bytes());
        let fee1 = RAYDIUM_PERSONAL_POSITION_FEES_OWED_1_OFFSET;
        data[fee1..fee1 + 8].copy_from_slice(&8u64.to_le_bytes());
        for (slot, offset) in RAYDIUM_PERSONAL_POSITION_REWARD_OWED_OFFSETS
            .iter()
            .enumerate()
        {
            // reward growth (u128) precedes each owed amount
            data[offset - 16..*offset].fill(0xDD);
            data[*offset..offset + 8].copy_from_slice(&(100 + slot as u64).to_le_bytes());
        }
        data
    }

    #[test]
    fn parses_all_fields() {
        let view = parse_personal_position_view_from_bytes(&build_position(123_456)).unwrap();
        assert_eq!(view.nft_mint, fixed(0x11));
        assert_eq!(view.pool_id, fixed(0x22));
        assert_eq!(view.tick_lower, -21_240);
        assert_eq!(view.tick_upper, -21_120);
        assert_eq!(view.liquidity, 123_456);
        assert_eq!(view.token_fees_owed_0, 7);
        assert_eq!(view.token_fees_owed_1, 8);
        assert_eq!(view.reward_amounts_owed, [100, 101, 102]);
    }

    #[test]
    fn liquidity_edges_zero_one_and_max() {
        for liquidity in [0u128, 1, u128::MAX] {
            let view = parse_personal_position_view_from_bytes(&build_position(liquidity)).unwrap();
            assert_eq!(view.liquidity, liquidity);
        }
    }

    #[test]
    fn rejects_short_buffer() {
        let mut data = build_position(1);
        data.truncate(RAYDIUM_PERSONAL_POSITION_LEN - 1);
        let err = parse_personal_position_view_from_bytes(&data).unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidPositionDiscriminator)
        );
    }

    #[test]
    fn rejects_empty_buffer() {
        let err = parse_personal_position_view_from_bytes(&[]).unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidPositionDiscriminator)
        );
    }

    #[test]
    fn rejects_wrong_discriminator() {
        let mut data = build_position(1);
        data[7] ^= 0x01;
        let err = parse_personal_position_view_from_bytes(&data).unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidPositionDiscriminator)
        );
    }

    #[test]
    fn account_info_reader_accepts_clmm_owned_position() {
        let key = fixed(0x33);
        let mut lamports = 0u64;
        let mut data = build_position(5);
        let owner = RAYDIUM_CLMM_PROGRAM_ID;
        let info = AccountInfo::new(&key, false, true, &mut lamports, &mut data, &owner, false);
        assert_eq!(read_personal_position_view(&info).unwrap().liquidity, 5);
    }

    #[test]
    fn account_info_reader_rejects_foreign_owner() {
        // Correct bytes under another program must not be trusted: the
        // on-chain liquidity read drives how much a drain removes.
        let key = fixed(0x33);
        let mut lamports = 0u64;
        let mut data = build_position(5);
        let owner = fixed(0xEE);
        let info = AccountInfo::new(&key, false, true, &mut lamports, &mut data, &owner, false);
        let err = read_personal_position_view(&info).unwrap_err();
        assert_eq!(err_code(&err), aargau_err_code(AargauError::InvalidPool));
    }

    #[test]
    fn bindings_accept_vault_mint_and_pool() {
        let view = parse_personal_position_view_from_bytes(&build_position(1)).unwrap();
        assert!(require_personal_position_bindings(&view, &fixed(0x11), &fixed(0x22)).is_ok());
    }

    #[test]
    fn bindings_reject_other_nft_mint() {
        let view = parse_personal_position_view_from_bytes(&build_position(1)).unwrap();
        let err =
            require_personal_position_bindings(&view, &fixed(0x99), &fixed(0x22)).unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidVaultPda)
        );
    }

    #[test]
    fn bindings_reject_other_pool() {
        let view = parse_personal_position_view_from_bytes(&build_position(1)).unwrap();
        let err =
            require_personal_position_bindings(&view, &fixed(0x11), &fixed(0x99)).unwrap_err();
        assert_eq!(err_code(&err), aargau_err_code(AargauError::InvalidPool));
    }
}
