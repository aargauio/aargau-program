//! Wire-format tests for `src/utils/orca/decrease_liquidity.rs`
//! (`decrease_liquidity_v2`, ModifyLiquidityV2).

mod discriminator_tests {
    use aargau_manager::constants::ORCA_DECREASE_LIQUIDITY_V2_DISCRIMINATOR;
    use sha2::{Digest, Sha256};

    fn anchor_disc(name: &str) -> [u8; 8] {
        let mut hasher = Sha256::new();
        hasher.update(format!("global:{name}").as_bytes());
        let mut out = [0u8; 8];
        out.copy_from_slice(&hasher.finalize()[..8]);
        out
    }

    #[test]
    fn decrease_liquidity_v2_discriminator_matches_sha256_and_backend() {
        assert_eq!(
            ORCA_DECREASE_LIQUIDITY_V2_DISCRIMINATOR,
            anchor_disc("decrease_liquidity_v2"),
        );
        assert_eq!(
            ORCA_DECREASE_LIQUIDITY_V2_DISCRIMINATOR,
            [58, 127, 188, 62, 79, 82, 196, 96],
        );
    }
}

mod build_decrease_liquidity_v2_tests {
    use aargau_manager::constants::ORCA_DECREASE_LIQUIDITY_V2_DISCRIMINATOR;
    use aargau_manager::utils::orca::decrease_liquidity::build_decrease_liquidity_v2_instruction_data;
    use anchor_lang::prelude::Pubkey;
    use anchor_lang::solana_program::instruction::AccountMeta;

    fn fixed(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn build(liq: u128, min_a: u64, min_b: u64) -> (Vec<u8>, Vec<AccountMeta>) {
        build_decrease_liquidity_v2_instruction_data(
            &fixed(0x01),
            &fixed(0x02),
            &fixed(0x03),
            &fixed(0x04),
            &fixed(0x05),
            &fixed(0x06),
            &fixed(0x07),
            &fixed(0x08),
            &fixed(0x09),
            &fixed(0x0a),
            &fixed(0x0b),
            &fixed(0x0c),
            &fixed(0x0d),
            &fixed(0x0e),
            &fixed(0x0f),
            liq,
            min_a,
            min_b,
        )
    }

    #[test]
    fn body_is_disc_u128_u64_u64_none() {
        let (data, _) = build(999u128, 5u64, 6u64);
        assert_eq!(&data[..8], &ORCA_DECREASE_LIQUIDITY_V2_DISCRIMINATOR);
        assert_eq!(u128::from_le_bytes(data[8..24].try_into().unwrap()), 999);
        assert_eq!(u64::from_le_bytes(data[24..32].try_into().unwrap()), 5);
        assert_eq!(u64::from_le_bytes(data[32..40].try_into().unwrap()), 6);
        assert_eq!(data[40], 0u8);
        assert_eq!(data.len(), 8 + 16 + 8 + 8 + 1);
    }

    // Role markers — unique byte per builder argument (see build() arg order).
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
    const TICK_ARRAY_LOWER: u8 = 0x0e;
    const TICK_ARRAY_UPPER: u8 = 0x0f;

    #[test]
    fn account_meta_order_and_flags_match_modify_liquidity_v2() {
        // Decrease shares the ModifyLiquidityV2 context. Expected
        // (role_byte, is_writable, is_signer) transcribed verbatim from
        // orca-so/whirlpools programs/whirlpool/src/instructions/v2/decrease_liquidity.rs
        // (ModifyLiquidityV2). whirlpool IS `#[account(mut)]`.
        let expected: [(u8, bool, bool); 15] = [
            (WHIRLPOOL, true, false),
            (TOKEN_PROGRAM_A, false, false),
            (TOKEN_PROGRAM_B, false, false),
            (MEMO_PROGRAM, false, false),
            (POSITION_AUTHORITY, false, true),
            (POSITION, true, false),
            (POSITION_TOKEN_ACCOUNT, false, false),
            (TOKEN_MINT_A, false, false),
            (TOKEN_MINT_B, false, false),
            (TOKEN_OWNER_ACCOUNT_A, true, false),
            (TOKEN_OWNER_ACCOUNT_B, true, false),
            (TOKEN_VAULT_A, true, false),
            (TOKEN_VAULT_B, true, false),
            (TICK_ARRAY_LOWER, true, false),
            (TICK_ARRAY_UPPER, true, false),
        ];

        let (_, m) = build(1, 1, 1);
        assert_eq!(m.len(), expected.len());
        for (idx, (role, writable, signer)) in expected.iter().enumerate() {
            assert_eq!(m[idx].pubkey, fixed(*role), "pubkey at slot {idx}");
            assert_eq!(m[idx].is_writable, *writable, "is_writable at slot {idx}");
            assert_eq!(m[idx].is_signer, *signer, "is_signer at slot {idx}");
        }
    }
}
