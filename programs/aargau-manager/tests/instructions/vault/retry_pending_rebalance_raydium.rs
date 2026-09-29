//! Tests for `retry_pending_rebalance_raydium` — phase two of the Raydium CLMM
//! two-transaction rebalance (OpenNew) — that need no SVM runtime: the
//! instruction wire shape, the vault-state machine and the error contract.
//!
//! The shared guards it composes (fresh NFT mint, openable range, increase
//! bounds, observed max-in, empty `remaining_accounts`) are tested with
//! `utils/raydium/position_guards`. The open + increase CPIs run only in the
//! replay against the pinned `raydium_clmm.so`, the verification gate named
//! in the handler header.

mod wire_shape_tests {
    use aargau_manager::instructions::RetryPendingRebalanceRaydiumParams;
    use anchor_lang::prelude::{AccountMeta, Pubkey};
    use anchor_lang::{Discriminator, InstructionData, ToAccountMetas};
    use sha2::{Digest, Sha256};

    fn global_discriminator() -> Vec<u8> {
        Sha256::digest(b"global:retry_pending_rebalance_raydium")[..8].to_vec()
    }

    #[test]
    fn instruction_discriminator_is_sha256_of_the_global_name() {
        assert_eq!(
            aargau_manager::instruction::RetryPendingRebalanceRaydium::DISCRIMINATOR,
            global_discriminator().as_slice()
        );
    }

    /// u128 + u64 + u64 = 32 bytes after the discriminator, little-endian. No
    /// tick params: the range comes from `pending_rebalance`.
    #[test]
    fn params_serialize_in_declared_field_order() {
        let data = aargau_manager::instruction::RetryPendingRebalanceRaydium {
            params: RetryPendingRebalanceRaydiumParams {
                liquidity_amount: 0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10,
                token_max_a: 0x1112_1314_1516_1718,
                token_max_b: 0x2122_2324_2526_2728,
            },
        }
        .data();
        let mut expected = global_discriminator();
        expected.extend(0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10u128.to_le_bytes());
        expected.extend(0x1112_1314_1516_1718u64.to_le_bytes());
        expected.extend(0x2122_2324_2526_2728u64.to_le_bytes());
        assert_eq!(data, expected);
        assert_eq!(data.len(), 8 + 16 + 8 + 8);
    }

    #[test]
    fn params_edges_round_trip() {
        for (liquidity, max_a, max_b) in [(0, 0, 0), (1, 1, 1), (u128::MAX, u64::MAX, u64::MAX)] {
            let params = RetryPendingRebalanceRaydiumParams {
                liquidity_amount: liquidity,
                token_max_a: max_a,
                token_max_b: max_b,
            };
            let bytes = borsh::to_vec(&params).unwrap();
            let decoded: RetryPendingRebalanceRaydiumParams = borsh::from_slice(&bytes).unwrap();
            assert_eq!(decoded.liquidity_amount, liquidity);
            assert_eq!(decoded.token_max_a, max_a);
            assert_eq!(decoded.token_max_b, max_b);
        }
    }

    fn key(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    /// Fixed account order and flags. The NFT mint is a declared signer, so
    /// the IDL tells clients to co-sign with the fresh keypair; there are no
    /// treasury accounts and no memo slot.
    #[test]
    fn account_metas_follow_the_declared_order_and_flags() {
        let accounts = aargau_manager::accounts::RetryPendingRebalanceRaydium {
            user: key(1),
            protocol_config: key(2),
            vault: key(3),
            mint_a: key(4),
            mint_b: key(5),
            vault_token_a: key(6),
            vault_token_b: key(7),
            pool_state: key(8),
            personal_position: key(9),
            position_nft_mint: key(10),
            position_nft_account: key(11),
            token_vault_0: key(12),
            token_vault_1: key(13),
            tick_array_lower: key(14),
            tick_array_upper: key(15),
            tick_array_bitmap_extension: key(16),
            token_program: key(17),
            token_2022_program: key(18),
            clmm_program: key(19),
            associated_token_program: key(20),
            system_program: key(21),
            rent: key(22),
        };
        let expected = vec![
            AccountMeta::new(key(1), true),
            AccountMeta::new_readonly(key(2), false),
            AccountMeta::new(key(3), false),
            AccountMeta::new_readonly(key(4), false),
            AccountMeta::new_readonly(key(5), false),
            AccountMeta::new(key(6), false),
            AccountMeta::new(key(7), false),
            AccountMeta::new(key(8), false),
            AccountMeta::new(key(9), false),
            AccountMeta::new(key(10), true),
            AccountMeta::new(key(11), false),
            AccountMeta::new(key(12), false),
            AccountMeta::new(key(13), false),
            AccountMeta::new(key(14), false),
            AccountMeta::new(key(15), false),
            AccountMeta::new(key(16), false),
            AccountMeta::new_readonly(key(17), false),
            AccountMeta::new_readonly(key(18), false),
            AccountMeta::new_readonly(key(19), false),
            AccountMeta::new_readonly(key(20), false),
            AccountMeta::new_readonly(key(21), false),
            AccountMeta::new_readonly(key(22), false),
        ];
        assert_eq!(accounts.to_account_metas(None), expected);
    }
}

mod state_machine_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use crate::common::vault_fixtures::{idle_vault, vault_with_position};
    use aargau_manager::errors::AargauError;
    use aargau_manager::instructions::{
        record_pending_rebalance, record_rebalanced_position, require_rebalance_retryable,
    };
    use aargau_manager::state::{PendingRebalance, Protocol, VaultAccount};
    use anchor_lang::prelude::Pubkey;

    fn assert_error<T: std::fmt::Debug>(result: anchor_lang::Result<T>, expected: AargauError) {
        assert_eq!(err_code(&result.unwrap_err()), aargau_err_code(expected));
    }

    /// Vault state right after a successful `start_rebalance_raydium`.
    fn pending_vault(tick_lower: i32, tick_upper: i32) -> VaultAccount {
        let mut vault = vault_with_position(Protocol::Raydium, -60, 60);
        record_pending_rebalance(&mut vault, tick_lower, tick_upper, 1_700_000_000);
        vault
    }

    #[test]
    fn range_comes_only_from_the_pending_state() {
        let pending = require_rebalance_retryable(&pending_vault(-240, 480)).unwrap();
        assert_eq!(pending.new_tick_lower, -240);
        assert_eq!(pending.new_tick_upper, 480);
        assert_eq!(pending.initiated_at, 1_700_000_000);
    }

    #[test]
    fn no_pending_rebalance_is_rejected() {
        assert_error(
            require_rebalance_retryable(&idle_vault(Protocol::Raydium)),
            AargauError::NoPendingRebalance,
        );
        assert_error(
            require_rebalance_retryable(&vault_with_position(Protocol::Raydium, -60, 60)),
            AargauError::NoPendingRebalance,
        );
    }

    #[test]
    fn active_position_is_rejected() {
        let mut vault = pending_vault(-120, 120);
        vault.position_address = Some(Pubkey::new_unique());
        assert_error(
            require_rebalance_retryable(&vault),
            AargauError::VaultHasActivePosition,
        );
    }

    #[test]
    fn other_protocols_are_rejected_even_with_a_pending_rebalance() {
        for protocol in [Protocol::Orca, Protocol::Meteora] {
            let mut vault = pending_vault(-120, 120);
            vault.protocol = protocol;
            assert_error(
                require_rebalance_retryable(&vault),
                AargauError::InvalidPool,
            );
        }
    }

    #[test]
    fn success_records_the_new_position_and_clears_pending() {
        let mut vault = pending_vault(-120, 360);
        let pending = require_rebalance_retryable(&vault).unwrap();
        let position = Pubkey::new_unique();
        let nft_mint = Pubkey::new_unique();
        record_rebalanced_position(
            &mut vault,
            position,
            nft_mint,
            pending.new_tick_lower,
            pending.new_tick_upper,
        );

        assert_eq!(vault.position_address, Some(position));
        assert_eq!(vault.position_mint, Some(nft_mint));
        assert_eq!(vault.position_range_lower, Some(-120));
        assert_eq!(vault.position_range_upper, Some(360));
        assert!(vault.pending_rebalance.is_none());
    }

    #[test]
    fn repeated_retry_after_success_fails_with_no_pending_rebalance() {
        let mut vault = pending_vault(-120, 120);
        let pending = require_rebalance_retryable(&vault).unwrap();
        record_rebalanced_position(
            &mut vault,
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            pending.new_tick_lower,
            pending.new_tick_upper,
        );
        assert_error(
            require_rebalance_retryable(&vault),
            AargauError::NoPendingRebalance,
        );
    }

    /// A failed attempt reverts atomically and writes nothing, so the vault
    /// stays retryable with the same stored range, whatever the retry count.
    #[test]
    fn failed_attempt_leaves_the_vault_retryable_on_the_same_range() {
        let mut vault = pending_vault(-120, 120);
        let first = require_rebalance_retryable(&vault).unwrap();

        vault.pending_rebalance = Some(PendingRebalance {
            retry_count: u8::MAX,
            last_failure_reason: 1,
            ..first
        });
        let second = require_rebalance_retryable(&vault).unwrap();
        assert_eq!(second.new_tick_lower, first.new_tick_lower);
        assert_eq!(second.new_tick_upper, first.new_tick_upper);
    }

    #[test]
    fn cancelled_rebalance_cannot_be_retried() {
        let mut vault = pending_vault(-120, 120);
        vault.pending_rebalance = None;
        assert_error(
            require_rebalance_retryable(&vault),
            AargauError::NoPendingRebalance,
        );
    }
}

/// Both legs composed: start → retry, start → cancel, and the guarantee that
/// a range the start accepts is one the retry can open.
mod two_phase_cycle_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use crate::common::vault_fixtures::vault_with_position;
    use aargau_manager::constants::{RAYDIUM_MAX_TICK, RAYDIUM_MIN_TICK};
    use aargau_manager::errors::AargauError;
    use aargau_manager::instructions::{
        record_pending_rebalance, record_rebalanced_position, require_rebalance_retryable,
        require_rebalance_startable,
    };
    use aargau_manager::state::Protocol;
    use aargau_manager::utils::raydium::position_guards::require_openable_tick_range;
    use anchor_lang::prelude::Pubkey;

    #[test]
    fn start_then_retry_success_then_retry_again_fails_with_no_pending() {
        let mut vault = vault_with_position(Protocol::Raydium, -60, 60);
        require_rebalance_startable(&vault).unwrap();
        record_pending_rebalance(&mut vault, -120, 120, 1);

        let pending = require_rebalance_retryable(&vault).unwrap();
        record_rebalanced_position(
            &mut vault,
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            pending.new_tick_lower,
            pending.new_tick_upper,
        );

        assert_eq!(
            err_code(&require_rebalance_retryable(&vault).unwrap_err()),
            aargau_err_code(AargauError::NoPendingRebalance)
        );
        // The rebalanced vault can start the next rebalance.
        assert!(require_rebalance_startable(&vault).is_ok());
    }

    #[test]
    fn start_then_cancel_leaves_an_idle_vault_that_cannot_retry() {
        let mut vault = vault_with_position(Protocol::Raydium, -60, 60);
        record_pending_rebalance(&mut vault, -120, 120, 1);
        // `cancel_pending_rebalance` only clears the pending state.
        vault.pending_rebalance = None;

        assert_eq!(
            err_code(&require_rebalance_retryable(&vault).unwrap_err()),
            aargau_err_code(AargauError::NoPendingRebalance)
        );
        assert_eq!(vault.position_address, None);
        assert!(require_rebalance_startable(&vault).is_ok());
    }

    #[test]
    fn every_range_the_start_accepts_is_openable_by_the_retry() {
        let cases: [(i32, i32, u16); 5] = [
            (-60, 60, 1),
            (-600, 600, 10),
            (RAYDIUM_MIN_TICK, RAYDIUM_MAX_TICK, 1),
            (-443_630, 443_630, 10),
            (-65_535, 65_535, u16::MAX),
        ];
        for (lower, upper, spacing) in cases {
            require_openable_tick_range(lower, upper, spacing).unwrap();

            let mut vault = vault_with_position(Protocol::Raydium, -1, 1);
            record_pending_rebalance(&mut vault, lower, upper, 0);
            let pending = require_rebalance_retryable(&vault).unwrap();
            assert!(require_openable_tick_range(
                pending.new_tick_lower,
                pending.new_tick_upper,
                spacing
            )
            .is_ok());
        }
    }
}

mod error_contract_tests {
    use aargau_manager::errors::AargauError;

    /// Discriminants off-chain clients map for this instruction.
    #[test]
    fn error_codes_are_stable() {
        assert_eq!(AargauError::SlippageExceeded as u32, 1002);
        assert_eq!(AargauError::InsufficientFunds as u32, 1006);
        assert_eq!(AargauError::InvalidActionPayload as u32, 1007);
        assert_eq!(AargauError::InvalidPool as u32, 1200);
        assert_eq!(AargauError::InvalidVaultPda as u32, 1300);
        assert_eq!(AargauError::VaultHasActivePosition as u32, 1301);
        assert_eq!(AargauError::NoPendingRebalance as u32, 1304);
        assert_eq!(AargauError::ProgramPaused as u32, 1400);
    }
}
