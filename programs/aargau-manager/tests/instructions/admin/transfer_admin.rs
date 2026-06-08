mod transfer_admin_tests {
    use aargau_manager::errors::AargauError;

    /// Verify the unauthorized-admin error code exists in the error table.
    /// Full two-step transfer tests require a running SVM; the instruction is a stub.
    #[test]
    #[ignore = "TRACKED: instruction is a stub — needs two-step + realloc implementation"]
    fn transfer_admin_stub() {
        assert_eq!(
            AargauError::UnauthorizedAdmin as u32,
            1101,
            "UnauthorizedAdmin must map to error 1101",
        );
    }
}
