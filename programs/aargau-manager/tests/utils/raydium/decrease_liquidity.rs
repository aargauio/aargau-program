//! Wire-format tests for `src/utils/raydium/decrease_liquidity.rs`
//! (`decrease_liquidity_v2`), including the reward-triple tail.

mod build_decrease_liquidity_v2_tests {
    use aargau_manager::constants::RAYDIUM_DECREASE_LIQUIDITY_V2_DISCRIMINATOR;
    use aargau_manager::utils::raydium::decrease_liquidity::{
        build_decrease_liquidity_v2_instruction_data, DecreaseLiquidityV2Keys,
    };
    use aargau_manager::utils::raydium::reward_accounts::RewardTransferKeys;
    use anchor_lang::prelude::Pubkey;
    use anchor_lang::solana_program::instruction::AccountMeta;

    fn fixed(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    const NFT_OWNER: u8 = 0x01;
    const NFT_ACCOUNT: u8 = 0x02;
    const PERSONAL_POSITION: u8 = 0x03;
    const POOL_STATE: u8 = 0x04;
    const PROTOCOL_POSITION: u8 = 0x05;
    const TOKEN_VAULT_0: u8 = 0x06;
    const TOKEN_VAULT_1: u8 = 0x07;
    const TICK_ARRAY_LOWER: u8 = 0x08;
    const TICK_ARRAY_UPPER: u8 = 0x09;
    const RECIPIENT_0: u8 = 0x0a;
    const RECIPIENT_1: u8 = 0x0b;
    const TOKEN_PROGRAM: u8 = 0x0c;
    const TOKEN_PROGRAM_2022: u8 = 0x0d;
    const MEMO_PROGRAM: u8 = 0x0e;
    const VAULT_0_MINT: u8 = 0x0f;
    const VAULT_1_MINT: u8 = 0x10;
    const TICK_ARRAY_BITMAP_EXTENSION: u8 = 0x11;

    /// (role, is_writable, is_signer) transcribed from Raydium's
    /// `DecreaseLiquidityV2`, followed by the bitmap extension.
    const FIXED_ACCOUNTS: [(u8, bool, bool); 17] = [
        (NFT_OWNER, false, true),
        (NFT_ACCOUNT, false, false),
        (PERSONAL_POSITION, true, false),
        (POOL_STATE, true, false),
        (PROTOCOL_POSITION, false, false),
        (TOKEN_VAULT_0, true, false),
        (TOKEN_VAULT_1, true, false),
        (TICK_ARRAY_LOWER, true, false),
        (TICK_ARRAY_UPPER, true, false),
        (RECIPIENT_0, true, false),
        (RECIPIENT_1, true, false),
        (TOKEN_PROGRAM, false, false),
        (TOKEN_PROGRAM_2022, false, false),
        (MEMO_PROGRAM, false, false),
        (VAULT_0_MINT, false, false),
        (VAULT_1_MINT, false, false),
        (TICK_ARRAY_BITMAP_EXTENSION, true, false),
    ];

    fn reward(base: u8) -> RewardTransferKeys {
        RewardTransferKeys {
            reward_vault: fixed(base),
            recipient: fixed(base + 1),
            reward_mint: fixed(base + 2),
        }
    }

    fn keys(rewards: Vec<RewardTransferKeys>) -> DecreaseLiquidityV2Keys {
        DecreaseLiquidityV2Keys {
            nft_owner: fixed(NFT_OWNER),
            nft_account: fixed(NFT_ACCOUNT),
            personal_position: fixed(PERSONAL_POSITION),
            pool_state: fixed(POOL_STATE),
            protocol_position: fixed(PROTOCOL_POSITION),
            token_vault_0: fixed(TOKEN_VAULT_0),
            token_vault_1: fixed(TOKEN_VAULT_1),
            tick_array_lower: fixed(TICK_ARRAY_LOWER),
            tick_array_upper: fixed(TICK_ARRAY_UPPER),
            recipient_token_account_0: fixed(RECIPIENT_0),
            recipient_token_account_1: fixed(RECIPIENT_1),
            token_program: fixed(TOKEN_PROGRAM),
            token_program_2022: fixed(TOKEN_PROGRAM_2022),
            memo_program: fixed(MEMO_PROGRAM),
            vault_0_mint: fixed(VAULT_0_MINT),
            vault_1_mint: fixed(VAULT_1_MINT),
            tick_array_bitmap_extension: fixed(TICK_ARRAY_BITMAP_EXTENSION),
            reward_accounts: rewards,
        }
    }

    fn assert_fixed_accounts(metas: &[AccountMeta]) {
        for (idx, (role, writable, signer)) in FIXED_ACCOUNTS.iter().enumerate() {
            assert_eq!(metas[idx].pubkey, fixed(*role), "pubkey at slot {idx}");
            assert_eq!(
                metas[idx].is_writable, *writable,
                "is_writable at slot {idx}"
            );
            assert_eq!(metas[idx].is_signer, *signer, "is_signer at slot {idx}");
        }
    }

    #[test]
    fn body_is_disc_u128_u64_u64() {
        let (data, _) = build_decrease_liquidity_v2_instruction_data(&keys(vec![]), 999, 5, 6);
        assert_eq!(data.len(), 8 + 16 + 8 + 8);
        assert_eq!(&data[..8], &RAYDIUM_DECREASE_LIQUIDITY_V2_DISCRIMINATOR);
        assert_eq!(u128::from_le_bytes(data[8..24].try_into().unwrap()), 999);
        assert_eq!(u64::from_le_bytes(data[24..32].try_into().unwrap()), 5);
        assert_eq!(u64::from_le_bytes(data[32..40].try_into().unwrap()), 6);
    }

    #[test]
    fn zero_liquidity_collect_encodes_zero() {
        // decrease(0) is the fee/reward collection path.
        let (data, _) = build_decrease_liquidity_v2_instruction_data(&keys(vec![]), 0, 0, 0);
        assert!(data[8..].iter().all(|b| *b == 0));
    }

    #[test]
    fn arg_edges_max() {
        let (data, _) =
            build_decrease_liquidity_v2_instruction_data(&keys(vec![]), u128::MAX, u64::MAX, 1);
        assert_eq!(&data[8..24], &u128::MAX.to_le_bytes());
        assert_eq!(&data[24..32], &u64::MAX.to_le_bytes());
        assert_eq!(&data[32..40], &1u64.to_le_bytes());
    }

    #[test]
    fn fixed_account_order_without_rewards() {
        let (_, metas) = build_decrease_liquidity_v2_instruction_data(&keys(vec![]), 1, 0, 0);
        assert_eq!(metas.len(), FIXED_ACCOUNTS.len());
        assert_fixed_accounts(&metas);
    }

    #[test]
    fn reward_triples_follow_the_bitmap_extension_in_order() {
        let rewards = vec![reward(0x40), reward(0x50), reward(0x60)];
        let (_, metas) = build_decrease_liquidity_v2_instruction_data(&keys(rewards), 1, 0, 0);
        assert_eq!(metas.len(), FIXED_ACCOUNTS.len() + 9);
        assert_fixed_accounts(&metas);

        for (slot, base) in [0x40u8, 0x50, 0x60].iter().enumerate() {
            let idx = FIXED_ACCOUNTS.len() + slot * 3;
            // reward_vault: writable (source of the payout)
            assert_eq!(metas[idx].pubkey, fixed(*base));
            assert!(metas[idx].is_writable && !metas[idx].is_signer);
            // recipient: writable
            assert_eq!(metas[idx + 1].pubkey, fixed(base + 1));
            assert!(metas[idx + 1].is_writable && !metas[idx + 1].is_signer);
            // reward_mint: readonly
            assert_eq!(metas[idx + 2].pubkey, fixed(base + 2));
            assert!(!metas[idx + 2].is_writable && !metas[idx + 2].is_signer);
        }
    }
}
