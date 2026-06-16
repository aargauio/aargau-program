//! Wire-format tests for `src/utils/orca/collect_fees.rs`
//! (`collect_fees_v2` + `collect_reward_v2`).

mod discriminator_tests {
    use aargau_manager::constants::{
        ORCA_COLLECT_FEES_V2_DISCRIMINATOR, ORCA_COLLECT_REWARD_V2_DISCRIMINATOR,
    };
    use sha2::{Digest, Sha256};

    fn anchor_disc(name: &str) -> [u8; 8] {
        let mut hasher = Sha256::new();
        hasher.update(format!("global:{name}").as_bytes());
        let mut out = [0u8; 8];
        out.copy_from_slice(&hasher.finalize()[..8]);
        out
    }

    #[test]
    fn collect_fees_v2_discriminator_matches_sha256() {
        // Re-derived here (the backend uses the v1 variant); pinned to detect
        // any accidental edit of the baked-in bytes.
        assert_eq!(
            ORCA_COLLECT_FEES_V2_DISCRIMINATOR,
            anchor_disc("collect_fees_v2"),
        );
        assert_eq!(
            ORCA_COLLECT_FEES_V2_DISCRIMINATOR,
            [207, 117, 95, 191, 229, 180, 226, 15],
        );
    }

    #[test]
    fn collect_reward_v2_discriminator_matches_sha256() {
        assert_eq!(
            ORCA_COLLECT_REWARD_V2_DISCRIMINATOR,
            anchor_disc("collect_reward_v2"),
        );
        assert_eq!(
            ORCA_COLLECT_REWARD_V2_DISCRIMINATOR,
            [177, 107, 37, 180, 160, 19, 49, 209],
        );
    }
}

mod build_collect_fees_v2_tests {
    use aargau_manager::constants::ORCA_COLLECT_FEES_V2_DISCRIMINATOR;
    use aargau_manager::utils::orca::collect_fees::build_collect_fees_v2_instruction_data;
    use anchor_lang::prelude::Pubkey;
    use anchor_lang::solana_program::instruction::AccountMeta;

    fn fixed(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn build() -> (Vec<u8>, Vec<AccountMeta>) {
        build_collect_fees_v2_instruction_data(
            &fixed(0x01), // whirlpool
            &fixed(0x02), // token_program_a
            &fixed(0x03), // token_program_b
            &fixed(0x04), // memo_program
            &fixed(0x05), // position_authority
            &fixed(0x06), // position
            &fixed(0x07), // position_token_account
            &fixed(0x08), // token_mint_a
            &fixed(0x09), // token_mint_b
            &fixed(0x0a), // token_owner_account_a
            &fixed(0x0b), // token_owner_account_b
            &fixed(0x0c), // token_vault_a
            &fixed(0x0d), // token_vault_b
        )
    }

    #[test]
    fn body_is_disc_plus_none() {
        let (data, _) = build();
        assert_eq!(&data[..8], &ORCA_COLLECT_FEES_V2_DISCRIMINATOR);
        assert_eq!(data[8], 0u8, "remaining_accounts_info = None");
        assert_eq!(data.len(), 8 + 1);
    }

    #[test]
    fn account_meta_order_matches_collect_fees_v2() {
        let (_, m) = build();
        assert_eq!(m.len(), 13);
        for (idx, meta) in m.iter().enumerate() {
            assert_eq!(meta.pubkey, fixed(0x01 + idx as u8));
        }
        // whirlpool: writable
        assert!(m[0].is_writable && !m[0].is_signer);
        // token programs + memo: readonly
        for idx in [1, 2, 3] {
            assert!(!m[idx].is_writable && !m[idx].is_signer);
        }
        // position_authority: readonly + signer
        assert!(m[4].is_signer && !m[4].is_writable);
        // position: writable
        assert!(m[5].is_writable);
        // position_token_account + mints: readonly
        for idx in [6, 7, 8] {
            assert!(!m[idx].is_writable && !m[idx].is_signer);
        }
        // owner accounts + pool vaults: writable
        for idx in [9, 10, 11, 12] {
            assert!(m[idx].is_writable && !m[idx].is_signer, "slot {idx}");
        }
    }
}

mod build_collect_reward_v2_tests {
    use aargau_manager::constants::ORCA_COLLECT_REWARD_V2_DISCRIMINATOR;
    use aargau_manager::utils::orca::collect_fees::build_collect_reward_v2_instruction_data;
    use anchor_lang::prelude::Pubkey;
    use anchor_lang::solana_program::instruction::AccountMeta;

    fn fixed(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn build(reward_index: u8) -> (Vec<u8>, Vec<AccountMeta>) {
        build_collect_reward_v2_instruction_data(
            &fixed(0x01), // whirlpool
            &fixed(0x02), // position_authority
            &fixed(0x03), // position
            &fixed(0x04), // position_token_account
            &fixed(0x05), // reward_owner_account
            &fixed(0x06), // reward_mint
            &fixed(0x07), // reward_vault
            &fixed(0x08), // reward_token_program
            &fixed(0x09), // memo_program
            reward_index,
        )
    }

    #[test]
    fn body_is_disc_index_none() {
        let (data, _) = build(2);
        assert_eq!(&data[..8], &ORCA_COLLECT_REWARD_V2_DISCRIMINATOR);
        assert_eq!(data[8], 2u8, "reward_index");
        assert_eq!(data[9], 0u8, "remaining_accounts_info = None");
        assert_eq!(data.len(), 8 + 1 + 1);
    }

    #[test]
    fn account_meta_order_matches_collect_reward_v2() {
        let (_, m) = build(0);
        assert_eq!(m.len(), 9);
        for (idx, meta) in m.iter().enumerate() {
            assert_eq!(meta.pubkey, fixed(0x01 + idx as u8));
        }
        // whirlpool: writable
        assert!(m[0].is_writable && !m[0].is_signer);
        // position_authority: readonly + signer
        assert!(m[1].is_signer && !m[1].is_writable);
        // position: writable
        assert!(m[2].is_writable);
        // position_token_account: readonly
        assert!(!m[3].is_writable && !m[3].is_signer);
        // reward_owner_account: writable
        assert!(m[4].is_writable);
        // reward_mint: readonly
        assert!(!m[5].is_writable);
        // reward_vault: writable
        assert!(m[6].is_writable);
        // reward_token_program + memo: readonly
        assert!(!m[7].is_writable && !m[8].is_writable);
    }
}
