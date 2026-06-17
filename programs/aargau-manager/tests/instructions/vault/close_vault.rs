mod close_vault_tests {
    use aargau_manager::errors::AargauError;

    /// `close_vault` requires no active LP position before the vault can be
    /// closed. The guard uses `VaultHasActivePosition`.
    #[test]
    #[ignore = "TRACKED: instruction is a stub — implement once close_vault CPI is wired"]
    fn close_vault_stub() {
        assert_eq!(
            AargauError::VaultHasActivePosition as u32,
            1301,
            "VaultHasActivePosition must map to error 1301",
        );
    }
}
