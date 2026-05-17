//! Tests for `src/utils/meteora/rebalance_liquidity.rs` — pure byte
//! validation of the `rebalance_liquidity` instruction wire format.
//!
//! ## Why these tests are the verification gate for S1 + S4
//!
//! Unlike `claim_fee2` / `add_liquidity_by_strategy2` / `remove_liquidity_by_range2`
//! (each cross-validated against the public Meteora DLMM IDL), wallet-mode
//! tooling for `rebalance_liquidity` is sparse — most clients compose
//! `remove + add` as two separate transactions instead. There is therefore
//! no widely-published reference for the account ordering or the Borsh body.
//!
//! Until a mainnet/devnet replay locks the layout (see ignored mollusk test
//! in `tests/instructions/vault/manual_rebalance_meteora.rs`), these
//! byte-level assertions are the contract: any change to the helper that
//! affects what we send to the DLMM program MUST fail this file first.

mod build_rebalance_liquidity_instruction_data_tests {
    use aargau_manager::constants::{
        METEORA_REBALANCE_LIQUIDITY_DISCRIMINATOR, METEORA_REBALANCE_SHOULD_CLAIM_FEE,
    };
    use aargau_manager::utils::meteora::add_liquidity::METEORA_STRATEGY_SPOT_IMBALANCED;
    use aargau_manager::utils::meteora::rebalance_liquidity::{
        build_rebalance_liquidity_instruction_data, RebalanceLiquidityArgs,
    };

    fn sample_args() -> RebalanceLiquidityArgs {
        RebalanceLiquidityArgs {
            old_lower_bin_id: -100,
            new_lower_bin_id: -50,
            new_upper_bin_id: 50,
            amount_x: 1_000_000,
            amount_y: 2_000_000,
            active_id: 0,
            max_active_bin_slippage: 3,
            strategy_type: METEORA_STRATEGY_SPOT_IMBALANCED,
            should_claim_fee: METEORA_REBALANCE_SHOULD_CLAIM_FEE,
        }
    }

    /// Total payload length per module-doc:
    /// 8 disc + 4 new_lower + 4 new_upper + 8 amount_x + 8 amount_y
    /// + 4 active_id + 4 slippage + 4 min_bin + 4 max_bin + 1 strategy
    /// + 64 strategy_params + 1 should_claim_fee + 8 empty-RAI
    const EXPECTED_PAYLOAD_LEN: usize = 8 + 4 + 4 + 8 + 8 + 4 + 4 + 4 + 4 + 1 + 64 + 1 + 8;

    #[test]
    fn payload_starts_with_rebalance_liquidity_discriminator() {
        let data = build_rebalance_liquidity_instruction_data(&sample_args());
        assert_eq!(&data[..8], &METEORA_REBALANCE_LIQUIDITY_DISCRIMINATOR);
    }

    #[test]
    fn payload_total_length_matches_spec() {
        let data = build_rebalance_liquidity_instruction_data(&sample_args());
        assert_eq!(data.len(), EXPECTED_PAYLOAD_LEN);
    }

    #[test]
    fn encodes_new_lower_then_upper_after_discriminator() {
        let args = sample_args();
        let data = build_rebalance_liquidity_instruction_data(&args);
        let lower: [u8; 4] = data[8..12].try_into().unwrap();
        let upper: [u8; 4] = data[12..16].try_into().unwrap();
        assert_eq!(i32::from_le_bytes(lower), args.new_lower_bin_id);
        assert_eq!(i32::from_le_bytes(upper), args.new_upper_bin_id);
    }

    #[test]
    fn encodes_amounts_as_u64_le() {
        let args = sample_args();
        let data = build_rebalance_liquidity_instruction_data(&args);
        let amount_x: [u8; 8] = data[16..24].try_into().unwrap();
        let amount_y: [u8; 8] = data[24..32].try_into().unwrap();
        assert_eq!(u64::from_le_bytes(amount_x), args.amount_x);
        assert_eq!(u64::from_le_bytes(amount_y), args.amount_y);
    }

    #[test]
    fn encodes_active_id_and_slippage_as_i32_le() {
        let mut args = sample_args();
        args.active_id = -77;
        args.max_active_bin_slippage = 9;
        let data = build_rebalance_liquidity_instruction_data(&args);
        let active: [u8; 4] = data[32..36].try_into().unwrap();
        let slippage: [u8; 4] = data[36..40].try_into().unwrap();
        assert_eq!(i32::from_le_bytes(active), -77);
        assert_eq!(i32::from_le_bytes(slippage), 9);
    }

    #[test]
    fn strategy_parameters_min_max_match_new_range() {
        // The strategy sub-struct min/max MUST mirror the new range so
        // Meteora distributes liquidity across the bins the caller asked for
        // (not a stale or wider range).
        let args = sample_args();
        let data = build_rebalance_liquidity_instruction_data(&args);
        let min_bin: [u8; 4] = data[40..44].try_into().unwrap();
        let max_bin: [u8; 4] = data[44..48].try_into().unwrap();
        assert_eq!(i32::from_le_bytes(min_bin), args.new_lower_bin_id);
        assert_eq!(i32::from_le_bytes(max_bin), args.new_upper_bin_id);
    }

    #[test]
    fn strategy_type_byte_is_present() {
        let data = build_rebalance_liquidity_instruction_data(&sample_args());
        assert_eq!(data[48], METEORA_STRATEGY_SPOT_IMBALANCED);
    }

    #[test]
    fn strategy_parameteres_padding_is_zero_filled_64_bytes() {
        let data = build_rebalance_liquidity_instruction_data(&sample_args());
        assert!(
            data[49..113].iter().all(|byte| *byte == 0),
            "strategy_parameters.parameteres MUST be 64 zero bytes",
        );
    }

    /// **Double-fee prevention invariant.**
    ///
    /// `manual_rebalance` always runs `claim_fee2` (with treasury performance
    /// split) BEFORE this CPI. Passing `should_claim_fee = true` would tell
    /// the DLMM program to deposit the just-claimed fees into the position
    /// owner's ATAs (the vault), letting the user double-harvest the same
    /// fees in the next `CollectFees` call. The constant pins the flag to
    /// `false` at every call site.
    #[test]
    fn should_claim_fee_byte_is_false_when_using_constant() {
        let args = sample_args();
        assert!(!args.should_claim_fee);
        let data = build_rebalance_liquidity_instruction_data(&args);
        // After 64 padding bytes (49..113), the should_claim_fee bool sits at
        // offset 113.
        assert_eq!(
            data[113], 0u8,
            "METEORA_REBALANCE_SHOULD_CLAIM_FEE = false MUST serialise as 0",
        );
    }

    /// Negative control: confirm the byte ACTUALLY flips when the flag is
    /// flipped — guards against an accidental hard-coded `0` overriding the
    /// arg. If this assertion ever needs relaxing, the double-fee invariant
    /// is broken.
    #[test]
    fn should_claim_fee_byte_flips_to_one_when_arg_is_true() {
        let mut args = sample_args();
        args.should_claim_fee = true;
        let data = build_rebalance_liquidity_instruction_data(&args);
        assert_eq!(data[113], 1u8);
    }

    #[test]
    fn payload_ends_with_empty_transfer_hook_remaining_accounts_info() {
        let data = build_rebalance_liquidity_instruction_data(&sample_args());
        // Last 8 bytes = RAI tail: u32 LE 2 (slices.len) + (0,0) + (1,0).
        let tail = &data[EXPECTED_PAYLOAD_LEN - 8..];
        assert_eq!(&tail[..4], &2u32.to_le_bytes());
        assert_eq!(tail[4], 0); // TransferHookX variant
        assert_eq!(tail[5], 0); // len = 0
        assert_eq!(tail[6], 1); // TransferHookY variant
        assert_eq!(tail[7], 0); // len = 0
    }
}

mod build_rebalance_liquidity_account_metas_tests {
    use aargau_manager::utils::meteora::rebalance_liquidity::build_rebalance_liquidity_account_metas;
    use anchor_lang::prelude::Pubkey;

    /// Slot bindings — match the table in the module doc of
    /// `src/utils/meteora/rebalance_liquidity.rs`.
    const POSITION: u8 = 0x21;
    const LB_PAIR: u8 = 0x22;
    const BITMAP_EXT: u8 = 0x23;
    const VAULT_TOKEN_X: u8 = 0x24;
    const VAULT_TOKEN_Y: u8 = 0x25;
    const RESERVE_X: u8 = 0x26;
    const RESERVE_Y: u8 = 0x27;
    const TOKEN_X_MINT: u8 = 0x28;
    const TOKEN_Y_MINT: u8 = 0x29;
    const VAULT: u8 = 0x2A;
    const TOKEN_PROGRAM_X: u8 = 0x2B;
    const TOKEN_PROGRAM_Y: u8 = 0x2C;
    const MEMO_PROGRAM: u8 = 0x2D;
    const EVENT_AUTH: u8 = 0x2E;
    const DLMM_PROGRAM: u8 = 0x2F;
    const BA_LOWER_OLD: u8 = 0x30;
    const BA_UPPER_OLD: u8 = 0x31;
    const BA_LOWER_NEW: u8 = 0x32;
    const BA_UPPER_NEW: u8 = 0x33;

    fn pk(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn metas() -> Vec<anchor_lang::solana_program::instruction::AccountMeta> {
        build_rebalance_liquidity_account_metas(
            &pk(POSITION),
            &pk(LB_PAIR),
            &pk(BITMAP_EXT),
            &pk(VAULT_TOKEN_X),
            &pk(VAULT_TOKEN_Y),
            &pk(RESERVE_X),
            &pk(RESERVE_Y),
            &pk(TOKEN_X_MINT),
            &pk(TOKEN_Y_MINT),
            &pk(VAULT),
            &pk(TOKEN_PROGRAM_X),
            &pk(TOKEN_PROGRAM_Y),
            &pk(MEMO_PROGRAM),
            &pk(EVENT_AUTH),
            &pk(DLMM_PROGRAM),
            &pk(BA_LOWER_OLD),
            &pk(BA_UPPER_OLD),
            &pk(BA_LOWER_NEW),
            &pk(BA_UPPER_NEW),
        )
    }

    #[test]
    fn account_count_is_19() {
        // 15 fixed slots + 4 BinArray PDAs (old lower/upper + new lower/upper).
        assert_eq!(metas().len(), 19);
    }

    #[test]
    fn slot_order_matches_module_doc() {
        let m = metas();
        assert_eq!(m[0].pubkey, pk(POSITION));
        assert_eq!(m[1].pubkey, pk(LB_PAIR));
        assert_eq!(m[2].pubkey, pk(BITMAP_EXT));
        assert_eq!(m[3].pubkey, pk(VAULT_TOKEN_X));
        assert_eq!(m[4].pubkey, pk(VAULT_TOKEN_Y));
        assert_eq!(m[5].pubkey, pk(RESERVE_X));
        assert_eq!(m[6].pubkey, pk(RESERVE_Y));
        assert_eq!(m[7].pubkey, pk(TOKEN_X_MINT));
        assert_eq!(m[8].pubkey, pk(TOKEN_Y_MINT));
        assert_eq!(m[9].pubkey, pk(VAULT));
        assert_eq!(m[10].pubkey, pk(TOKEN_PROGRAM_X));
        assert_eq!(m[11].pubkey, pk(TOKEN_PROGRAM_Y));
        assert_eq!(m[12].pubkey, pk(MEMO_PROGRAM));
        assert_eq!(m[13].pubkey, pk(EVENT_AUTH));
        assert_eq!(m[14].pubkey, pk(DLMM_PROGRAM));
        assert_eq!(m[15].pubkey, pk(BA_LOWER_OLD));
        assert_eq!(m[16].pubkey, pk(BA_UPPER_OLD));
        assert_eq!(m[17].pubkey, pk(BA_LOWER_NEW));
        assert_eq!(m[18].pubkey, pk(BA_UPPER_NEW));
    }

    #[test]
    fn only_vault_pda_is_signer() {
        let m = metas();
        for (idx, meta) in m.iter().enumerate() {
            let expected = idx == 9; // vault PDA slot
            assert_eq!(
                meta.is_signer, expected,
                "slot {idx} signer flag mismatch (expected {expected})",
            );
        }
    }

    #[test]
    fn writable_flags_match_spec() {
        let m = metas();
        // Writable slots per spec: 0 position, 1 lb_pair, 3 vault_token_x,
        // 4 vault_token_y, 5 reserve_x, 6 reserve_y, 9 vault, 15-18 BinArrays.
        let writable_slots: [usize; 11] = [0, 1, 3, 4, 5, 6, 9, 15, 16, 17, 18];
        for (idx, meta) in m.iter().enumerate() {
            let expected = writable_slots.contains(&idx);
            assert_eq!(
                meta.is_writable, expected,
                "slot {idx} writable flag mismatch (expected {expected})",
            );
        }
    }
}
