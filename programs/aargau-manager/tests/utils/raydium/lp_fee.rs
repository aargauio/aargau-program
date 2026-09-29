//! Tests for `src/utils/raydium/lp_fee.rs` — the performance-fee base on
//! Raydium fee collection.
//!
//! The base must be the LP fee the vault received (pool-vault outflow net of
//! the live Token-2022 transfer fee), never the vault ATA delta, which also
//! carries rewards paid in a pair mint.

mod lp_fee_received_tests {
    use aargau_manager::utils::raydium::lp_fee::lp_fee_received;
    use aargau_manager::utils::token_2022::TransferFeeSnapshot;

    fn fee(transfer_fee_bps: u16, maximum_fee: u64) -> Option<TransferFeeSnapshot> {
        Some(TransferFeeSnapshot {
            transfer_fee_bps,
            maximum_fee,
        })
    }

    #[test]
    fn spl_classic_receives_the_full_outflow() {
        assert_eq!(lp_fee_received(0, None), 0);
        assert_eq!(lp_fee_received(1, None), 1);
        assert_eq!(lp_fee_received(1_000_000, None), 1_000_000);
        assert_eq!(lp_fee_received(u64::MAX, None), u64::MAX);
    }

    #[test]
    fn token_2022_zero_bps_receives_the_full_outflow() {
        assert_eq!(lp_fee_received(1_000_000, fee(0, u64::MAX)), 1_000_000);
    }

    #[test]
    fn token_2022_fee_is_deducted() {
        // 1% of 10_000 = 100.
        assert_eq!(lp_fee_received(10_000, fee(100, u64::MAX)), 9_900);
    }

    #[test]
    fn token_2022_fee_rounds_up_so_received_rounds_down() {
        // ceil(1 * 1 / 10_000) = 1 → nothing received.
        assert_eq!(lp_fee_received(1, fee(1, u64::MAX)), 0);
        // ceil(199 * 50 / 10_000) = ceil(0.995) = 1.
        assert_eq!(lp_fee_received(199, fee(50, u64::MAX)), 198);
    }

    #[test]
    fn token_2022_fee_is_capped_by_maximum_fee() {
        // 10% of 1_000_000 = 100_000, capped at 25.
        assert_eq!(lp_fee_received(1_000_000, fee(1_000, 25)), 999_975);
    }

    #[test]
    fn zero_outflow_receives_zero() {
        assert_eq!(lp_fee_received(0, fee(500, u64::MAX)), 0);
    }

    #[test]
    fn u64_max_outflow_does_not_overflow() {
        // 100% fee takes everything; capped fee leaves the rest.
        assert_eq!(lp_fee_received(u64::MAX, fee(10_000, u64::MAX)), 0);
        assert_eq!(lp_fee_received(u64::MAX, fee(10_000, 7)), u64::MAX - 7);
    }
}

mod pool_vault_outflow_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::raydium::lp_fee::pool_vault_outflow;

    #[test]
    fn outflow_is_before_minus_after() {
        assert_eq!(pool_vault_outflow(10, 10).unwrap(), 0);
        assert_eq!(pool_vault_outflow(10, 9).unwrap(), 1);
        assert_eq!(pool_vault_outflow(u64::MAX, 0).unwrap(), u64::MAX);
    }

    #[test]
    fn pool_vault_growth_is_a_measurement_mismatch() {
        let err = pool_vault_outflow(9, 10).unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::LpFeeMeasurementMismatch)
        );
    }
}

mod measure_lp_fee_received_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::raydium::lp_fee::{measure_lp_fee_received, LpFeeLegBalances};
    use aargau_manager::utils::token_2022::TransferFeeSnapshot;

    fn balances(
        pool_vault_before: u64,
        pool_vault_after: u64,
        vault_ata_before: u64,
        vault_ata_after: u64,
    ) -> LpFeeLegBalances {
        LpFeeLegBalances {
            pool_vault_before,
            pool_vault_after,
            vault_ata_before,
            vault_ata_after,
        }
    }

    #[test]
    fn spl_fee_equals_outflow_when_no_reward_in_the_mint() {
        let measured = measure_lp_fee_received(&balances(1_000, 900, 50, 150), None).unwrap();
        assert_eq!(measured, 100);
    }

    #[test]
    fn reward_in_a_pair_mint_is_not_part_of_the_fee_base() {
        // Pool vault pays 100 of LP fees; the reward vault pays 40 of the same
        // mint into the same ATA. Only the 100 is taxable.
        let measured = measure_lp_fee_received(&balances(1_000, 900, 0, 140), None).unwrap();
        assert_eq!(measured, 100);
    }

    #[test]
    fn token_2022_fee_base_is_what_the_vault_received() {
        let transfer_fee = Some(TransferFeeSnapshot {
            transfer_fee_bps: 100,
            maximum_fee: u64::MAX,
        });
        // Outflow 10_000, 1% withheld → vault receives 9_900.
        let measured =
            measure_lp_fee_received(&balances(10_000, 0, 0, 9_900), transfer_fee).unwrap();
        assert_eq!(measured, 9_900);
    }

    #[test]
    fn zero_fees_measure_zero() {
        assert_eq!(
            measure_lp_fee_received(&balances(7, 7, 3, 3), None).unwrap(),
            0
        );
    }

    #[test]
    fn u64_max_edges_do_not_overflow() {
        let measured = measure_lp_fee_received(&balances(u64::MAX, 0, 0, u64::MAX), None).unwrap();
        assert_eq!(measured, u64::MAX);
    }

    #[test]
    fn fee_larger_than_the_vault_increase_is_rejected() {
        // A recipient other than the vault ATA (or a stale fee config) would
        // show an outflow the vault never received.
        let err = measure_lp_fee_received(&balances(1_000, 900, 0, 99), None).unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::LpFeeMeasurementMismatch)
        );
    }

    #[test]
    fn missing_transfer_fee_on_a_fee_mint_is_caught_by_the_bound() {
        // If the fee config were ignored the gross 10_000 would exceed the
        // 9_900 actually received.
        let err = measure_lp_fee_received(&balances(10_000, 0, 0, 9_900), None).unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::LpFeeMeasurementMismatch)
        );
    }

    #[test]
    fn growing_pool_vault_is_rejected() {
        let err = measure_lp_fee_received(&balances(900, 1_000, 0, 0), None).unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::LpFeeMeasurementMismatch)
        );
    }

    #[test]
    fn shrinking_vault_ata_is_rejected() {
        let err = measure_lp_fee_received(&balances(1_000, 1_000, 5, 4), None).unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::PostCpiBalanceDecreased)
        );
    }
}
