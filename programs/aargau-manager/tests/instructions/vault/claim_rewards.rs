mod claim_rewards_tests {
    use aargau_manager::errors::AargauError;

    /// `claim_rewards` is not supported for the Meteora DLMM protocol (fees
    /// are claimed via `CollectFees`). The error code must exist.
    #[test]
    fn rewards_not_supported_error_code_exists() {
        assert_eq!(
            AargauError::RewardsNotSupportedForProtocol as u32,
            1401,
            "RewardsNotSupportedForProtocol must map to error 1401",
        );
    }
}
