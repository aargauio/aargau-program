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
    use aargau_manager::constants::METEORA_DLMM_PROGRAM_ID;

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

        let _ = METEORA_DLMM_PROGRAM_ID;
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
}
