mod report_rebalance_attempt_tests {
    use aargau_manager::errors::AargauError;

    /// `report_rebalance_attempt` records a failed or partial rebalance attempt
    /// from the keeper. The `UnauthorizedKeeper` error code must exist.
    #[test]
    #[ignore = "TRACKED: instruction is a stub — implement once keeper dual-authority model is complete"]
    fn report_rebalance_attempt_stub() {
        assert_eq!(
            AargauError::UnauthorizedKeeper as u32,
            1103,
            "UnauthorizedKeeper must map to error 1103",
        );
    }
}
