//! Tests for `execute_action_raydium` that need no SVM runtime: the
//! instruction wire shape, the exact `remaining_accounts` count per action,
//! and every pure guard / arithmetic helper the handler composes.
//!
//! The end-to-end replay against the pinned mainnet `raydium_clmm.so` is the
//! verification gate named in the handler's `//!` header and lives with the
//! Raydium fixtures.

mod wire_shape_tests {
    use aargau_manager::instructions::ExecuteActionRaydiumParams;
    use aargau_manager::state::RaydiumKeeperAction;
    use anchor_lang::prelude::{AccountMeta, Pubkey};
    use anchor_lang::{Discriminator, InstructionData, ToAccountMetas};
    use sha2::{Digest, Sha256};

    #[test]
    fn instruction_discriminator_is_sha256_of_the_global_name() {
        let digest = Sha256::digest(b"global:execute_action_raydium");
        assert_eq!(
            aargau_manager::instruction::ExecuteActionRaydium::DISCRIMINATOR,
            &digest[..8]
        );
    }

    #[test]
    fn params_serialize_as_the_bare_action() {
        let action = RaydiumKeeperAction::DecreaseLiquidity {
            liquidity_amount: 5,
            token_min_a: 6,
            token_min_b: 7,
        };
        let data = aargau_manager::instruction::ExecuteActionRaydium {
            params: ExecuteActionRaydiumParams { action },
        }
        .data();
        let mut expected = Sha256::digest(b"global:execute_action_raydium")[..8].to_vec();
        expected.extend(borsh::to_vec(&action).unwrap());
        assert_eq!(data, expected);
    }

    fn key(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    /// Fixed account order and flags off-chain builders must follow. The NFT
    /// mint is not a signer in the struct: `OpenPosition` callers must set
    /// `is_signer` on that meta themselves (the handler rejects it otherwise).
    #[test]
    fn account_metas_follow_the_declared_order_and_flags() {
        let accounts = aargau_manager::accounts::ExecuteActionRaydium {
            user: key(1),
            protocol_config: key(2),
            vault: key(3),
            mint_a: key(4),
            mint_b: key(5),
            vault_token_a: key(6),
            vault_token_b: key(7),
            treasury_pda: key(8),
            treasury_token_a: key(9),
            treasury_token_b: key(10),
            pool_state: key(11),
            personal_position: key(12),
            position_nft_mint: key(13),
            position_nft_account: key(14),
            token_vault_0: key(15),
            token_vault_1: key(16),
            tick_array_lower: key(17),
            tick_array_upper: key(18),
            tick_array_bitmap_extension: key(19),
            token_program: key(20),
            token_2022_program: key(21),
            memo_program: key(22),
            clmm_program: key(23),
            associated_token_program: key(24),
            system_program: key(25),
            rent: key(26),
        };
        let expected = vec![
            AccountMeta::new(key(1), true),
            AccountMeta::new_readonly(key(2), false),
            AccountMeta::new(key(3), false),
            AccountMeta::new_readonly(key(4), false),
            AccountMeta::new_readonly(key(5), false),
            AccountMeta::new(key(6), false),
            AccountMeta::new(key(7), false),
            AccountMeta::new_readonly(key(8), false),
            AccountMeta::new(key(9), false),
            AccountMeta::new(key(10), false),
            AccountMeta::new(key(11), false),
            AccountMeta::new(key(12), false),
            AccountMeta::new(key(13), false),
            AccountMeta::new(key(14), false),
            AccountMeta::new(key(15), false),
            AccountMeta::new(key(16), false),
            AccountMeta::new(key(17), false),
            AccountMeta::new(key(18), false),
            AccountMeta::new(key(19), false),
            AccountMeta::new_readonly(key(20), false),
            AccountMeta::new_readonly(key(21), false),
            AccountMeta::new_readonly(key(22), false),
            AccountMeta::new_readonly(key(23), false),
            AccountMeta::new_readonly(key(24), false),
            AccountMeta::new_readonly(key(25), false),
            AccountMeta::new_readonly(key(26), false),
        ];
        assert_eq!(accounts.to_account_metas(None), expected);
    }
}

mod remaining_account_count_tests {
    use aargau_manager::instructions::raydium_remaining_account_count;
    use aargau_manager::state::RaydiumKeeperAction;
    use aargau_manager::utils::raydium::pool_state_view::{PoolStateView, RaydiumRewardSlot};
    use anchor_lang::prelude::Pubkey;

    fn fixed(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn pool_with_initialized_rewards(count: usize) -> PoolStateView {
        let mut rewards = [RaydiumRewardSlot::default(); 3];
        for (index, slot) in rewards.iter_mut().take(count).enumerate() {
            let byte = 0xc0 + index as u8;
            *slot = RaydiumRewardSlot {
                token_mint: fixed(byte),
                token_vault: fixed(byte + 0x10),
            };
        }
        PoolStateView {
            token_mint_0: fixed(0xa1),
            token_mint_1: fixed(0xb1),
            token_vault_0: fixed(0xa2),
            token_vault_1: fixed(0xb2),
            tick_spacing: 1,
            rewards,
        }
    }

    fn all_actions() -> [RaydiumKeeperAction; 5] {
        [
            RaydiumKeeperAction::OpenPosition {
                tick_lower: -60,
                tick_upper: 60,
            },
            RaydiumKeeperAction::IncreaseLiquidity {
                liquidity_amount: 1,
                token_max_a: 1,
                token_max_b: 1,
            },
            RaydiumKeeperAction::DecreaseLiquidity {
                liquidity_amount: 1,
                token_min_a: 1,
                token_min_b: 0,
            },
            RaydiumKeeperAction::CollectFees,
            RaydiumKeeperAction::ClosePosition,
        ]
    }

    fn takes_reward_triples(action: &RaydiumKeeperAction) -> bool {
        matches!(
            action,
            RaydiumKeeperAction::CollectFees | RaydiumKeeperAction::DecreaseLiquidity { .. }
        )
    }

    #[test]
    fn decrease_based_actions_take_three_accounts_per_initialized_slot() {
        for initialized in 0..=3 {
            let pool = pool_with_initialized_rewards(initialized);
            for action in all_actions().iter().filter(|a| takes_reward_triples(a)) {
                assert_eq!(
                    raydium_remaining_account_count(action, &pool).unwrap(),
                    initialized * 3,
                    "{action:?} with {initialized} initialized slots"
                );
            }
        }
    }

    #[test]
    fn open_increase_and_close_take_no_remaining_accounts() {
        let pool = pool_with_initialized_rewards(3);
        for action in all_actions().iter().filter(|a| !takes_reward_triples(a)) {
            assert_eq!(raydium_remaining_account_count(action, &pool).unwrap(), 0);
        }
    }

    #[test]
    fn gaps_between_initialized_slots_still_count_only_initialized_ones() {
        let mut pool = pool_with_initialized_rewards(3);
        pool.rewards[1] = RaydiumRewardSlot::default();
        assert_eq!(
            raydium_remaining_account_count(&RaydiumKeeperAction::CollectFees, &pool).unwrap(),
            6
        );
    }
}

mod open_guard_tests {
    use aargau_manager::instructions::is_fresh_position_nft_mint;
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

mod increase_guard_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::errors::AargauError;
    use aargau_manager::instructions::{
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

mod decrease_guard_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::errors::AargauError;
    use aargau_manager::instructions::{
        require_principal_meets_min, require_raydium_decrease_bounds,
    };

    fn assert_error(result: anchor_lang::Result<()>, expected: AargauError) {
        assert_eq!(err_code(&result.unwrap_err()), aargau_err_code(expected));
    }

    #[test]
    fn accepts_partial_and_full_decreases() {
        assert!(require_raydium_decrease_bounds(1, 1, 1, 0).is_ok());
        assert!(require_raydium_decrease_bounds(1, 100, 0, 1).is_ok());
        assert!(require_raydium_decrease_bounds(u128::MAX, u128::MAX, u64::MAX, u64::MAX).is_ok());
    }

    #[test]
    fn zero_liquidity_is_rejected() {
        // A zero decrease is `CollectFees`; routing it through Decrease would
        // skip nothing but is not a decrease.
        assert_error(
            require_raydium_decrease_bounds(0, 100, 1, 1),
            AargauError::InvalidActionPayload,
        );
    }

    #[test]
    fn more_than_the_position_liquidity_is_rejected() {
        assert_error(
            require_raydium_decrease_bounds(101, 100, 1, 1),
            AargauError::InvalidActionPayload,
        );
        assert_error(
            require_raydium_decrease_bounds(1, 0, 1, 1),
            AargauError::InvalidActionPayload,
        );
    }

    #[test]
    fn zero_zero_slippage_floor_is_rejected() {
        // Only `emergency_withdraw` may exit without a floor.
        assert_error(
            require_raydium_decrease_bounds(1, 100, 0, 0),
            AargauError::SlippageExceeded,
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
        assert_error(
            require_principal_meets_min(4, 5),
            AargauError::SlippageExceeded,
        );
        assert_error(
            require_principal_meets_min(0, 1),
            AargauError::SlippageExceeded,
        );
    }
}

mod close_guard_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::errors::AargauError;
    use aargau_manager::instructions::require_raydium_position_empty;
    use aargau_manager::utils::raydium::personal_position_view::PersonalPositionView;
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
    use aargau_manager::instructions::{observed_credit, observed_debit};

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
}

mod lp_fee_split_tests {
    use aargau_manager::instructions::{split_lp_fee, LpFeeSplit};

    #[test]
    fn zero_fee_splits_to_zero() {
        assert_eq!(
            split_lp_fee(0, 2_000).unwrap(),
            LpFeeSplit {
                gross: 0,
                aargau_fee: 0,
                user_net: 0,
            }
        );
    }

    #[test]
    fn one_unit_rounds_the_protocol_fee_down() {
        assert_eq!(
            split_lp_fee(1, 2_000).unwrap(),
            LpFeeSplit {
                gross: 1,
                aargau_fee: 0,
                user_net: 1,
            }
        );
    }

    #[test]
    fn typical_split_at_the_fee_cap() {
        assert_eq!(
            split_lp_fee(1_000_000, 2_000).unwrap(),
            LpFeeSplit {
                gross: 1_000_000,
                aargau_fee: 200_000,
                user_net: 800_000,
            }
        );
    }

    #[test]
    fn u64_max_does_not_overflow_and_legs_sum_to_gross() {
        let split = split_lp_fee(u64::MAX, 2_000).unwrap();
        assert_eq!(split.aargau_fee, u64::MAX / 5);
        assert_eq!(split.aargau_fee + split.user_net, u64::MAX);
    }

    #[test]
    fn zero_rate_leaves_everything_to_the_user() {
        let split = split_lp_fee(12_345, 0).unwrap();
        assert_eq!(split.aargau_fee, 0);
        assert_eq!(split.user_net, 12_345);
    }
}
