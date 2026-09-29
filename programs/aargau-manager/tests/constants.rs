//! Tests for `src/constants.rs` — regression guard for program-wide constants.
//!
//! Any accidental change to a constant that affects on-chain behavior
//! (timeouts, limits, encoding) will be caught here.

#[cfg(test)]
mod constants_tests {
    use aargau_manager::constants::*;

    #[test]
    fn test_max_meteora_bins_is_69() {
        // Meteora DLMM caps at 69 bins per position; validated upfront before any CPI.
        assert_eq!(MAX_METEORA_BINS, 69);
    }

    #[test]
    fn test_pending_rebalance_timeout_is_240_seconds() {
        // A pending rebalance older than 240 s is considered stale and can be retried or cancelled.
        assert_eq!(PENDING_REBALANCE_TIMEOUT_SECS, 240);
    }

    #[test]
    fn test_bps_divisor_is_10000() {
        // All fee and percentage math divides by BPS_DIVISOR; must stay 10_000.
        assert_eq!(BPS_DIVISOR, 10_000);
    }

    #[test]
    fn test_program_version_encoding_v0_1_0() {
        // v0.1.0 is encoded as: major * 10_000 + minor * 100 + patch = 0 + 100 + 0 = 100
        assert_eq!(PROGRAM_VERSION, 100);
    }

    #[test]
    fn test_program_version_encoding_formula() {
        // Verify the encoding formula: major * 10_000 + minor * 100 + patch.
        // For v0.1.0: 0 * 10_000 + 1 * 100 + 0 = 100.
        let major: u32 = 0;
        let minor: u32 = 1;
        let patch: u32 = 0;
        let encoded = major * 10_000 + minor * 100 + patch;
        assert_eq!(encoded, PROGRAM_VERSION);
    }

    #[test]
    fn test_bps_divisor_u16_equals_bps_divisor() {
        // BPS_DIVISOR_U16 must agree with BPS_DIVISOR to keep fee math consistent.
        assert_eq!(BPS_DIVISOR_U16 as u64, BPS_DIVISOR);
    }

    #[test]
    fn test_bps_divisor_u16_is_10000() {
        assert_eq!(BPS_DIVISOR_U16, 10_000u16);
    }
}

#[cfg(test)]
mod program_id_tests {
    use aargau_manager::constants::*;
    use anchor_lang::prelude::Pubkey;
    use std::str::FromStr;

    #[test]
    fn test_orca_whirlpool_program_id_string() {
        let expected = Pubkey::from_str("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc").unwrap();
        assert_eq!(ORCA_WHIRLPOOL_PROGRAM_ID, expected);
    }

    #[test]
    fn test_raydium_clmm_program_id_string() {
        let expected = Pubkey::from_str("CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK").unwrap();
        assert_eq!(RAYDIUM_CLMM_PROGRAM_ID, expected);
    }

    #[test]
    fn test_meteora_dlmm_program_id_string() {
        let expected = Pubkey::from_str("LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo").unwrap();
        assert_eq!(METEORA_DLMM_PROGRAM_ID, expected);
    }

    #[test]
    fn test_all_program_ids_are_distinct() {
        // Swapping program IDs would cause all CPI owner checks to fail silently.
        assert_ne!(ORCA_WHIRLPOOL_PROGRAM_ID, RAYDIUM_CLMM_PROGRAM_ID);
        assert_ne!(ORCA_WHIRLPOOL_PROGRAM_ID, METEORA_DLMM_PROGRAM_ID);
        assert_ne!(RAYDIUM_CLMM_PROGRAM_ID, METEORA_DLMM_PROGRAM_ID);
    }
}

#[cfg(test)]
mod discriminator_tests {
    use aargau_manager::constants::*;

    #[test]
    fn test_orca_whirlpool_discriminator() {
        assert_eq!(
            ORCA_WHIRLPOOL_DISCRIMINATOR,
            [63u8, 149, 209, 12, 225, 128, 99, 9]
        );
    }

    #[test]
    fn test_raydium_pool_state_discriminator() {
        assert_eq!(
            RAYDIUM_POOL_STATE_DISCRIMINATOR,
            [247u8, 237, 227, 245, 215, 195, 222, 70]
        );
    }

    #[test]
    fn test_meteora_lb_pair_discriminator() {
        assert_eq!(
            METEORA_LB_PAIR_DISCRIMINATOR,
            [33u8, 11, 49, 98, 181, 101, 177, 13]
        );
    }

    #[test]
    fn test_all_discriminators_are_exactly_8_bytes() {
        assert_eq!(ORCA_WHIRLPOOL_DISCRIMINATOR.len(), 8);
        assert_eq!(RAYDIUM_POOL_STATE_DISCRIMINATOR.len(), 8);
        assert_eq!(METEORA_LB_PAIR_DISCRIMINATOR.len(), 8);
    }

    #[test]
    fn test_all_discriminators_are_distinct() {
        assert_ne!(
            ORCA_WHIRLPOOL_DISCRIMINATOR,
            RAYDIUM_POOL_STATE_DISCRIMINATOR
        );
        assert_ne!(ORCA_WHIRLPOOL_DISCRIMINATOR, METEORA_LB_PAIR_DISCRIMINATOR);
        assert_ne!(
            RAYDIUM_POOL_STATE_DISCRIMINATOR,
            METEORA_LB_PAIR_DISCRIMINATOR
        );
    }
}

#[cfg(test)]
mod mint_offset_tests {
    use aargau_manager::constants::*;

    #[test]
    fn test_orca_mint_a_offset() {
        assert_eq!(ORCA_MINT_A_OFFSET, 101);
    }

    #[test]
    fn test_orca_mint_b_offset() {
        assert_eq!(ORCA_MINT_B_OFFSET, 181);
    }

    #[test]
    fn test_raydium_mint_a_offset() {
        assert_eq!(RAYDIUM_MINT_A_OFFSET, 73);
    }

    #[test]
    fn test_raydium_mint_b_offset() {
        assert_eq!(RAYDIUM_MINT_B_OFFSET, 105);
    }

    #[test]
    fn test_meteora_mint_a_offset() {
        // Absolute offset into the raw LbPair account buffer (the 8-byte
        // Anchor discriminator is included). Mirrors
        // `LB_PAIR_TOKEN_X_MINT_OFFSET = 88` in `utils::meteora::lb_pair_view`
        // and the production backend decoder.
        assert_eq!(METEORA_MINT_A_OFFSET, 88);
    }

    #[test]
    fn test_meteora_mint_b_offset() {
        // Absolute offset into the raw LbPair account buffer.
        assert_eq!(METEORA_MINT_B_OFFSET, 120);
    }

    #[test]
    fn test_mint_b_offset_is_always_after_mint_a() {
        // Mint B must come after Mint A in every protocol's account layout.
        // `const { … }` forces evaluation at compile time so the lint sees
        // these as compile-time invariants, not runtime assertions.
        const _: () = assert!(ORCA_MINT_B_OFFSET > ORCA_MINT_A_OFFSET);
        const _: () = assert!(RAYDIUM_MINT_B_OFFSET > RAYDIUM_MINT_A_OFFSET);
        const _: () = assert!(METEORA_MINT_B_OFFSET > METEORA_MINT_A_OFFSET);
    }
}

#[cfg(test)]
mod raydium_discriminator_tests {
    use aargau_manager::constants::*;
    use sha2::{Digest, Sha256};

    /// Anchor discriminator = `sha256("<namespace>:<name>")[..8]`.
    fn anchor_disc(namespace: &str, name: &str) -> [u8; 8] {
        let mut hasher = Sha256::new();
        hasher.update(format!("{namespace}:{name}").as_bytes());
        let mut out = [0u8; 8];
        out.copy_from_slice(&hasher.finalize()[..8]);
        out
    }

    #[test]
    fn instruction_discriminators_match_sha256() {
        assert_eq!(
            RAYDIUM_OPEN_POSITION_WITH_TOKEN22_NFT_DISCRIMINATOR,
            anchor_disc("global", "open_position_with_token22_nft"),
        );
        assert_eq!(
            RAYDIUM_INCREASE_LIQUIDITY_V2_DISCRIMINATOR,
            anchor_disc("global", "increase_liquidity_v2"),
        );
        assert_eq!(
            RAYDIUM_DECREASE_LIQUIDITY_V2_DISCRIMINATOR,
            anchor_disc("global", "decrease_liquidity_v2"),
        );
        assert_eq!(
            RAYDIUM_CLOSE_POSITION_DISCRIMINATOR,
            anchor_disc("global", "close_position"),
        );
    }

    #[test]
    fn account_discriminators_match_sha256() {
        assert_eq!(
            RAYDIUM_POOL_STATE_DISCRIMINATOR,
            anchor_disc("account", "PoolState"),
        );
        assert_eq!(
            RAYDIUM_PERSONAL_POSITION_DISCRIMINATOR,
            anchor_disc("account", "PersonalPositionState"),
        );
        assert_eq!(
            RAYDIUM_TICK_ARRAY_DISCRIMINATOR,
            anchor_disc("account", "TickArrayState"),
        );
        assert_eq!(
            RAYDIUM_TICK_ARRAY_BITMAP_EXTENSION_DISCRIMINATOR,
            anchor_disc("account", "TickArrayBitmapExtension"),
        );
    }

    #[test]
    fn instruction_discriminators_match_mainnet_bytes() {
        // Literal bytes pinned too, so an accidental edit of a constant fails
        // here even if the sha256 helper were changed alongside it.
        assert_eq!(
            RAYDIUM_OPEN_POSITION_WITH_TOKEN22_NFT_DISCRIMINATOR,
            [77, 255, 174, 82, 125, 29, 201, 46]
        );
        assert_eq!(
            RAYDIUM_CLOSE_POSITION_DISCRIMINATOR,
            [123, 134, 81, 0, 49, 68, 98, 98]
        );
    }
}

#[cfg(test)]
mod raydium_layout_tests {
    use aargau_manager::constants::*;

    #[test]
    fn seeds_match_on_chain_program() {
        assert_eq!(RAYDIUM_POSITION_SEED, b"position");
        assert_eq!(RAYDIUM_TICK_ARRAY_SEED, b"tick_array");
        assert_eq!(
            RAYDIUM_TICK_ARRAY_BITMAP_EXTENSION_SEED,
            b"pool_tick_array_bitmap_extension"
        );
    }

    #[test]
    fn tick_array_and_reward_sizes() {
        assert_eq!(RAYDIUM_TICK_ARRAY_SIZE, 60);
        assert_eq!(RAYDIUM_REWARD_SLOTS, 3);
        assert_eq!(RAYDIUM_REWARD_ACCOUNTS_PER_SLOT, 3);
    }

    #[test]
    fn pool_state_offsets() {
        assert_eq!(RAYDIUM_POOL_TOKEN_MINT_0_OFFSET, 73);
        assert_eq!(RAYDIUM_POOL_TOKEN_MINT_1_OFFSET, 105);
        assert_eq!(RAYDIUM_POOL_TOKEN_VAULT_0_OFFSET, 137);
        assert_eq!(RAYDIUM_POOL_TOKEN_VAULT_1_OFFSET, 169);
        assert_eq!(RAYDIUM_POOL_TICK_SPACING_OFFSET, 235);
        assert_eq!(RAYDIUM_POOL_REWARD_INFOS_OFFSET, 397);
        assert_eq!(RAYDIUM_POOL_REWARD_INFO_STRIDE, 169);
        assert_eq!(RAYDIUM_REWARD_INFO_TOKEN_MINT_RELATIVE_OFFSET, 57);
        assert_eq!(RAYDIUM_REWARD_INFO_TOKEN_VAULT_RELATIVE_OFFSET, 89);
    }

    #[test]
    fn pool_state_mint_offsets_agree_with_create_vault_constants() {
        // Both name the same absolute bytes of `PoolState`.
        assert_eq!(RAYDIUM_POOL_TOKEN_MINT_0_OFFSET, RAYDIUM_MINT_A_OFFSET);
        assert_eq!(RAYDIUM_POOL_TOKEN_MINT_1_OFFSET, RAYDIUM_MINT_B_OFFSET);
    }

    #[test]
    fn pool_state_packed_layout_is_contiguous() {
        // mint0 → mint1 → vault0 → vault1 are back-to-back pubkeys.
        assert_eq!(
            RAYDIUM_POOL_TOKEN_MINT_1_OFFSET,
            RAYDIUM_POOL_TOKEN_MINT_0_OFFSET + 32
        );
        assert_eq!(
            RAYDIUM_POOL_TOKEN_VAULT_0_OFFSET,
            RAYDIUM_POOL_TOKEN_MINT_1_OFFSET + 32
        );
        assert_eq!(
            RAYDIUM_POOL_TOKEN_VAULT_1_OFFSET,
            RAYDIUM_POOL_TOKEN_VAULT_0_OFFSET + 32
        );
        // RewardInfo: state 1 + open 8 + end 8 + last_update 8 + emissions 16
        // + emitted 8 + claimed 8 = 57 → mint 32 → vault 32 → authority 32
        // → growth_global 16 = 169.
        assert_eq!(
            RAYDIUM_REWARD_INFO_TOKEN_MINT_RELATIVE_OFFSET,
            1 + 8 * 3 + 16 + 8 + 8
        );
        assert_eq!(
            RAYDIUM_REWARD_INFO_TOKEN_VAULT_RELATIVE_OFFSET,
            RAYDIUM_REWARD_INFO_TOKEN_MINT_RELATIVE_OFFSET + 32
        );
        assert_eq!(
            RAYDIUM_POOL_REWARD_INFO_STRIDE,
            RAYDIUM_REWARD_INFO_TOKEN_VAULT_RELATIVE_OFFSET + 32 + 32 + 16
        );
    }

    #[test]
    fn personal_position_offsets() {
        assert_eq!(RAYDIUM_PERSONAL_POSITION_LEN, 281);
        assert_eq!(RAYDIUM_PERSONAL_POSITION_NFT_MINT_OFFSET, 9);
        assert_eq!(RAYDIUM_PERSONAL_POSITION_POOL_ID_OFFSET, 41);
        assert_eq!(RAYDIUM_PERSONAL_POSITION_TICK_LOWER_OFFSET, 73);
        assert_eq!(RAYDIUM_PERSONAL_POSITION_TICK_UPPER_OFFSET, 77);
        assert_eq!(RAYDIUM_PERSONAL_POSITION_LIQUIDITY_OFFSET, 81);
        assert_eq!(RAYDIUM_PERSONAL_POSITION_FEES_OWED_0_OFFSET, 129);
        assert_eq!(RAYDIUM_PERSONAL_POSITION_FEES_OWED_1_OFFSET, 137);
        assert_eq!(
            RAYDIUM_PERSONAL_POSITION_REWARD_OWED_OFFSETS,
            [161, 185, 209]
        );
    }

    #[test]
    fn personal_position_len_matches_raydium_formula() {
        // 8 disc + 1 bump + 32 nft_mint + 32 pool_id + 4 + 4 ticks + 16 liquidity
        // + 16 + 16 fee growth + 8 + 8 fees owed + 3 * 24 rewards + 64 tail.
        assert_eq!(
            RAYDIUM_PERSONAL_POSITION_LEN,
            8 + 1 + 32 + 32 + 4 + 4 + 16 + 16 + 16 + 8 + 8 + 24 * 3 + 64
        );
        // reward_infos start at 145, stride 24, owed after the u128 growth.
        for (slot, offset) in RAYDIUM_PERSONAL_POSITION_REWARD_OWED_OFFSETS
            .iter()
            .enumerate()
        {
            assert_eq!(*offset, 145 + slot * 24 + 16);
        }
    }

    #[test]
    fn protocol_position_placeholder_is_the_clmm_program() {
        assert_eq!(
            RAYDIUM_PROTOCOL_POSITION_PLACEHOLDER,
            RAYDIUM_CLMM_PROGRAM_ID
        );
    }

    #[test]
    fn spl_token_program_id() {
        use anchor_lang::prelude::Pubkey;
        use std::str::FromStr;
        assert_eq!(
            SPL_TOKEN_PROGRAM_ID,
            Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap()
        );
        assert_eq!(SPL_TOKEN_PROGRAM_ID, anchor_spl::token::ID);
    }
}
