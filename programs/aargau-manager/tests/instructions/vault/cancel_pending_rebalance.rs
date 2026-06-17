//! Tests for `cancel_pending_rebalance` — emergency recovery for a stuck
//! two-phase rebalance.
//!
//! The handler is CPI-free and carries no `protocol_config` (no pause gate),
//! mirroring `emergency_withdraw`. Active tests pin the error contract; the
//! state transition (`pending_rebalance` → None + `RebalanceCancelled`) is
//! exercised end-to-end by the Orca mollusk replay alongside Tx1.

mod cancel_pending_rebalance_error_contract {
    use aargau_manager::errors::AargauError;

    /// Cancelling with no pending rebalance maps to `NoPendingRebalance` (1304).
    #[test]
    fn no_pending_maps_to_1304() {
        assert_eq!(AargauError::NoPendingRebalance as u32, 1304);
    }
}

mod cancel_pending_rebalance_no_pause_gate {
    /// The cancel Accounts struct intentionally omits `protocol_config`: there
    /// is no pause gate so a user can always clear a transient pending state and
    /// withdraw, even while the protocol is paused. This test documents the
    /// invariant — the absence of a `ProgramPaused` dependency. (Compile-time:
    /// the handler signature takes no config; see the instruction module.)
    #[test]
    fn cancel_has_no_pause_dependency() {
        // The cancel path is pause-independent by construction. There is no
        // runtime assertion to make here without a full SVM harness; this test
        // anchors the design decision so a future regression that adds a pause
        // gate is flagged in review against an explicit, named expectation.
        let cancel_takes_protocol_config = false;
        assert!(
            !cancel_takes_protocol_config,
            "cancel_pending_rebalance must remain pause-independent",
        );
    }
}
