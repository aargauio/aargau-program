//! Tests for `src/utils/raydium/position_guards.rs` — the pure rules every
//! Raydium CLMM instruction shares: fresh NFT mint, openable tick range,
//! increase bounds, full-drain slippage floor, empty-position check and the
//! observed balance deltas around a CPI.

mod open_guard_tests {
    use aargau_manager::utils::raydium::position_guards::is_fresh_position_nft_mint;
    use anchor_lang::prelude::Pubkey;

    const SYSTEM_PROGRAM: Pubkey = anchor_lang::system_program::ID;

    #[test]
    fn fresh_signer_keypair_is_accepted() {
        assert!(is_fresh_position_nft_mint(true, &SYSTEM_PROGRAM, 0));
    }

    #[test]
    fn unsigned_mint_is_rejected() {
        assert!(!is_fresh_position_nft_mint(false, &SYSTEM_PROGRAM, 0));
    }

    #[test]
    fn existing_mint_is_rejected() {
        let token_2022 = aargau_manager::constants::TOKEN_2022_PROGRAM_ID;
        assert!(!is_fresh_position_nft_mint(true, &token_2022, 82));
        assert!(!is_fresh_position_nft_mint(true, &token_2022, 0));
        assert!(!is_fresh_position_nft_mint(true, &SYSTEM_PROGRAM, 1));
    }
}

mod no_remaining_accounts_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::raydium::position_guards::require_no_remaining_accounts;

    #[test]
    fn empty_remaining_accounts_pass() {
        assert!(require_no_remaining_accounts(0).is_ok());
    }

    #[test]
    fn any_remaining_account_is_rejected() {
        for count in [1, 3, 9, usize::MAX] {
            let err = require_no_remaining_accounts(count).unwrap_err();
            assert_eq!(
                err_code(&err),
                aargau_err_code(AargauError::InvalidActionPayload)
            );
        }
    }
}

mod openable_tick_range_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::constants::{RAYDIUM_MAX_TICK, RAYDIUM_MIN_TICK};
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::raydium::position_guards::require_openable_tick_range;

    fn assert_error(result: anchor_lang::Result<()>, expected: AargauError) {
        assert_eq!(err_code(&result.unwrap_err()), aargau_err_code(expected));
    }

    #[test]
    fn aligned_ordered_range_is_accepted() {
        assert!(require_openable_tick_range(-60, 60, 1).is_ok());
        assert!(require_openable_tick_range(-600, 600, 10).is_ok());
        assert!(require_openable_tick_range(0, 1, 1).is_ok());
        assert!(require_openable_tick_range(-120, -60, 60).is_ok());
    }

    #[test]
    fn full_tick_domain_is_accepted_at_spacing_one() {
        assert!(require_openable_tick_range(RAYDIUM_MIN_TICK, RAYDIUM_MAX_TICK, 1).is_ok());
    }

    #[test]
    fn equal_or_inverted_bounds_are_rejected() {
        assert_error(
            require_openable_tick_range(60, 60, 1),
            AargauError::InvalidActionPayload,
        );
        assert_error(
            require_openable_tick_range(60, -60, 1),
            AargauError::InvalidActionPayload,
        );
        assert_error(
            require_openable_tick_range(i32::MAX, i32::MIN, 1),
            AargauError::InvalidActionPayload,
        );
    }

    #[test]
    fn misaligned_bounds_are_rejected_on_either_side() {
        assert_error(
            require_openable_tick_range(-59, 60, 10),
            AargauError::InvalidActionPayload,
        );
        assert_error(
            require_openable_tick_range(-60, 61, 10),
            AargauError::InvalidActionPayload,
        );
        assert_error(
            require_openable_tick_range(1, 10, 10),
            AargauError::InvalidActionPayload,
        );
    }

    #[test]
    fn bounds_outside_the_tick_domain_are_rejected() {
        assert_error(
            require_openable_tick_range(RAYDIUM_MIN_TICK - 1, 0, 1),
            AargauError::InvalidActionPayload,
        );
        assert_error(
            require_openable_tick_range(0, RAYDIUM_MAX_TICK + 1, 1),
            AargauError::InvalidActionPayload,
        );
        assert_error(
            require_openable_tick_range(i32::MIN, i32::MAX, 1),
            AargauError::InvalidActionPayload,
        );
    }

    #[test]
    fn largest_aligned_tick_inside_the_domain_is_accepted() {
        // 443_636 is not a multiple of 10; the highest usable tick is 443_630.
        assert!(require_openable_tick_range(0, 443_630, 10).is_ok());
        assert_error(
            require_openable_tick_range(0, RAYDIUM_MAX_TICK, 10),
            AargauError::InvalidActionPayload,
        );
    }

    #[test]
    fn zero_tick_spacing_is_a_division_by_zero() {
        assert_error(
            require_openable_tick_range(-60, 60, 0),
            AargauError::DivisionByZero,
        );
    }

    #[test]
    fn maximum_tick_spacing_aligns_only_its_multiples() {
        let spacing = u16::MAX;
        let step = i32::from(spacing);
        assert!(require_openable_tick_range(-step, step, spacing).is_ok());
        assert_error(
            require_openable_tick_range(-step, step + 1, spacing),
            AargauError::InvalidActionPayload,
        );
    }
}

mod increase_guard_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::raydium::position_guards::{
        require_consumed_within_max, require_raydium_increase_bounds,
    };

    fn assert_error(result: anchor_lang::Result<()>, expected: AargauError) {
        assert_eq!(err_code(&result.unwrap_err()), aargau_err_code(expected));
    }

    #[test]
    fn accepts_bounded_payload() {
        assert!(require_raydium_increase_bounds(1, 1, 0, 1, 0).is_ok());
        assert!(
            require_raydium_increase_bounds(u128::MAX, u64::MAX, u64::MAX, u64::MAX, u64::MAX)
                .is_ok()
        );
    }

    #[test]
    fn zero_liquidity_is_rejected() {
        assert_error(
            require_raydium_increase_bounds(0, 10, 10, 10, 10),
            AargauError::InvalidActionPayload,
        );
    }

    #[test]
    fn both_maxima_zero_is_rejected() {
        assert_error(
            require_raydium_increase_bounds(1, 0, 0, 10, 10),
            AargauError::InvalidActionPayload,
        );
    }

    #[test]
    fn maximum_above_the_vault_balance_is_rejected_on_either_side() {
        assert_error(
            require_raydium_increase_bounds(1, 11, 0, 10, 10),
            AargauError::InsufficientFunds,
        );
        assert_error(
            require_raydium_increase_bounds(1, 0, 11, 10, 10),
            AargauError::InsufficientFunds,
        );
        assert_error(
            require_raydium_increase_bounds(1, u64::MAX, 0, u64::MAX - 1, 0),
            AargauError::InsufficientFunds,
        );
    }

    #[test]
    fn observed_debit_within_the_maximum_passes() {
        assert!(require_consumed_within_max(0, 0).is_ok());
        assert!(require_consumed_within_max(10, 10).is_ok());
        assert!(require_consumed_within_max(u64::MAX, u64::MAX).is_ok());
    }

    #[test]
    fn observed_debit_above_the_maximum_is_slippage() {
        assert_error(
            require_consumed_within_max(1, 0),
            AargauError::SlippageExceeded,
        );
        assert_error(
            require_consumed_within_max(u64::MAX, u64::MAX - 1),
            AargauError::SlippageExceeded,
        );
    }
}

mod full_drain_floor_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::raydium::position_guards::require_full_drain_floor;

    fn assert_slippage(result: anchor_lang::Result<()>) {
        assert_eq!(
            err_code(&result.unwrap_err()),
            aargau_err_code(AargauError::SlippageExceeded)
        );
    }

    #[test]
    fn position_with_liquidity_rejects_a_zero_zero_floor() {
        assert_slippage(require_full_drain_floor(1, 0, 0));
        assert_slippage(require_full_drain_floor(u128::MAX, 0, 0));
    }

    #[test]
    fn position_with_liquidity_accepts_a_single_sided_floor() {
        assert!(require_full_drain_floor(1, 1, 0).is_ok());
        assert!(require_full_drain_floor(1, 0, 1).is_ok());
        assert!(require_full_drain_floor(u128::MAX, u64::MAX, u64::MAX).is_ok());
    }

    #[test]
    fn empty_position_needs_no_floor() {
        assert!(require_full_drain_floor(0, 0, 0).is_ok());
        assert!(require_full_drain_floor(0, 1, u64::MAX).is_ok());
    }
}

mod position_empty_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::raydium::personal_position_view::PersonalPositionView;
    use aargau_manager::utils::raydium::position_guards::require_raydium_position_empty;
    use anchor_lang::prelude::Pubkey;

    fn empty_position() -> PersonalPositionView {
        PersonalPositionView {
            nft_mint: Pubkey::new_from_array([1; 32]),
            pool_id: Pubkey::new_from_array([2; 32]),
            tick_lower: -60,
            tick_upper: 60,
            liquidity: 0,
            token_fees_owed_0: 0,
            token_fees_owed_1: 0,
            reward_amounts_owed: [0; 3],
        }
    }

    fn assert_not_empty(position: &PersonalPositionView) {
        let err = require_raydium_position_empty(position).unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::PositionNotEmpty)
        );
    }

    #[test]
    fn drained_position_can_close() {
        assert!(require_raydium_position_empty(&empty_position()).is_ok());
    }

    #[test]
    fn remaining_liquidity_blocks_close() {
        let mut position = empty_position();
        position.liquidity = 1;
        assert_not_empty(&position);
        position.liquidity = u128::MAX;
        assert_not_empty(&position);
    }

    #[test]
    fn owed_fees_block_close() {
        let mut position = empty_position();
        position.token_fees_owed_0 = 1;
        assert_not_empty(&position);

        let mut position = empty_position();
        position.token_fees_owed_1 = u64::MAX;
        assert_not_empty(&position);
    }

    #[test]
    fn owed_rewards_in_any_slot_block_close() {
        for slot in 0..3 {
            let mut position = empty_position();
            position.reward_amounts_owed[slot] = 1;
            assert_not_empty(&position);
        }
    }
}

mod balance_delta_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::raydium::position_guards::{
        observed_credit, observed_debit, require_principal_meets_min,
    };

    #[test]
    fn debit_is_before_minus_after() {
        assert_eq!(observed_debit(10, 10).unwrap(), 0);
        assert_eq!(observed_debit(10, 9).unwrap(), 1);
        assert_eq!(observed_debit(u64::MAX, 0).unwrap(), u64::MAX);
    }

    #[test]
    fn balance_growth_during_a_debit_is_an_underflow() {
        let err = observed_debit(9, 10).unwrap_err();
        assert_eq!(err_code(&err), aargau_err_code(AargauError::Underflow));
    }

    #[test]
    fn credit_is_after_minus_before() {
        assert_eq!(observed_credit(10, 10).unwrap(), 0);
        assert_eq!(observed_credit(9, 10).unwrap(), 1);
        assert_eq!(observed_credit(0, u64::MAX).unwrap(), u64::MAX);
    }

    #[test]
    fn balance_drop_during_a_credit_is_rejected() {
        let err = observed_credit(10, 9).unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::PostCpiBalanceDecreased)
        );
    }

    #[test]
    fn principal_at_or_above_the_floor_passes() {
        assert!(require_principal_meets_min(0, 0).is_ok());
        assert!(require_principal_meets_min(5, 5).is_ok());
        assert!(require_principal_meets_min(u64::MAX, 1).is_ok());
    }

    #[test]
    fn principal_below_the_floor_is_slippage() {
        for (received, floor) in [(4, 5), (0, 1), (u64::MAX - 1, u64::MAX)] {
            let err = require_principal_meets_min(received, floor).unwrap_err();
            assert_eq!(
                err_code(&err),
                aargau_err_code(AargauError::SlippageExceeded)
            );
        }
    }
}
