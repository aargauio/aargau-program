mod vault_ops_tests {
    use aargau_manager::utils::vault_ops::{
        require_bin_count_within_cap, require_range_within_inline_bitmap,
    };

    // -------------------------------------------------------------------------
    // require_bin_count_within_cap
    // -------------------------------------------------------------------------

    #[test]
    fn bin_count_within_cap_accepts_valid_range() {
        // A range of 1 bin — the minimum valid width.
        assert!(require_bin_count_within_cap(0, 0).is_ok());
        // A range exactly at MAX_METEORA_BINS (69 bins).
        assert!(require_bin_count_within_cap(0, 68).is_ok());
    }

    #[test]
    fn bin_count_lower_gt_upper_returns_error() {
        // lower > upper produces a non-positive width — must be rejected.
        let result = require_bin_count_within_cap(10, 5);
        assert!(result.is_err());
    }

    #[test]
    fn bin_count_equals_lower_upper_is_one_bin() {
        // lower == upper means exactly 1 bin, which is valid.
        assert!(require_bin_count_within_cap(42, 42).is_ok());
    }

    #[test]
    fn bin_count_exceeds_cap_returns_error() {
        // 70 bins exceeds MAX_METEORA_BINS (69).
        let result = require_bin_count_within_cap(0, 69);
        assert!(result.is_err());
    }

    // -------------------------------------------------------------------------
    // require_range_within_inline_bitmap
    // -------------------------------------------------------------------------

    #[test]
    fn bitmap_range_within_limit_is_ok() {
        // Both endpoints within the inline bitmap range (|bin_id| <= 5460).
        assert!(require_range_within_inline_bitmap(-5460, 5460).is_ok());
        assert!(require_range_within_inline_bitmap(0, 100).is_ok());
    }

    #[test]
    fn bitmap_extension_out_of_range_returns_error() {
        // -5461 is beyond the inline bitmap limit of 5460 bins from centre.
        // (The spec says 512*64=32768 as the theoretical Meteora max, but
        // METEORA_INLINE_BITMAP_BIN_LIMIT is set to 5460 as the safe limit.)
        let result = require_range_within_inline_bitmap(-5461, 0);
        assert!(result.is_err());
    }

    #[test]
    fn bitmap_upper_out_of_range_returns_error() {
        // Upper endpoint > METEORA_INLINE_BITMAP_BIN_LIMIT (5460) must also fail.
        let result = require_range_within_inline_bitmap(0, 5461);
        assert!(result.is_err());
    }
}
