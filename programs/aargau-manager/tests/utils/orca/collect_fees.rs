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

    // Role markers — each argument gets a UNIQUE byte so we can assert the
    // builder placed it at the index the authoritative struct demands. This is
    // independent of the builder: the expected sequence below is transcribed by
    // hand from CollectFeesV2 `#[derive(Accounts)]`, so a builder reordering
    // (e.g. the old ModifyLiquidityV2 layout) makes the pubkey at some index
    // mismatch and the test fails.
    const WHIRLPOOL: u8 = 0x01;
    const TOKEN_PROGRAM_A: u8 = 0x02;
    const TOKEN_PROGRAM_B: u8 = 0x03;
    const MEMO_PROGRAM: u8 = 0x04;
    const POSITION_AUTHORITY: u8 = 0x05;
    const POSITION: u8 = 0x06;
    const POSITION_TOKEN_ACCOUNT: u8 = 0x07;
    const TOKEN_MINT_A: u8 = 0x08;
    const TOKEN_MINT_B: u8 = 0x09;
    const TOKEN_OWNER_ACCOUNT_A: u8 = 0x0a;
    const TOKEN_OWNER_ACCOUNT_B: u8 = 0x0b;
    const TOKEN_VAULT_A: u8 = 0x0c;
    const TOKEN_VAULT_B: u8 = 0x0d;

    #[test]
    fn account_meta_order_matches_collect_fees_v2() {
        let (_, m) = build();

        // Expected (role_byte, is_writable, is_signer) transcribed verbatim from
        // orca-so/whirlpools programs/whirlpool/src/instructions/v2/collect_fees.rs
        // CollectFeesV2. NOTE: whirlpool is readonly here (Box<Account>, no mut),
        // token programs + memo are LAST, and A/B owner+vault pairs interleave.
        let expected: [(u8, bool, bool); 13] = [
            (WHIRLPOOL, false, false),
            (POSITION_AUTHORITY, false, true),
            (POSITION, true, false),
            (POSITION_TOKEN_ACCOUNT, false, false),
            (TOKEN_MINT_A, false, false),
            (TOKEN_MINT_B, false, false),
            (TOKEN_OWNER_ACCOUNT_A, true, false),
            (TOKEN_VAULT_A, true, false),
            (TOKEN_OWNER_ACCOUNT_B, true, false),
            (TOKEN_VAULT_B, true, false),
            (TOKEN_PROGRAM_A, false, false),
            (TOKEN_PROGRAM_B, false, false),
            (MEMO_PROGRAM, false, false),
        ];

        assert_eq!(m.len(), expected.len());
        for (idx, (role, writable, signer)) in expected.iter().enumerate() {
            assert_eq!(m[idx].pubkey, fixed(*role), "pubkey at slot {idx}");
            assert_eq!(m[idx].is_writable, *writable, "is_writable at slot {idx}");
            assert_eq!(m[idx].is_signer, *signer, "is_signer at slot {idx}");
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

    // Role markers — unique byte per builder argument (see build() arg order).
    const WHIRLPOOL: u8 = 0x01;
    const POSITION_AUTHORITY: u8 = 0x02;
    const POSITION: u8 = 0x03;
    const POSITION_TOKEN_ACCOUNT: u8 = 0x04;
    const REWARD_OWNER_ACCOUNT: u8 = 0x05;
    const REWARD_MINT: u8 = 0x06;
    const REWARD_VAULT: u8 = 0x07;
    const REWARD_TOKEN_PROGRAM: u8 = 0x08;
    const MEMO_PROGRAM: u8 = 0x09;

    #[test]
    fn account_meta_order_matches_collect_reward_v2() {
        let (_, m) = build(0);

        // Expected (role_byte, is_writable, is_signer) transcribed verbatim from
        // orca-so/whirlpools programs/whirlpool/src/instructions/v2/collect_reward.rs
        // CollectRewardV2. whirlpool is readonly (Box<Account>, no mut).
        let expected: [(u8, bool, bool); 9] = [
            (WHIRLPOOL, false, false),
            (POSITION_AUTHORITY, false, true),
            (POSITION, true, false),
            (POSITION_TOKEN_ACCOUNT, false, false),
            (REWARD_OWNER_ACCOUNT, true, false),
            (REWARD_MINT, false, false),
            (REWARD_VAULT, true, false),
            (REWARD_TOKEN_PROGRAM, false, false),
            (MEMO_PROGRAM, false, false),
        ];

        assert_eq!(m.len(), expected.len());
        for (idx, (role, writable, signer)) in expected.iter().enumerate() {
            assert_eq!(m[idx].pubkey, fixed(*role), "pubkey at slot {idx}");
            assert_eq!(m[idx].is_writable, *writable, "is_writable at slot {idx}");
            assert_eq!(m[idx].is_signer, *signer, "is_signer at slot {idx}");
        }
    }
}
