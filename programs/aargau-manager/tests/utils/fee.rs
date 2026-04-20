//! Tests for `src/utils/fee.rs` — calc_aargau_fee and calc_net_after_transfer_fee.

#[cfg(test)]
mod fee_tests {
    use aargau_manager::utils::fee::{calc_aargau_fee, calc_net_after_transfer_fee};

    // --- calc_aargau_fee ---

    #[test]
    fn test_calc_aargau_fee_6pct_on_1000() {
        // 6% of 1000 = 60
        assert_eq!(calc_aargau_fee(1000, 600).unwrap(), 60);
    }

    #[test]
    fn test_calc_aargau_fee_6pct_on_zero_gross() {
        assert_eq!(calc_aargau_fee(0, 600).unwrap(), 0);
    }

    #[test]
    fn test_calc_aargau_fee_6pct_truncates_fractional_result() {
        // 1 token at 6% = 0.06 → integer division truncates to 0
        assert_eq!(calc_aargau_fee(1, 600).unwrap(), 0);
    }

    #[test]
    fn test_calc_aargau_fee_20pct_on_100() {
        // 20% of 100 = 20
        assert_eq!(calc_aargau_fee(100, 2000).unwrap(), 20);
    }

    // --- calc_net_after_transfer_fee ---

    #[test]
    fn test_calc_net_after_transfer_fee_zero_bps_returns_full_amount() {
        // bps = 0 means no transfer fee; net equals gross
        assert_eq!(calc_net_after_transfer_fee(10_000, 0, u64::MAX), 10_000);
    }

    #[test]
    fn test_calc_net_after_transfer_fee_1pct_on_10000() {
        // 1% of 10_000 = 100; net = 9_900
        assert_eq!(calc_net_after_transfer_fee(10_000, 100, u64::MAX), 9_900);
    }

    #[test]
    fn test_calc_net_after_transfer_fee_1pct_on_1000() {
        // 100 bps = 1%, of 1000 = 10; net = 990
        assert_eq!(calc_net_after_transfer_fee(1000, 100, u64::MAX), 990);
    }

    #[test]
    fn test_calc_net_after_transfer_fee_capped_by_max_fee() {
        // 10% of 10_000 = 1_000; but max_fee = 5; net = 9_995
        assert_eq!(calc_net_after_transfer_fee(10_000, 1000, 5), 9_995);
    }

    // --- calc_aargau_fee additional edge cases ---

    #[test]
    fn test_calc_aargau_fee_zero_rate_returns_zero() {
        // fee_rate_bps = 0 means Aargau takes nothing.
        assert_eq!(calc_aargau_fee(1_000_000, 0).unwrap(), 0);
    }

    #[test]
    fn test_calc_aargau_fee_full_10000_bps_returns_gross() {
        // 10_000 bps = 100%; fee equals the entire gross amount.
        assert_eq!(calc_aargau_fee(1000, 10_000).unwrap(), 1000);
    }

    #[test]
    fn test_calc_aargau_fee_max_u64_does_not_overflow() {
        // u64::MAX × 2000 bps fits in u128 before dividing by 10_000.
        // Result: u64::MAX * 2000 / 10_000 = u64::MAX / 5 = 3_689_348_814_741_910_323
        let result = calc_aargau_fee(u64::MAX, 2000);
        assert!(result.is_ok());
        // Verify the result is exactly u64::MAX / 5 (integer division).
        assert_eq!(result.unwrap(), u64::MAX / 5);
    }

    #[test]
    fn test_calc_aargau_fee_result_never_exceeds_gross() {
        // Fee must never be larger than the gross amount for any valid rate.
        let gross: u64 = 999_999_999;
        let fee = calc_aargau_fee(gross, 2000).unwrap();
        assert!(fee <= gross);
    }

    // --- calc_net_after_transfer_fee additional edge cases ---

    #[test]
    fn test_calc_net_after_transfer_fee_zero_amount() {
        // 0 transferred → 0 received regardless of rate.
        assert_eq!(calc_net_after_transfer_fee(0, 500, u64::MAX), 0);
    }

    #[test]
    fn test_calc_net_after_transfer_fee_max_fee_zero_deducts_nothing() {
        // max_fee = 0 means the fee is capped to zero; full amount returned.
        assert_eq!(calc_net_after_transfer_fee(10_000, 500, 0), 10_000);
    }

    #[test]
    fn test_calc_net_after_transfer_fee_ceiling_rounding() {
        // 1 token at 1 bps: raw_fee_numerator = 1 * 1 + (10_000 - 1) = 9_999.
        // ceil(1 * 1 / 10_000) = ceil(0.0001) = 1.
        // net = 1 - min(1, max_fee=u64::MAX) = 0.
        assert_eq!(calc_net_after_transfer_fee(1, 1, u64::MAX), 0);
    }

    #[test]
    fn test_calc_net_after_transfer_fee_near_u64_max_does_not_panic() {
        // Saturating arithmetic prevents overflow on extreme inputs.
        let net = calc_net_after_transfer_fee(u64::MAX, 10_000, u64::MAX);
        // After maximum fee deduction the net must be 0 (saturating_sub).
        assert_eq!(net, 0);
    }

    #[test]
    fn test_calc_net_after_transfer_fee_net_never_exceeds_amount() {
        // The function must never return more than the input amount.
        let amount: u64 = 100_000;
        let net = calc_net_after_transfer_fee(amount, 500, u64::MAX);
        assert!(net <= amount);
    }

    #[test]
    fn test_calc_net_after_transfer_fee_high_bps_but_low_max_fee() {
        // bps > 10_000 is a degenerate input; max_fee cap must still bound the result.
        // raw_fee would be huge but capped to max_fee = 10.
        let net = calc_net_after_transfer_fee(1_000_000, 65535, 10);
        assert_eq!(net, 1_000_000 - 10);
    }
}
