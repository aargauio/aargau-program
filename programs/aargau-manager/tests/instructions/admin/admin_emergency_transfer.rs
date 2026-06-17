mod admin_emergency_transfer_tests {
    use aargau_manager::errors::AargauError;

    /// Verify `admin_emergency_transfer` is registered in the error table.
    /// The instruction is a stub — full CPI tests require a running SVM.
    #[test]
    #[ignore = "TRACKED: instruction is a stub — implement once admin CPI surface is complete"]
    fn admin_emergency_transfer_stub() {
        // Verify the unauthorized-admin error code exists in the table.
        assert_eq!(
            AargauError::UnauthorizedAdmin as u32,
            1101,
            "UnauthorizedAdmin must map to error 1101",
        );
    }
}
