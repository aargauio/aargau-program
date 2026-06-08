mod retry_pending_rebalance_tests {
    use aargau_manager::errors::AargauError;

    /// `retry_pending_rebalance` opens a new position using stored
    /// `PendingRebalance` params. The `NoPendingRebalance` error must exist.
    #[test]
    #[ignore = "TRACKED: instruction is a stub — implement once PendingRebalance flow is complete"]
    fn retry_pending_rebalance_stub() {
        assert_eq!(
            AargauError::NoPendingRebalance as u32,
            1304,
            "NoPendingRebalance must map to error 1304",
        );
        assert_eq!(
            AargauError::PendingRebalanceNotExpired as u32,
            1305,
            "PendingRebalanceNotExpired must map to error 1305",
        );
    }
}
