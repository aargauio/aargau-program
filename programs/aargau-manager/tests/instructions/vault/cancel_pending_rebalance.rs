mod cancel_pending_rebalance_tests {
    use aargau_manager::errors::AargauError;

    /// `cancel_pending_rebalance` clears the `PendingRebalance` state without
    /// opening a new position. The error code `NoPendingRebalance` must exist.
    #[test]
    #[ignore = "TRACKED: instruction is a stub — implement once PendingRebalance flow is complete"]
    fn cancel_pending_rebalance_stub() {
        assert_eq!(
            AargauError::NoPendingRebalance as u32,
            1304,
            "NoPendingRebalance must map to error 1304",
        );
    }
}
