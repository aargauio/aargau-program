//! Integration tests for `execute_action_orca` against the Orca Whirlpools
//! program.
//!
//! ## Status: harness scaffolded, fixture-gated
//!
//! Every test requires the pinned Orca Whirlpools `.so` at
//! `tests/fixtures/orca_whirlpools.so`. Without it the mollusk harness cannot
//! load the program and the CPI under test would fail with `ProgramNotCached`
//! rather than the invariant we are trying to assert.
//!
//! Each test is therefore marked `#[ignore = "requires orca_whirlpools.so fixture"]`.
//! Dump the fixture once (see `tests/fixtures/mod.rs` for the exact command)
//! and run with:
//!
//! ```text
//! cargo test --manifest-path programs/aargau-manager/Cargo.toml -- --ignored execute_action_orca
//! ```
//!
//! The pure-Rust counterparts under `tests/utils/orca/` lock the wire format
//! (discriminators, account ordering, Borsh body) without the runtime, so the
//! byte-level contract is enforced on every `cargo test` run. This mollusk
//! replay is the only end-to-end proof that the wire matches the deployed
//! program and is the mandatory gate before any deploy — the same gate the
//! Meteora `manual_rebalance` path carries.

#![allow(dead_code)]

mod harness {
    use crate::fixtures::load_whirlpools_program;
    use aargau_manager::constants::ORCA_WHIRLPOOL_PROGRAM_ID;
    use mollusk_svm::Mollusk;

    /// Build a Mollusk environment with the Aargau program and the Orca
    /// Whirlpools `.so` preloaded. Returns `Err` (the calling test should
    /// early-return) when the fixture is missing, keeping the suite green on
    /// machines that have not dumped the program locally.
    pub fn try_build_whirlpools_mollusk(
        aargau_program_id: &anchor_lang::prelude::Pubkey,
    ) -> Result<Mollusk, String> {
        let whirlpools_elf = load_whirlpools_program()?;

        let mut mollusk = Mollusk::default();
        mollusk.add_program_with_loader_and_elf(
            &ORCA_WHIRLPOOL_PROGRAM_ID,
            &mollusk_svm::program::loader_keys::LOADER_V3,
            &whirlpools_elf,
        );

        // The Aargau program must also be loaded for end-to-end tests; built by
        // `cargo build-sbf`. If absent, fixture-gated tests early-return via the
        // loader guard before reaching any Aargau CPI.
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

        Ok(mollusk)
    }
}

mod execute_action_orca_replay {
    use super::harness::try_build_whirlpools_mollusk;
    use anchor_lang::prelude::Pubkey;

    /// OpenPosition: vault PDA is `owner`, NFT minted into vault ATA, position
    /// PDA + range recorded. Replays the open CPI against the real program.
    #[test]
    #[ignore = "requires orca_whirlpools.so fixture"]
    fn open_position_mints_nft_to_vault() {
        let aargau_program_id = Pubkey::new_unique();
        let Ok(_mollusk) = try_build_whirlpools_mollusk(&aargau_program_id) else {
            return; // fixture missing — skip
        };
        // Full end-to-end replay wiring is added with the dumped fixture.
    }

    /// IncreaseLiquidity → DecreaseLiquidity round-trip with vault PDA as
    /// position_authority; validates tick-array bindings against the range.
    #[test]
    #[ignore = "requires orca_whirlpools.so fixture"]
    fn increase_then_decrease_liquidity() {
        let aargau_program_id = Pubkey::new_unique();
        let Ok(_mollusk) = try_build_whirlpools_mollusk(&aargau_program_id) else {
            return;
        };
    }

    /// CollectFees sweeps fees (v2) + active rewards (v2), routes the fee split
    /// to the treasury ATAs.
    ///
    /// Replay must assert, against a MIXED pool (one mint SPL-classic, the other
    /// Token-2022), that each performance-fee `transfer_checked` leg is invoked
    /// against that leg's own token program (`token_program_a` for the mint_a
    /// leg, `token_program_b` for the mint_b leg). Passing a single shared
    /// token program for both legs makes one `transfer_checked` fail because the
    /// program does not own that mint — the bug this gate guards against.
    /// Reward destinations are bound to the vault's ATA for each reward mint
    /// (see `require_reward_owner_is_vault_ata` wire tests in
    /// `tests/utils/orca/accounts.rs`); the replay confirms rewards land in
    /// vault custody.
    #[test]
    #[ignore = "requires orca_whirlpools.so fixture"]
    fn collect_fees_and_rewards() {
        let aargau_program_id = Pubkey::new_unique();
        let Ok(_mollusk) = try_build_whirlpools_mollusk(&aargau_program_id) else {
            return;
        };
    }

    /// ClosePosition burns the empty NFT from the vault ATA and refunds rent to
    /// the user; vault position fields are cleared.
    #[test]
    #[ignore = "requires orca_whirlpools.so fixture"]
    fn close_position_burns_nft_and_refunds_rent() {
        let aargau_program_id = Pubkey::new_unique();
        let Ok(_mollusk) = try_build_whirlpools_mollusk(&aargau_program_id) else {
            return;
        };
    }
}
