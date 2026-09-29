//! Wire-format tests for `src/utils/raydium/close_position.rs`
//! (`close_position`).

mod build_close_position_tests {
    use aargau_manager::constants::RAYDIUM_CLOSE_POSITION_DISCRIMINATOR;
    use aargau_manager::utils::raydium::close_position::{
        build_close_position_instruction_data, ClosePositionKeys,
    };
    use anchor_lang::prelude::Pubkey;

    fn fixed(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    const NFT_OWNER: u8 = 0x01;
    const POSITION_NFT_MINT: u8 = 0x02;
    const POSITION_NFT_ACCOUNT: u8 = 0x03;
    const PERSONAL_POSITION: u8 = 0x04;
    const SYSTEM_PROGRAM: u8 = 0x05;
    const TOKEN_PROGRAM: u8 = 0x06;
    const POOL_STATE: u8 = 0x07;

    fn keys(pool_state_for_thaw: Option<Pubkey>) -> ClosePositionKeys {
        ClosePositionKeys {
            nft_owner: fixed(NFT_OWNER),
            position_nft_mint: fixed(POSITION_NFT_MINT),
            position_nft_account: fixed(POSITION_NFT_ACCOUNT),
            personal_position: fixed(PERSONAL_POSITION),
            system_program: fixed(SYSTEM_PROGRAM),
            token_program: fixed(TOKEN_PROGRAM),
            pool_state_for_thaw,
        }
    }

    /// (role, is_writable, is_signer) transcribed from Raydium's `ClosePosition`.
    const EXPECTED: [(u8, bool, bool); 6] = [
        (NFT_OWNER, true, true),
        (POSITION_NFT_MINT, true, false),
        (POSITION_NFT_ACCOUNT, true, false),
        (PERSONAL_POSITION, true, false),
        (SYSTEM_PROGRAM, false, false),
        (TOKEN_PROGRAM, false, false),
    ];

    #[test]
    fn body_is_discriminator_only() {
        let (data, _) = build_close_position_instruction_data(&keys(None));
        assert_eq!(data, RAYDIUM_CLOSE_POSITION_DISCRIMINATOR);
    }

    #[test]
    fn account_meta_order_and_flags_without_frozen_nft() {
        let (_, metas) = build_close_position_instruction_data(&keys(None));
        assert_eq!(metas.len(), EXPECTED.len());
        for (idx, (role, writable, signer)) in EXPECTED.iter().enumerate() {
            assert_eq!(metas[idx].pubkey, fixed(*role), "pubkey at slot {idx}");
            assert_eq!(
                metas[idx].is_writable, *writable,
                "is_writable at slot {idx}"
            );
            assert_eq!(metas[idx].is_signer, *signer, "is_signer at slot {idx}");
        }
    }

    #[test]
    fn frozen_nft_appends_pool_state_readonly() {
        let (_, metas) = build_close_position_instruction_data(&keys(Some(fixed(POOL_STATE))));
        assert_eq!(metas.len(), EXPECTED.len() + 1);
        let pool = &metas[EXPECTED.len()];
        assert_eq!(pool.pubkey, fixed(POOL_STATE));
        assert!(!pool.is_writable && !pool.is_signer);
    }
}
