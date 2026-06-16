//! Wire-format tests for `src/utils/orca/increase_liquidity.rs`
//! (`increase_liquidity_v2`, ModifyLiquidityV2).

mod discriminator_tests {
    use aargau_manager::constants::ORCA_INCREASE_LIQUIDITY_V2_DISCRIMINATOR;
    use sha2::{Digest, Sha256};

    fn anchor_disc(name: &str) -> [u8; 8] {
        let mut hasher = Sha256::new();
        hasher.update(format!("global:{name}").as_bytes());
        let mut out = [0u8; 8];
        out.copy_from_slice(&hasher.finalize()[..8]);
        out
    }

    #[test]
    fn increase_liquidity_v2_discriminator_matches_sha256_and_backend() {
        assert_eq!(
            ORCA_INCREASE_LIQUIDITY_V2_DISCRIMINATOR,
            anchor_disc("increase_liquidity_v2"),
        );
        assert_eq!(
            ORCA_INCREASE_LIQUIDITY_V2_DISCRIMINATOR,
            [133, 29, 89, 223, 69, 238, 176, 10],
        );
    }
}

mod build_increase_liquidity_v2_tests {
    use aargau_manager::constants::ORCA_INCREASE_LIQUIDITY_V2_DISCRIMINATOR;
    use aargau_manager::utils::orca::increase_liquidity::build_increase_liquidity_v2_instruction_data;
    use anchor_lang::prelude::Pubkey;
    use anchor_lang::solana_program::instruction::AccountMeta;

    fn fixed(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn build(liq: u128, max_a: u64, max_b: u64) -> (Vec<u8>, Vec<AccountMeta>) {
        build_increase_liquidity_v2_instruction_data(
            &fixed(0x01), // whirlpool
            &fixed(0x02), // token_program_a
            &fixed(0x03), // token_program_b
            &fixed(0x04), // memo_program
            &fixed(0x05), // position_authority (vault)
            &fixed(0x06), // position
            &fixed(0x07), // position_token_account
            &fixed(0x08), // token_mint_a
            &fixed(0x09), // token_mint_b
            &fixed(0x0a), // token_owner_account_a (vault ATA)
            &fixed(0x0b), // token_owner_account_b (vault ATA)
            &fixed(0x0c), // token_vault_a (pool)
            &fixed(0x0d), // token_vault_b (pool)
            &fixed(0x0e), // tick_array_lower
            &fixed(0x0f), // tick_array_upper
            liq,
            max_a,
            max_b,
        )
    }

    #[test]
    fn body_is_disc_u128_u64_u64_none() {
        let (data, _) = build(123_456_789u128, 1_000u64, 2_000u64);
        assert_eq!(&data[..8], &ORCA_INCREASE_LIQUIDITY_V2_DISCRIMINATOR);
        assert_eq!(
            u128::from_le_bytes(data[8..24].try_into().unwrap()),
            123_456_789
        );
        assert_eq!(u64::from_le_bytes(data[24..32].try_into().unwrap()), 1_000);
        assert_eq!(u64::from_le_bytes(data[32..40].try_into().unwrap()), 2_000);
        // remaining_accounts_info = None → single trailing 0u8.
        assert_eq!(data[40], 0u8);
        assert_eq!(data.len(), 8 + 16 + 8 + 8 + 1);
    }

    #[test]
    fn account_meta_order_matches_modify_liquidity_v2() {
        let (_, m) = build(1, 1, 1);
        assert_eq!(m.len(), 15);
        for (idx, meta) in m.iter().enumerate() {
            assert_eq!(meta.pubkey, fixed(0x01 + idx as u8), "slot {idx} pubkey");
        }
    }

    #[test]
    fn flags_match_on_chain_layout() {
        let (_, m) = build(1, 1, 1);
        // whirlpool: writable
        assert!(m[0].is_writable && !m[0].is_signer);
        // token_program_a/b, memo: readonly
        for idx in [1, 2, 3] {
            assert!(!m[idx].is_writable && !m[idx].is_signer);
        }
        // position_authority (vault): readonly + signer (signs via seeds)
        assert!(!m[4].is_writable && m[4].is_signer);
        // position: writable
        assert!(m[5].is_writable && !m[5].is_signer);
        // position_token_account: readonly
        assert!(!m[6].is_writable && !m[6].is_signer);
        // token_mint_a/b: readonly
        assert!(!m[7].is_writable && !m[8].is_writable);
        // owner accounts + pool vaults + tick arrays: writable
        for idx in [9, 10, 11, 12, 13, 14] {
            assert!(
                m[idx].is_writable && !m[idx].is_signer,
                "slot {idx} writable"
            );
        }
    }
}
