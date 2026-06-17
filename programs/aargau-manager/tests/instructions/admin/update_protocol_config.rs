mod update_protocol_config_tests {
    use aargau_manager::errors::AargauError;

    /// Verify the unauthorized-admin error code exists in the error table.
    /// Full handler tests require a running SVM; the instruction is a stub.
    #[test]
    #[ignore = "TRACKED: instruction is a stub — implement once update logic is complete"]
    fn update_protocol_config_stub() {
        assert_eq!(
            AargauError::UnauthorizedAdmin as u32,
            1101,
            "UnauthorizedAdmin must map to error 1101",
        );
    }
}
