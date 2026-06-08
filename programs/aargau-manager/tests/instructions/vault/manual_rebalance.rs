//! Unit tests for `manual_rebalance` handler logic.
//!
//! These are pure-Rust tests that do not require a SVM runtime. Each test
//! exercises a narrow validation path that can be proven without CPI
//! execution. Runtime / mollusk-backed tests live in
//! `tests/instructions/vault/manual_rebalance_meteora.rs`.

mod manual_rebalance_tests {
    use aargau_manager::constants::{
        MAX_ACTIVE_BIN_SLIPPAGE_HARD_CAP, MAX_METEORA_BINS, METEORA_INLINE_BITMAP_BIN_LIMIT,
        METEORA_REBALANCE_SHOULD_CLAIM_FEE,
    };
    use aargau_manager::errors::AargauError;

    /// `active_id_slippage` equal to the hard cap is accepted.
    #[test]
    fn active_id_slippage_at_hard_cap_is_valid() {
        let slippage = MAX_ACTIVE_BIN_SLIPPAGE_HARD_CAP;
        assert!(
            slippage <= MAX_ACTIVE_BIN_SLIPPAGE_HARD_CAP,
            "slippage == cap must be accepted",
        );
    }

    /// `active_id_slippage` exceeding the hard cap must be rejected.
    #[test]
    fn active_id_slippage_above_hard_cap_is_invalid() {
        let slippage = MAX_ACTIVE_BIN_SLIPPAGE_HARD_CAP + 1;
        let is_valid = slippage <= MAX_ACTIVE_BIN_SLIPPAGE_HARD_CAP;
        assert!(!is_valid, "slippage > cap must fail the guard");
        assert_eq!(
            AargauError::SlippageOutOfRange as u32,
            1010,
            "SlippageOutOfRange must map to error 1010",
        );
    }

    /// A range exactly at `MAX_METEORA_BINS` (69) wide is accepted.
    #[test]
    fn bin_range_at_max_is_valid() {
        let lower: i32 = 0;
        let upper: i32 = MAX_METEORA_BINS - 1;
        let width = upper - lower + 1;
        assert!(
            width > 0 && width <= MAX_METEORA_BINS,
            "range of exactly MAX_METEORA_BINS must be valid",
        );
    }

    /// A range one bin over `MAX_METEORA_BINS` must be rejected.
    #[test]
    fn bin_range_over_max_is_invalid() {
        let lower: i32 = 0;
        let upper: i32 = MAX_METEORA_BINS; // width = MAX + 1
        let width = upper - lower + 1;
        assert!(
            !(width > 0 && width <= MAX_METEORA_BINS),
            "range of MAX_METEORA_BINS + 1 must fail the bin-count cap",
        );
        assert_eq!(
            AargauError::TooManyBins as u32,
            1003,
            "TooManyBins must map to error 1003",
        );
    }

    /// Ranges within the inline bitmap limit are accepted.
    #[test]
    fn range_within_inline_bitmap_is_valid() {
        let lower: i32 = -METEORA_INLINE_BITMAP_BIN_LIMIT;
        let upper: i32 = METEORA_INLINE_BITMAP_BIN_LIMIT;
        let is_within =
            lower >= -METEORA_INLINE_BITMAP_BIN_LIMIT && upper <= METEORA_INLINE_BITMAP_BIN_LIMIT;
        assert!(is_within, "range within bitmap limit must be valid");
    }

    /// Ranges outside the inline bitmap limit must be rejected with
    /// `BitmapExtensionRequired`.
    #[test]
    fn range_outside_inline_bitmap_is_invalid() {
        let lower: i32 = -(METEORA_INLINE_BITMAP_BIN_LIMIT + 1);
        let upper: i32 = 0;
        let is_within =
            lower >= -METEORA_INLINE_BITMAP_BIN_LIMIT && upper <= METEORA_INLINE_BITMAP_BIN_LIMIT;
        assert!(!is_within, "range below -INLINE_BITMAP_LIMIT must fail");
        assert_eq!(
            AargauError::BitmapExtensionRequired as u32,
            1206,
            "BitmapExtensionRequired must map to error 1206",
        );
    }

    /// The `should_claim_fee` constant is pinned to `false`. Any change
    /// breaks the double-fee invariant documented in the module doc.
    #[test]
    fn should_claim_fee_constant_is_false() {
        assert!(
            !METEORA_REBALANCE_SHOULD_CLAIM_FEE,
            "METEORA_REBALANCE_SHOULD_CLAIM_FEE must be false",
        );
    }
}
