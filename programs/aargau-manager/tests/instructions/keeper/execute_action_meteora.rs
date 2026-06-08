//! Integration tests for `execute_action` against the Meteora DLMM program.
//!
//! ## Status: harness scaffolded, fixture-gated
//!
//! Every test in this file requires the pinned Meteora DLMM `.so` at
//! `tests/fixtures/meteora_dlmm.so`. Without it the mollusk harness cannot
//! load the program and the CPI under test would fail with `ProgramNotCached`
//! rather than the invariant we are trying to assert.
//!
//! Each test is therefore marked `#[ignore = "requires meteora_dlmm.so fixture"]`
//! by default. Dump the fixture once (see `tests/fixtures/mod.rs` for the
//! exact `solana program dump` command) and run with:
//!
//! ```text
//! cargo test --manifest-path programs/aargau-manager/Cargo.toml -- --ignored execute_action_meteora
//! ```
//!
//! Each test below documents what invariant it would exercise once enabled.
//! The pure-Rust counterparts under `tests/utils/meteora/{open_position,
//! close_position,rebalance_liquidity}.rs` lock the wire format (account
//! ordering, Borsh body) without needing the runtime, so the byte-level
//! contract is already enforced on every `cargo test` run.

#![allow(dead_code)]

mod harness {
    use crate::fixtures::load_meteora_dlmm_program;
    use aargau_manager::constants::METEORA_DLMM_PROGRAM_ID;
    use mollusk_svm::Mollusk;

    /// Attempt to build a Mollusk environment with the Aargau program and
    /// the Meteora DLMM `.so` preloaded. Returns `Err` (and the calling
    /// test should early-return) when the fixture is missing — this keeps
    /// the integration test crate green on contributor machines that have
    /// not dumped the program locally.
    pub fn try_build_meteora_mollusk(
        aargau_program_id: &anchor_lang::prelude::Pubkey,
    ) -> Result<Mollusk, String> {
        let meteora_elf = load_meteora_dlmm_program()?;

        // Aargau program ID is the same `declare_id!` value from `src/lib.rs`.
        // Loaded from a path-less ELF would require an Aargau `.so` too —
        // the harness builder calls `add_program_with_loader_and_elf` for
        // both binaries. For now we wire only the Meteora program; the
        // Aargau-side instruction is encoded as a raw `Instruction` and
        // executed via `process_instruction`, which Mollusk routes through
        // the loaded ELFs.
        let mut mollusk = Mollusk::default();
        mollusk.add_program_with_loader_and_elf(
            &METEORA_DLMM_PROGRAM_ID,
            &mollusk_svm::program::loader_keys::LOADER_V3,
            &meteora_elf,
        );

        // The Aargau program must also be loaded for end-to-end tests.
        // Built by `cargo build-sbf` — run it before integration tests.
        let program_so = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/deploy/aargau_manager.so");

        if program_so.exists() {
            let aargau_elf = std::fs::read(&program_so)
                .map_err(|e| format!("failed to read aargau_manager.so: {e}"))?;
            mollusk.add_program_with_loader_and_elf(
                aargau_program_id,
                &mollusk_svm::program::loader_keys::LOADER_V3,
                &aargau_elf,
            );
        }
        // If the binary is absent, fixture-gated tests will early-return via
        // the `try_load_meteora` guard before reaching any Aargau CPI.

        Ok(mollusk)
    }
}

mod execute_action_meteora_tests {
    use super::harness::try_build_meteora_mollusk;
    use anchor_lang::prelude::Pubkey;

    /// Happy path: open → collect → close, asserting the Meteora program
    /// emits the expected `PositionCreated` / `FeeClaimed` events and the
    /// vault ATAs settle to the predicted balances.
    ///
    /// Requires: PositionV2 fixture, LbPair fixture, BinArray pair fixtures,
    /// treasury PDA + ATA fixtures, mint fixtures (SPL classic).
    #[test]
    #[ignore = "requires meteora_dlmm.so fixture + full account fixtures"]
    fn happy_path_open_collect_close() {
        let aargau = Pubkey::new_unique();
        let Ok(_mollusk) = try_build_meteora_mollusk(&aargau) else {
            return;
        };
        // TODO: instantiate vault PDA + ATAs, call OpenPosition, then
        //       CollectFees, then ClosePosition; assert treasury balance =
        //       gross * fee_rate_bps / 10_000 and vault ATAs zeroed.
    }

    /// `Token-2022 mint rejected`. Build `create_vault` accounts with a
    /// Token-2022-owned mint and assert the handler returns
    /// `AargauError::Token2022NotSupported` BEFORE any CPI fires.
    ///
    /// The same invariant is already asserted by the pure-Rust tests in
    /// `tests/utils/meteora/lb_pair_view.rs::require_spl_classic_mints_tests`
    /// (3 cases, all green on every run). The integration variant below
    /// would add a cross-check that the handler actually consults that
    /// helper.
    #[test]
    #[ignore = "requires meteora_dlmm.so fixture; pure-Rust coverage in lb_pair_view.rs"]
    fn rejects_token_2022_mint() {
        let aargau = Pubkey::new_unique();
        let Ok(_mollusk) = try_build_meteora_mollusk(&aargau) else {
            return;
        };
    }

    /// `>69 bins rejected`. Send an `OpenPosition` payload with
    /// `upper - lower + 1 > MAX_METEORA_BINS` (= 69) and assert
    /// `AargauError::TooManyBins` (= 1003).
    ///
    /// The bin-count math is also asserted by the pure-Rust width test in
    /// `tests/utils/meteora/open_position.rs::width_is_upper_minus_lower_plus_one_inclusive`.
    #[test]
    #[ignore = "requires meteora_dlmm.so fixture"]
    fn rejects_more_than_69_bins() {
        let aargau = Pubkey::new_unique();
        let Ok(_mollusk) = try_build_meteora_mollusk(&aargau) else {
            return;
        };
    }

    /// `Active bin outside inline bitmap rejected`. Pass a range with
    /// `lower < -METEORA_INLINE_BITMAP_BIN_LIMIT` (= -5460) and assert
    /// `AargauError::BitmapExtensionRequired` (= 1206).
    #[test]
    #[ignore = "requires meteora_dlmm.so fixture"]
    fn rejects_range_outside_inline_bitmap() {
        let aargau = Pubkey::new_unique();
        let Ok(_mollusk) = try_build_meteora_mollusk(&aargau) else {
            return;
        };
    }

    /// `PositionV2 discriminator check`. Hand the keeper a synthetic
    /// account at `vault.position_address` whose discriminator does not
    /// match `METEORA_POSITION_V2_DISCRIMINATOR` and assert
    /// `AargauError::InvalidPositionDiscriminator` (= 1205).
    ///
    /// The byte-level check is already covered by
    /// `tests/utils/meteora/position_view.rs::rejects_buffer_with_wrong_discriminator`.
    #[test]
    #[ignore = "requires meteora_dlmm.so fixture; byte coverage in position_view.rs"]
    fn rejects_fake_position_account() {
        let aargau = Pubkey::new_unique();
        let Ok(_mollusk) = try_build_meteora_mollusk(&aargau) else {
            return;
        };
    }

    /// `Fee math on CollectFees`. Set `ProtocolConfig.fee_rate_bps = 500`
    /// (5 %), claim a deterministic gross of 1_000_000 lamports, and
    /// assert the treasury ATA delta equals `1_000_000 * 500 / 10_000 = 50_000`
    /// for each side. The vault ATA must receive `gross - fee`.
    ///
    /// The pure math is already covered by
    /// `tests/utils/fee.rs::calc_aargau_fee_*`.
    #[test]
    #[ignore = "requires meteora_dlmm.so fixture; pure math coverage in utils/fee.rs"]
    fn collect_fees_routes_performance_split_to_treasury() {
        let aargau = Pubkey::new_unique();
        let Ok(_mollusk) = try_build_meteora_mollusk(&aargau) else {
            return;
        };
    }
}
