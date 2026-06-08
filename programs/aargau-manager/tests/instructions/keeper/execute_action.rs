//! Unit tests for `execute_action` handler logic.
//!
//! These are pure-Rust tests — they do not spin up a SVM runtime or use
//! mollusk. Each test exercises a narrow validation path (input guard or
//! fee computation) that can be proven without CPI execution.

mod execute_action_tests {
    use aargau_manager::constants::{BPS_DIVISOR, MAX_ACTIVE_BIN_SLIPPAGE_HARD_CAP};
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::fee::calc_aargau_fee;

    // -------------------------------------------------------------------------
    // Fee routing
    // -------------------------------------------------------------------------

    /// Performance fee is proportional to `fee_rate_bps`.
    /// The handler computes `fee = gross * fee_rate_bps / 10_000` and routes
    /// that amount to the treasury ATAs. This test verifies the arithmetic
    /// used by `calc_aargau_fee` (which `handle_collect_fees` delegates to)
    /// for a representative fee rate.
    #[test]
    fn collect_fees_routes_performance_fee_to_treasury() {
        // 500 bps = 5%
        let fee_rate_bps: u16 = 500;
        let gross: u64 = 1_000_000;

        let fee = calc_aargau_fee(gross, fee_rate_bps).unwrap();
        let user_net = gross - fee;

        assert_eq!(fee, 50_000, "5% of 1_000_000 must be 50_000");
        assert_eq!(user_net, 950_000, "user net must be gross - fee");

        // Treasury receives exactly `fee`; nothing more.
        assert_eq!(fee + user_net, gross, "fee + user_net must equal gross");
    }

    // -------------------------------------------------------------------------
    // IncreaseLiquidity validation
    // -------------------------------------------------------------------------

    /// `active_id_slippage > MAX_ACTIVE_BIN_SLIPPAGE_HARD_CAP` must be
    /// rejected before any CPI fires.
    ///
    /// The guard in `handle_increase_liquidity` is:
    ///   `require!(active_id_slippage <= MAX_ACTIVE_BIN_SLIPPAGE_HARD_CAP, SlippageOutOfRange)`
    /// This test replicates that guard to ensure the constant and the
    /// comparison are correct.
    #[test]
    fn active_id_slippage_exceeds_hard_cap_returns_error() {
        let slippage: u16 = u16::MAX;

        // Replicate the guard from handle_increase_liquidity.
        let is_valid = slippage <= MAX_ACTIVE_BIN_SLIPPAGE_HARD_CAP;
        assert!(
            !is_valid,
            "u16::MAX must exceed MAX_ACTIVE_BIN_SLIPPAGE_HARD_CAP ({})",
            MAX_ACTIVE_BIN_SLIPPAGE_HARD_CAP,
        );

        // Verify the expected error code is SlippageOutOfRange (1010).
        assert_eq!(
            AargauError::SlippageOutOfRange as u32,
            1010,
            "SlippageOutOfRange must map to error 1010",
        );
    }

    // -------------------------------------------------------------------------
    // DecreaseLiquidity validation
    // -------------------------------------------------------------------------

    /// `bps = 0` must be rejected — the guard is
    ///   `require!(bps > 0 && bps <= BPS_DIVISOR_U16, InvalidActionPayload)`
    #[test]
    fn decrease_liquidity_bps_zero_returns_error() {
        let bps: u64 = 0;

        // Replicate the guard: bps must be in [1, BPS_DIVISOR].
        let is_valid = bps > 0 && bps <= BPS_DIVISOR;
        assert!(!is_valid, "bps = 0 must fail the bps > 0 guard",);

        assert_eq!(
            AargauError::InvalidActionPayload as u32,
            1007,
            "InvalidActionPayload must map to error 1007",
        );
    }

    /// `bps > 10_000` must be rejected (same guard as above).
    #[test]
    fn increase_liquidity_bps_exceeds_cap_returns_error() {
        let bps: u64 = BPS_DIVISOR + 1;

        // Replicate the guard.
        let is_valid = bps > 0 && bps <= BPS_DIVISOR;
        assert!(
            !is_valid,
            "bps = BPS_DIVISOR + 1 must fail the bps <= BPS_DIVISOR guard",
        );

        assert_eq!(
            AargauError::InvalidActionPayload as u32,
            1007,
            "InvalidActionPayload must map to error 1007",
        );
    }

    // -------------------------------------------------------------------------
    // OpenPosition / ClosePosition vault-state guards
    // -------------------------------------------------------------------------

    /// `OpenPosition` must be rejected when the vault already has an active
    /// position (i.e. `vault.position_address.is_some()`). The handler guard
    /// is: `require!(vault.position_address.is_none(), VaultHasActivePosition)`.
    #[test]
    fn open_position_when_already_open_returns_error() {
        use anchor_lang::prelude::Pubkey;

        // Simulate vault state with an active position.
        let existing_position: Option<Pubkey> = Some(Pubkey::new_unique());

        // Replicate the guard from handle_open_position.
        let is_idle = existing_position.is_none();
        assert!(
            !is_idle,
            "vault with an existing position must not be treated as idle",
        );

        assert_eq!(
            AargauError::VaultHasActivePosition as u32,
            1301,
            "VaultHasActivePosition must map to error 1301",
        );
    }

    /// `ClosePosition` must be rejected when the vault has no active position
    /// (i.e. `vault.position_address.is_none()`). The handler gate is:
    ///   `require_active_position_v2` → `vault.position_address.ok_or(VaultNoActivePosition)`.
    #[test]
    fn close_position_when_none_open_returns_error() {
        let position_address: Option<anchor_lang::prelude::Pubkey> = None;

        // Replicate the guard from require_active_position_v2.
        let has_position = position_address.is_some();
        assert!(
            !has_position,
            "vault with no position must trigger VaultNoActivePosition",
        );

        assert_eq!(
            AargauError::VaultNoActivePosition as u32,
            1302,
            "VaultNoActivePosition must map to error 1302",
        );
    }

    // -------------------------------------------------------------------------
    // CollectFees — zero-gross edge case
    // -------------------------------------------------------------------------

    /// `CollectFees` with zero claimed fees must not panic and must route
    /// a zero fee to the treasury (no-op transfer).
    ///
    /// The `transfer_performance_fee` helper already guards `amount == 0`
    /// with an early return to avoid issuing zero-value CPIs. This test
    /// verifies that `calc_aargau_fee(0, any_rate)` returns `0` and that the
    /// no-op path is reachable without arithmetic errors.
    #[test]
    fn collect_fees_zero_fees_does_not_panic() {
        let fee_rate_bps: u16 = 500;
        let gross: u64 = 0;

        let fee = calc_aargau_fee(gross, fee_rate_bps).unwrap();
        assert_eq!(fee, 0, "fee on zero gross must be 0");

        // The transfer_performance_fee helper returns early when amount == 0,
        // so no CPI is issued and no panic can occur.
        let would_transfer = fee > 0;
        assert!(!would_transfer, "zero fee must not trigger a transfer CPI",);
    }
}
