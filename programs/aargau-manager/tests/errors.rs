//! Tests for `src/errors.rs` — AargauError discriminant values and range layout.

#[cfg(test)]
mod error_code_tests {
    use aargau_manager::errors::AargauError;

    // --- Input validation range: 1000–1099 ---

    #[test]
    fn test_deposit_amount_zero_discriminant() {
        // First error — base offset must be exactly 1000.
        assert_eq!(AargauError::DepositAmountZero as u32, 1000);
    }

    #[test]
    fn test_invalid_withdraw_pct_bps_discriminant() {
        assert_eq!(AargauError::InvalidWithdrawPctBps as u32, 1001);
    }

    #[test]
    fn test_slippage_exceeded_discriminant() {
        assert_eq!(AargauError::SlippageExceeded as u32, 1002);
    }

    #[test]
    fn test_too_many_bins_discriminant() {
        assert_eq!(AargauError::TooManyBins as u32, 1003);
    }

    #[test]
    fn test_fee_rate_too_high_discriminant() {
        assert_eq!(AargauError::FeeRateTooHigh as u32, 1004);
    }

    #[test]
    fn test_invalid_public_key_discriminant() {
        assert_eq!(AargauError::InvalidPublicKey as u32, 1005);
    }

    // --- Authority range: 1100–1199 ---

    #[test]
    fn test_unauthorized_user_discriminant() {
        assert_eq!(AargauError::UnauthorizedUser as u32, 1100);
    }

    #[test]
    fn test_unauthorized_admin_discriminant() {
        assert_eq!(AargauError::UnauthorizedAdmin as u32, 1101);
    }

    #[test]
    fn test_duplicate_keeper_authority_discriminant() {
        assert_eq!(AargauError::DuplicateKeeperAuthority as u32, 1102);
    }

    #[test]
    fn test_unauthorized_keeper_discriminant() {
        assert_eq!(AargauError::UnauthorizedKeeper as u32, 1103);
    }

    // --- Protocol / Pool validation range: 1200–1299 ---

    #[test]
    fn test_invalid_pool_discriminant() {
        assert_eq!(AargauError::InvalidPool as u32, 1200);
    }

    #[test]
    fn test_invalid_pool_discriminator_discriminant() {
        assert_eq!(AargauError::InvalidPoolDiscriminator as u32, 1201);
    }

    #[test]
    fn test_invalid_pool_mint_discriminant() {
        assert_eq!(AargauError::InvalidPoolMint as u32, 1202);
    }

    #[test]
    fn test_non_transferable_mint_discriminant() {
        assert_eq!(AargauError::NonTransferableMint as u32, 1203);
    }

    #[test]
    fn test_pool_operation_disabled_discriminant() {
        assert_eq!(AargauError::PoolOperationDisabled as u32, 1204);
    }

    // --- Vault state range: 1300–1399 ---

    #[test]
    fn test_invalid_vault_pda_discriminant() {
        assert_eq!(AargauError::InvalidVaultPda as u32, 1300);
    }

    #[test]
    fn test_vault_has_active_position_discriminant() {
        assert_eq!(AargauError::VaultHasActivePosition as u32, 1301);
    }

    #[test]
    fn test_vault_no_active_position_discriminant() {
        assert_eq!(AargauError::VaultNoActivePosition as u32, 1302);
    }

    #[test]
    fn test_pending_rebalance_exists_discriminant() {
        assert_eq!(AargauError::PendingRebalanceExists as u32, 1303);
    }

    #[test]
    fn test_no_pending_rebalance_discriminant() {
        assert_eq!(AargauError::NoPendingRebalance as u32, 1304);
    }

    #[test]
    fn test_pending_rebalance_not_expired_discriminant() {
        assert_eq!(AargauError::PendingRebalanceNotExpired as u32, 1305);
    }

    #[test]
    fn test_post_cpi_balance_decreased_discriminant() {
        assert_eq!(AargauError::PostCpiBalanceDecreased as u32, 1306);
    }

    #[test]
    fn test_fee_exceeds_gross_discriminant() {
        assert_eq!(AargauError::FeeExceedsGross as u32, 1307);
    }

    // --- Protocol state range: 1400–1499 ---

    #[test]
    fn test_program_paused_discriminant() {
        assert_eq!(AargauError::ProgramPaused as u32, 1400);
    }

    #[test]
    fn test_rewards_not_supported_for_protocol_discriminant() {
        assert_eq!(AargauError::RewardsNotSupportedForProtocol as u32, 1401);
    }

    // --- Treasury range: 1500–1599 ---

    #[test]
    fn test_invalid_treasury_discriminant() {
        assert_eq!(AargauError::InvalidTreasury as u32, 1500);
    }

    #[test]
    fn test_treasury_invariant_violated_discriminant() {
        assert_eq!(AargauError::TreasuryInvariantViolated as u32, 1501);
    }

    // --- Arithmetic range: 1600–1699 ---

    #[test]
    fn test_overflow_discriminant() {
        assert_eq!(AargauError::Overflow as u32, 1600);
    }

    #[test]
    fn test_underflow_discriminant() {
        assert_eq!(AargauError::Underflow as u32, 1601);
    }

    #[test]
    fn test_division_by_zero_discriminant() {
        assert_eq!(AargauError::DivisionByZero as u32, 1602);
    }

    // --- Range boundary invariants ---

    #[test]
    fn test_no_error_below_1000() {
        // The lowest discriminant must be >= 1000 to avoid collisions with Anchor built-ins.
        assert!(AargauError::DepositAmountZero as u32 >= 1000);
    }

    #[test]
    fn test_arithmetic_errors_at_1600() {
        // Arithmetic errors must start at 1600 — guarding the chosen sub-range.
        assert_eq!(AargauError::Overflow as u32, 1600);
    }
}
