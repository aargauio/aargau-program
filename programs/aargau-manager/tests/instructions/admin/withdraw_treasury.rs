mod withdraw_treasury_tests {
    use aargau_manager::errors::AargauError;

    /// Verify the treasury error codes exist in the error table.
    /// Full handler tests require a running SVM; the instruction is a stub.
    #[test]
    #[ignore = "TRACKED: instruction is a stub — implement once treasury CPI is wired"]
    fn withdraw_treasury_stub() {
        assert_eq!(
            AargauError::InvalidTreasury as u32,
            1500,
            "InvalidTreasury must map to error 1500",
        );
    }
}
