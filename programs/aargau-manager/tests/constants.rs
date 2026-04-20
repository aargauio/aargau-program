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
        let expected =
            Pubkey::from_str("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc").unwrap();
        assert_eq!(ORCA_WHIRLPOOL_PROGRAM_ID, expected);
    }

    #[test]
    fn test_raydium_clmm_program_id_string() {
        let expected =
            Pubkey::from_str("CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK").unwrap();
        assert_eq!(RAYDIUM_CLMM_PROGRAM_ID, expected);
    }

    #[test]
    fn test_meteora_dlmm_program_id_string() {
        let expected =
            Pubkey::from_str("LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo").unwrap();
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
        assert_ne!(ORCA_WHIRLPOOL_DISCRIMINATOR, RAYDIUM_POOL_STATE_DISCRIMINATOR);
        assert_ne!(ORCA_WHIRLPOOL_DISCRIMINATOR, METEORA_LB_PAIR_DISCRIMINATOR);
        assert_ne!(RAYDIUM_POOL_STATE_DISCRIMINATOR, METEORA_LB_PAIR_DISCRIMINATOR);
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
        assert_eq!(METEORA_MINT_A_OFFSET, 0);
    }

    #[test]
    fn test_meteora_mint_b_offset() {
        assert_eq!(METEORA_MINT_B_OFFSET, 32);
    }

    #[test]
    fn test_mint_b_offset_is_always_after_mint_a() {
        // Mint B must come after Mint A in every protocol's account layout.
        assert!(ORCA_MINT_B_OFFSET > ORCA_MINT_A_OFFSET);
        assert!(RAYDIUM_MINT_B_OFFSET > RAYDIUM_MINT_A_OFFSET);
        assert!(METEORA_MINT_B_OFFSET > METEORA_MINT_A_OFFSET);
    }
}
