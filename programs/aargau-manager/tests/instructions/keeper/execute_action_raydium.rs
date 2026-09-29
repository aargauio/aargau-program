//! Tests for `execute_action_raydium` that need no SVM runtime: the
//! instruction wire shape, the exact `remaining_accounts` count per action,
//! and the decrease payload bounds. The guards and balance arithmetic the
//! handler shares with the rebalance instructions are tested with
//! `utils/raydium/position_guards` and `utils/raydium/lp_fee`.
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

mod decrease_guard_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::errors::AargauError;
    use aargau_manager::instructions::require_raydium_decrease_bounds;

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
}
