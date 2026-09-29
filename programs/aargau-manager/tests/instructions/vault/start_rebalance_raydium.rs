//! Tests for `start_rebalance_raydium` — phase one of the Raydium CLMM
//! two-transaction rebalance (CloseOld) — that need no SVM runtime: the
//! instruction wire shape, the vault-state machine, the exact
//! `remaining_accounts` count and the error contract.
//!
//! The shared guards it composes (openable target range, full-drain slippage
//! floor, empty-position check, observed min-out) are tested with
//! `utils/raydium/position_guards`; the fee split with `utils/raydium/lp_fee`.
//! The CPI sequence itself runs only in the replay against the pinned
//! `raydium_clmm.so`, the verification gate named in the handler header.

mod wire_shape_tests {
    use aargau_manager::instructions::StartRebalanceRaydiumParams;
    use anchor_lang::prelude::{AccountMeta, Pubkey};
    use anchor_lang::{Discriminator, InstructionData, ToAccountMetas};
    use sha2::{Digest, Sha256};

    fn global_discriminator() -> Vec<u8> {
        Sha256::digest(b"global:start_rebalance_raydium")[..8].to_vec()
    }

    #[test]
    fn instruction_discriminator_is_sha256_of_the_global_name() {
        assert_eq!(
            aargau_manager::instruction::StartRebalanceRaydium::DISCRIMINATOR,
            global_discriminator().as_slice()
        );
    }

    /// i32 + i32 + u64 + u64 = 24 bytes after the discriminator, in field
    /// order, little-endian. No liquidity field: the drain amount is read
    /// on-chain.
    #[test]
    fn params_serialize_in_declared_field_order() {
        let data = aargau_manager::instruction::StartRebalanceRaydium {
            params: StartRebalanceRaydiumParams {
                new_tick_lower: -120,
                new_tick_upper: 360,
                token_min_a: 0x1112_1314_1516_1718,
                token_min_b: 0x2122_2324_2526_2728,
            },
        }
        .data();
        let mut expected = global_discriminator();
        expected.extend((-120i32).to_le_bytes());
        expected.extend(360i32.to_le_bytes());
        expected.extend(0x1112_1314_1516_1718u64.to_le_bytes());
        expected.extend(0x2122_2324_2526_2728u64.to_le_bytes());
        assert_eq!(data, expected);
        assert_eq!(data.len(), 8 + 4 + 4 + 8 + 8);
    }

    #[test]
    fn params_edges_round_trip() {
        for (lower, upper, min_a, min_b) in [
            (0, 0, 0, 0),
            (1, 1, 1, 1),
            (i32::MIN, i32::MAX, u64::MAX, u64::MAX),
        ] {
            let params = StartRebalanceRaydiumParams {
                new_tick_lower: lower,
                new_tick_upper: upper,
                token_min_a: min_a,
                token_min_b: min_b,
            };
            let bytes = borsh::to_vec(&params).unwrap();
            let decoded: StartRebalanceRaydiumParams = borsh::from_slice(&bytes).unwrap();
            assert_eq!(decoded.new_tick_lower, lower);
            assert_eq!(decoded.new_tick_upper, upper);
            assert_eq!(decoded.token_min_a, min_a);
            assert_eq!(decoded.token_min_b, min_b);
        }
    }

    fn key(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    /// Fixed account order and flags off-chain builders must follow; the
    /// reward triples follow as `remaining_accounts`.
    #[test]
    fn account_metas_follow_the_declared_order_and_flags() {
        let accounts = aargau_manager::accounts::StartRebalanceRaydium {
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
            system_program: key(24),
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
        ];
        assert_eq!(accounts.to_account_metas(None), expected);
    }
}

mod state_machine_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use crate::common::vault_fixtures::vault_with_position;
    use aargau_manager::errors::AargauError;
    use aargau_manager::instructions::{record_pending_rebalance, require_rebalance_startable};
    use aargau_manager::state::Protocol;

    fn assert_error(result: anchor_lang::Result<()>, expected: AargauError) {
        assert_eq!(err_code(&result.unwrap_err()), aargau_err_code(expected));
    }

    #[test]
    fn raydium_vault_with_a_position_can_start() {
        assert!(
            require_rebalance_startable(&vault_with_position(Protocol::Raydium, -60, 60)).is_ok()
        );
    }

    #[test]
    fn other_protocols_are_rejected() {
        for protocol in [Protocol::Orca, Protocol::Meteora] {
            assert_error(
                require_rebalance_startable(&vault_with_position(protocol, -60, 60)),
                AargauError::InvalidPool,
            );
        }
    }

    #[test]
    fn start_records_the_target_range_and_forgets_the_old_position() {
        let mut vault = vault_with_position(Protocol::Raydium, -600, 600);
        record_pending_rebalance(&mut vault, -120, 240, 1_700_000_000);

        assert_eq!(vault.position_address, None);
        assert_eq!(vault.position_mint, None);
        assert_eq!(vault.position_range_lower, None);
        assert_eq!(vault.position_range_upper, None);
        let pending = vault.pending_rebalance.expect("pending rebalance");
        assert_eq!(pending.new_tick_lower, -120);
        assert_eq!(pending.new_tick_upper, 240);
        assert_eq!(pending.initiated_at, 1_700_000_000);
        assert_eq!(pending.retry_count, 0);
        assert_eq!(pending.last_failure_reason, 0);
    }

    #[test]
    fn repeated_start_fails_with_pending_rebalance_exists() {
        let mut vault = vault_with_position(Protocol::Raydium, -60, 60);
        require_rebalance_startable(&vault).unwrap();
        record_pending_rebalance(&mut vault, -120, 120, 1);
        assert_error(
            require_rebalance_startable(&vault),
            AargauError::PendingRebalanceExists,
        );
    }

    #[test]
    fn tick_edges_are_stored_verbatim() {
        let mut vault = vault_with_position(Protocol::Raydium, 0, 1);
        record_pending_rebalance(&mut vault, i32::MIN, i32::MAX, i64::MAX);
        let pending = vault.pending_rebalance.unwrap();
        assert_eq!(pending.new_tick_lower, i32::MIN);
        assert_eq!(pending.new_tick_upper, i32::MAX);
        assert_eq!(pending.initiated_at, i64::MAX);
    }
}

/// The start leg checks the target range with `require_openable_tick_range`
/// before it writes `pending_rebalance`, so a range the open leg could not
/// open never becomes pending. The start → retry round trip of an accepted
/// range is tested with `retry_pending_rebalance_raydium`.
mod target_range_tests {
    use aargau_manager::constants::{RAYDIUM_MAX_TICK, RAYDIUM_MIN_TICK};
    use aargau_manager::utils::raydium::position_guards::require_openable_tick_range;

    #[test]
    fn ranges_the_retry_could_not_open_are_rejected_at_start() {
        for (lower, upper, spacing) in [
            (-59, 60, 10),
            (60, 60, 1),
            (RAYDIUM_MIN_TICK - 1, 0, 1),
            (0, RAYDIUM_MAX_TICK + 1, 1),
            (-60, 60, 0),
        ] {
            assert!(require_openable_tick_range(lower, upper, spacing).is_err());
        }
    }
}

/// `remaining_accounts` must hold exactly one reward triple per initialized
/// pool reward slot; the count is checked before any account is read.
mod remaining_account_count_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::raydium::pool_state_view::{PoolStateView, RaydiumRewardSlot};
    use aargau_manager::utils::raydium::reward_accounts::{
        bind_reward_transfer_accounts, expected_reward_account_count,
    };
    use anchor_lang::prelude::{AccountInfo, Pubkey};

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
            tick_spacing: 10,
            rewards,
        }
    }

    /// Unbound placeholder accounts; the count check fails before any is read.
    fn placeholders(count: usize) -> Vec<AccountInfo<'static>> {
        (0..count)
            .map(|index| {
                let key = Box::leak(Box::new(fixed(index as u8)));
                let owner = Box::leak(Box::new(Pubkey::default()));
                let lamports = Box::leak(Box::new(0u64));
                let data: &'static mut [u8] = Box::leak(Vec::new().into_boxed_slice());
                AccountInfo::new(key, false, true, lamports, data, owner, false)
            })
            .collect()
    }

    fn assert_count_rejected(pool: &PoolStateView, count: usize) {
        let err = bind_reward_transfer_accounts(pool, &fixed(0xee), &placeholders(count))
            .err()
            .expect("wrong reward-account count must be rejected");
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidActionPayload)
        );
    }

    #[test]
    fn expected_count_is_three_per_initialized_slot() {
        for initialized in 0..=3 {
            let pool = pool_with_initialized_rewards(initialized);
            assert_eq!(
                expected_reward_account_count(&pool).unwrap(),
                initialized * 3
            );
        }
    }

    #[test]
    fn pool_without_rewards_takes_no_remaining_accounts() {
        let pool = pool_with_initialized_rewards(0);
        assert!(bind_reward_transfer_accounts(&pool, &fixed(0xee), &[])
            .unwrap()
            .is_empty());
        assert_count_rejected(&pool, 1);
        assert_count_rejected(&pool, 3);
    }

    #[test]
    fn one_account_short_or_over_is_rejected() {
        for initialized in 1..=3 {
            let pool = pool_with_initialized_rewards(initialized);
            let expected = initialized * 3;
            assert_count_rejected(&pool, expected - 1);
            assert_count_rejected(&pool, expected + 1);
            assert_count_rejected(&pool, 0);
        }
    }
}

mod error_contract_tests {
    use aargau_manager::errors::AargauError;

    /// Discriminants off-chain clients map for this instruction.
    #[test]
    fn error_codes_are_stable() {
        assert_eq!(AargauError::SlippageExceeded as u32, 1002);
        assert_eq!(AargauError::InvalidActionPayload as u32, 1007);
        assert_eq!(AargauError::InvalidPool as u32, 1200);
        assert_eq!(AargauError::VaultNoActivePosition as u32, 1302);
        assert_eq!(AargauError::PendingRebalanceExists as u32, 1303);
        assert_eq!(AargauError::PositionNotEmpty as u32, 1309);
        assert_eq!(AargauError::LpFeeMeasurementMismatch as u32, 1310);
        assert_eq!(AargauError::ProgramPaused as u32, 1400);
    }
}
