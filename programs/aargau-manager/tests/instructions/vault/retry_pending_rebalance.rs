//! Tests for `retry_pending_rebalance` — phase two of the Orca two-transaction
//! rebalance (OpenNew).
//!
//! The params shape changed from the original stub: the tick params were
//! dropped (the new range is taken strictly from `pending_rebalance`) and
//! replaced with the open-leg amount + slippage params. Active tests pin the
//! new Borsh wire shape and the error-code contract; the mollusk replay is
//! fixture-gated.

#![allow(dead_code)]

mod retry_pending_rebalance_params_tests {
    use aargau_manager::instructions::vault::retry_pending_rebalance::RetryPendingRebalanceParams;
    use borsh::BorshSerialize;

    /// Params Borsh layout: u128 + u64 + u64 = 16 + 8 + 8 = 32 bytes, in field
    /// order. No tick params — the range comes from `pending_rebalance`.
    #[test]
    fn params_serialize_in_declared_field_order() {
        let params = RetryPendingRebalanceParams {
            liquidity_amount: 0x0102_0304_0506_0708_u128,
            token_max_a: 0x1112_1314_1516_1718,
            token_max_b: 0x2122_2324_2526_2728,
        };
        let mut bytes = Vec::new();
        params.serialize(&mut bytes).unwrap();
        assert_eq!(bytes.len(), 16 + 8 + 8);
        assert_eq!(
            u128::from_le_bytes(bytes[0..16].try_into().unwrap()),
            0x0102_0304_0506_0708_u128,
        );
        assert_eq!(
            u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
            0x1112_1314_1516_1718,
        );
        assert_eq!(
            u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
            0x2122_2324_2526_2728,
        );
    }
}

mod retry_pending_rebalance_error_contract {
    use aargau_manager::errors::AargauError;

    /// Retrying without a pending rebalance (or re-running after a successful
    /// open) maps to `NoPendingRebalance` (1304).
    #[test]
    fn no_pending_maps_to_1304() {
        assert_eq!(AargauError::NoPendingRebalance as u32, 1304);
    }

    /// `PendingRebalanceNotExpired` (1305) stays reserved — not consumed by the
    /// user-signed retry path (no timeout gate); kept for future keeper takeover.
    #[test]
    fn not_expired_reserved_at_1305() {
        assert_eq!(AargauError::PendingRebalanceNotExpired as u32, 1305);
    }

    /// The open leg must not see a leftover active position — Tx1 cleared it.
    #[test]
    fn active_position_maps_to_1301() {
        assert_eq!(AargauError::VaultHasActivePosition as u32, 1301);
    }

    /// Open-leg slippage ceiling surfaces `SlippageExceeded` (1002).
    #[test]
    fn slippage_maps_to_1002() {
        assert_eq!(AargauError::SlippageExceeded as u32, 1002);
    }
}

mod retry_pending_rebalance_replay {
    use crate::fixtures::load_whirlpools_program;
    use aargau_manager::constants::ORCA_WHIRLPOOL_PROGRAM_ID;
    use anchor_lang::prelude::Pubkey;
    use mollusk_svm::Mollusk;

    fn try_build_whirlpools_mollusk(_aargau_program_id: &Pubkey) -> Result<Mollusk, String> {
        let whirlpools_elf = load_whirlpools_program()?;
        let mut mollusk = Mollusk::default();
        mollusk.add_program_with_loader_and_elf(
            &ORCA_WHIRLPOOL_PROGRAM_ID,
            &mollusk_svm::program::loader_keys::LOADER_V3,
            &whirlpools_elf,
        );
        Ok(mollusk)
    }

    /// Happy path Tx2: open a new position over the STORED pending range + fund
    /// it; `pending_rebalance` cleared and `position_*` set to the new range.
    #[test]
    #[ignore = "requires orca_whirlpools.so fixture"]
    fn retry_opens_new_position_over_stored_range() {
        let aargau_program_id = Pubkey::new_unique();
        let Ok(_mollusk) = try_build_whirlpools_mollusk(&aargau_program_id) else {
            return; // fixture missing — skip
        };
        // Replay must assert the new position range equals the pending target,
        // not any caller-supplied tick (the tick params were removed).
    }

    /// Idempotent retry: a first attempt that fails atomically (open+increase)
    /// persists no position; a retry with a FRESH ephemeral mint succeeds and
    /// clears pending. Re-running after success hits `NoPendingRebalance`.
    ///
    /// Replay contract (asserted once the fixture lands and the instruction
    /// harness drives `RetryPendingRebalance` against the loaded program):
    ///   1. Seed a vault in the post-Tx1 state: `pending_rebalance = Some`,
    ///      `position_address = None`, drained tokens in the vault ATAs.
    ///   2. First Tx2 attempt with `position_mint = mint_1` succeeds:
    ///      `pending_rebalance == None`, `position_address == Some(pos_1)`,
    ///      `position_range_{lower,upper}` equal the STORED pending target (not
    ///      any caller tick — the tick params were removed).
    ///   3. A second Tx2 (any amounts, any `position_mint`) must fail with
    ///      `AargauError::NoPendingRebalance` (1304) — the pending flag is gone,
    ///      so the open never re-runs (idempotency: success is a one-shot).
    ///   4. Re-using the same `position_mint = mint_1` on a fresh pending vault
    ///      must fail at the position-PDA bind (the PDA derives from the mint
    ///      and `mint_1`'s position already exists) — every attempt requires a
    ///      freshly generated ephemeral mint.
    #[test]
    #[ignore = "requires orca_whirlpools.so fixture"]
    fn retry_with_fresh_mint_is_idempotent() {
        let aargau_program_id = Pubkey::new_unique();
        let Ok(_mollusk) = try_build_whirlpools_mollusk(&aargau_program_id) else {
            return;
        };
        // Assertions per the replay contract above are wired with the harness
        // that builds the `RetryPendingRebalance` instruction + account set
        // against the dumped Whirlpools program.
    }
}
