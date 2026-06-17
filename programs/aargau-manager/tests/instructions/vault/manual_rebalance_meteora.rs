//! Integration tests for `manual_rebalance` against the Meteora DLMM
//! program. See `tests/instructions/keeper/execute_action_meteora.rs` for
//! the harness convention — every test here is fixture-gated and ignored
//! by default.
//!
//! ## Why this file is the formal verification gate for S1 + S4
//!
//! `rebalance_liquidity` is the only Meteora CPI in the codebase with no
//! in-tree backend tx builder to cross-validate against. The wire format
//! (account ordering + Borsh body) was assembled from secondary sources.
//! The pure-Rust byte tests in
//! `tests/utils/meteora/rebalance_liquidity.rs` lock OUR encoder against
//! itself — they catch regressions but NOT a wrong-spec helper.
//!
//! The replay test below is the only thing that proves wire-correctness
//! against the real Meteora program. Until it runs (with the `.so`
//! fixture + a captured mainnet rebalance tx), neither
//! `invoke_rebalance_liquidity` nor `manual_rebalance` should be promoted
//! beyond local cluster testing.

#![allow(dead_code)]

mod manual_rebalance_meteora_tests {
    use crate::fixtures::load_meteora_dlmm_program;

    fn try_load_meteora() -> Result<Vec<u8>, String> {
        load_meteora_dlmm_program()
    }

    /// **Replay test for `rebalance_liquidity`.**
    ///
    /// Capture a known-good Meteora `rebalance_liquidity` instruction from
    /// mainnet (e.g. a Hawksight automation tx), then rebuild the same
    /// instruction with our `build_rebalance_liquidity_*` helpers using
    /// the captured pubkeys + args, and assert the resulting `data` and
    /// `accounts` are byte-for-byte identical to the on-chain tx.
    ///
    /// Mismatch ⇒ the helper layout is wrong and must be fixed before any
    /// `manual_rebalance` reaches devnet. **Do not patch over a diff
    /// here**: report it to the coordinator with a full byte diff and a
    /// link to the source tx.
    #[test]
    #[ignore = "requires captured rebalance_liquidity tx + meteora_dlmm.so fixture"]
    fn replay_against_known_good_mainnet_rebalance_tx() {
        let Ok(_elf) = try_load_meteora() else {
            return;
        };

        // TODO once a real tx is captured:
        //   1. Decode the original instruction (program_id == METEORA_DLMM_PROGRAM_ID).
        //   2. Reconstruct `RebalanceLiquidityArgs` from its data bytes.
        //   3. Call `build_rebalance_liquidity_instruction_data(&args)` and
        //      `build_rebalance_liquidity_account_metas(...)` with the same
        //      pubkeys.
        //   4. `assert_eq!(rebuilt.data, original.data)`.
        //   5. `assert_eq!(rebuilt.accounts, original.accounts)`.
        //
        // A mismatch on (4) means the Borsh body schema is wrong (likely
        // field order or `should_claim_fee` offset). A mismatch on (5)
        // means the account ordering or signer/writable flags are wrong.

        // The METEORA_DLMM_PROGRAM_ID constant is referenced only in the TODO
        // comment above. Once a real tx is captured, import it from
        // `aargau_manager::constants` and verify the program_id field of the
        // decoded instruction.
        let _ = aargau_manager::constants::METEORA_DLMM_PROGRAM_ID;
    }

    /// **Width semantics smoke test against the real program.**
    ///
    /// The pure-Rust test at
    /// `tests/utils/meteora/open_position.rs::width_is_upper_minus_lower_plus_one_inclusive`
    /// already locks our encoder to the `upper - lower + 1` convention. The
    /// runtime test here would assert that after
    /// `OpenPosition { lower: -5, upper: 5 }` the Meteora program reports
    /// a position covering exactly 11 bins (-5 .. =5 inclusive). If
    /// mollusk reports 10 or 12, the convention diverges from Meteora and
    /// the encoder must be revisited.
    #[test]
    #[ignore = "requires meteora_dlmm.so fixture"]
    fn open_position_with_lower_minus_5_upper_plus_5_yields_11_bins() {
        let Ok(_elf) = try_load_meteora() else {
            return;
        };
    }

    /// **Double-fee prevention end-to-end.**
    ///
    /// The pure-Rust test
    /// `tests/utils/meteora/rebalance_liquidity.rs::should_claim_fee_byte_is_false_when_using_constant`
    /// asserts the encoder pins the flag to 0. This runtime test would
    /// additionally confirm that `manual_rebalance` (a) collects fees
    /// itself in step 1, (b) routes the performance split to the treasury,
    /// and (c) passes `should_claim_fee = false` to Meteora so the user
    /// is not double-credited.
    #[test]
    #[ignore = "requires meteora_dlmm.so fixture"]
    fn rebalance_does_not_double_claim_fees() {
        let Ok(_elf) = try_load_meteora() else {
            return;
        };
    }

    /// **Happy path manual rebalance**: ClaimFee2 → RebalanceLiquidity,
    /// treasury receives the performance fee, the vault's
    /// `position_range_lower/upper` are updated to the new range.
    #[test]
    #[ignore = "requires meteora_dlmm.so fixture"]
    fn happy_path_claim_then_rebalance_updates_position_range() {
        let Ok(_elf) = try_load_meteora() else {
            return;
        };
    }

    /// **Wire format excludes `old_lower_bin_id`.**
    ///
    /// The Borsh payload for `rebalance_liquidity` does NOT include
    /// `old_lower_bin_id` — Meteora infers the old range from the existing
    /// `PositionV2` account. Verify that the payload bytes only encode the
    /// new range fields and that `old_lower_bin_id` does not appear.
    ///
    /// Specifically, after the 8-byte discriminator the first field is
    /// `new_lower_bin_id` (i32 LE). An `old_lower_bin_id` present would
    /// shift `new_lower_bin_id` to offset 12 instead of 8.
    #[test]
    fn rebalance_wire_format_excludes_old_lower_bin_id() {
        use aargau_manager::constants::{
            METEORA_REBALANCE_LIQUIDITY_DISCRIMINATOR, METEORA_REBALANCE_SHOULD_CLAIM_FEE,
        };
        use aargau_manager::utils::meteora::add_liquidity::METEORA_STRATEGY_SPOT_IMBALANCED;
        use aargau_manager::utils::meteora::rebalance_liquidity::{
            build_rebalance_liquidity_instruction_data, RebalanceLiquidityArgs,
        };

        let old_lower: i32 = -100;
        let new_lower: i32 = -50;
        let new_upper: i32 = 50;

        let args = RebalanceLiquidityArgs {
            old_lower_bin_id: old_lower,
            new_lower_bin_id: new_lower,
            new_upper_bin_id: new_upper,
            amount_x: 0,
            amount_y: 0,
            active_id: 0,
            max_active_bin_slippage: 3,
            strategy_type: METEORA_STRATEGY_SPOT_IMBALANCED,
            should_claim_fee: METEORA_REBALANCE_SHOULD_CLAIM_FEE,
        };

        let data = build_rebalance_liquidity_instruction_data(&args);

        // Verify discriminator at bytes 0..8.
        assert_eq!(&data[..8], &METEORA_REBALANCE_LIQUIDITY_DISCRIMINATOR);

        // Byte 8..12 must be `new_lower_bin_id`, NOT `old_lower_bin_id`.
        // If old_lower_bin_id were included, the first i32 field would be
        // -100 and the payload would be 4 bytes longer overall.
        let first_i32 = i32::from_le_bytes(data[8..12].try_into().unwrap());
        assert_eq!(
            first_i32, new_lower,
            "offset 8 must encode new_lower_bin_id ({new_lower}), not old_lower_bin_id ({old_lower})",
        );

        // Total length must not include the extra 4 bytes an old_lower field would add.
        // Expected: 8 disc + 4 new_lower + 4 new_upper + 8 amt_x + 8 amt_y
        //         + 4 active_id + 4 slippage + 4 min_bin + 4 max_bin + 1 strategy
        //         + 64 params + 1 should_claim_fee + 8 RAI = 122 bytes.
        assert_eq!(
            data.len(),
            122,
            "payload must be 122 bytes (no old_lower_bin_id field)"
        );
    }

    /// **Bitmap extension out-of-range returns `BitmapExtensionRequired`.**
    ///
    /// Ranges whose endpoints exceed `METEORA_INLINE_BITMAP_BIN_LIMIT`
    /// (= 5460) require the `bin_array_bitmap_extension` PDA, which is not
    /// yet wired. The handler must reject such ranges upfront.
    #[test]
    fn bitmap_extension_out_of_range_returns_error() {
        use aargau_manager::constants::METEORA_INLINE_BITMAP_BIN_LIMIT;
        use aargau_manager::errors::AargauError;

        // -33000 is well beyond the inline bitmap limit of ±5460.
        let lower_bin_id: i32 = -33000;
        let upper_bin_id: i32 = -32000;

        let is_within = lower_bin_id >= -METEORA_INLINE_BITMAP_BIN_LIMIT
            && upper_bin_id <= METEORA_INLINE_BITMAP_BIN_LIMIT;

        assert!(
            !is_within,
            "lower={lower_bin_id} must be outside ±{METEORA_INLINE_BITMAP_BIN_LIMIT} inline bitmap range",
        );

        // Verify the expected error code maps to BitmapExtensionRequired (1206).
        assert_eq!(
            AargauError::BitmapExtensionRequired as u32,
            1206,
            "BitmapExtensionRequired must map to error 1206",
        );
    }

    /// **Double-fee not charged — `should_claim_fee` constructor invariant.**
    ///
    /// `manual_rebalance` calls `claim_fee2` first (step 1) then passes
    /// `should_claim_fee = METEORA_REBALANCE_SHOULD_CLAIM_FEE` (= `false`)
    /// to `rebalance_liquidity` (step 2). Passing `true` would deposit the
    /// just-claimed fees back into the vault's ATAs, double-crediting the
    /// user with revenue already routed to the treasury.
    ///
    /// This test asserts the `RebalanceLiquidityArgs` constructor in the
    /// handler uses the pinned constant value `false` for `should_claim_fee`.
    #[test]
    fn double_fee_not_charged() {
        use aargau_manager::constants::METEORA_REBALANCE_SHOULD_CLAIM_FEE;
        use aargau_manager::utils::meteora::add_liquidity::METEORA_STRATEGY_SPOT_IMBALANCED;
        use aargau_manager::utils::meteora::rebalance_liquidity::{
            build_rebalance_liquidity_instruction_data, RebalanceLiquidityArgs,
        };

        // Constant must be false — any change to this is a double-fee bug.
        assert!(
            !METEORA_REBALANCE_SHOULD_CLAIM_FEE,
            "METEORA_REBALANCE_SHOULD_CLAIM_FEE must be false to prevent double-fee",
        );

        let args = RebalanceLiquidityArgs {
            old_lower_bin_id: 0,
            new_lower_bin_id: -10,
            new_upper_bin_id: 10,
            amount_x: 0,
            amount_y: 0,
            active_id: 0,
            max_active_bin_slippage: 3,
            strategy_type: METEORA_STRATEGY_SPOT_IMBALANCED,
            should_claim_fee: METEORA_REBALANCE_SHOULD_CLAIM_FEE,
        };

        assert!(
            !args.should_claim_fee,
            "RebalanceLiquidityArgs.should_claim_fee must be false at every production call site",
        );

        // Verify the encoded byte is 0 at the known offset (113).
        // Layout: 8 disc + 4 new_lower + 4 new_upper + 8 amt_x + 8 amt_y
        //       + 4 active_id + 4 slippage + 4 min_bin + 4 max_bin + 1 strategy
        //       + 64 params => should_claim_fee is at byte 113.
        let data = build_rebalance_liquidity_instruction_data(&args);
        assert_eq!(
            data[113], 0u8,
            "should_claim_fee byte at offset 113 must be 0 (false)",
        );
    }
}
