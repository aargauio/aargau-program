//! Tests for `src/utils/raydium/reward_accounts.rs` — the reward triples
//! appended to `decrease_liquidity_v2`.
//!
//! Raydium only checks that a reward recipient has the right mint, so these
//! bindings are what keeps rewards inside the vault.

mod reward_binding_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::constants::{SPL_TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID};
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::raydium::pool_state_view::{PoolStateView, RaydiumRewardSlot};
    use aargau_manager::utils::raydium::reward_accounts::{
        bind_reward_transfer_accounts, expected_reward_account_count,
        require_reward_transfer_bound, RewardTransferKeys,
    };
    use anchor_lang::prelude::{AccountInfo, Pubkey};
    use anchor_spl::associated_token::get_associated_token_address_with_program_id;

    fn fixed(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn vault() -> Pubkey {
        fixed(0x0A)
    }

    fn slot(mint: u8, vault: u8) -> RaydiumRewardSlot {
        RaydiumRewardSlot {
            token_mint: fixed(mint),
            token_vault: fixed(vault),
        }
    }

    fn view_with_rewards(rewards: [RaydiumRewardSlot; 3]) -> PoolStateView {
        PoolStateView {
            token_mint_0: fixed(0xa1),
            token_mint_1: fixed(0xb1),
            token_vault_0: fixed(0xa2),
            token_vault_1: fixed(0xb2),
            tick_spacing: 1,
            rewards,
        }
    }

    fn vault_ata(mint: &Pubkey, token_program: &Pubkey) -> Pubkey {
        get_associated_token_address_with_program_id(&vault(), mint, token_program)
    }

    fn keys_for(slot: &RaydiumRewardSlot, token_program: &Pubkey) -> RewardTransferKeys {
        RewardTransferKeys {
            reward_vault: slot.token_vault,
            recipient: vault_ata(&slot.token_mint, token_program),
            reward_mint: slot.token_mint,
        }
    }

    // --- pure per-slot binding ------------------------------------------------

    #[test]
    fn accepts_vault_ata_under_spl_token() {
        let reward = slot(0xc1, 0xc2);
        let keys = keys_for(&reward, &SPL_TOKEN_PROGRAM_ID);
        assert!(
            require_reward_transfer_bound(&reward, &keys, &vault(), &SPL_TOKEN_PROGRAM_ID).is_ok()
        );
    }

    #[test]
    fn accepts_vault_ata_under_token_2022() {
        let reward = slot(0xc1, 0xc2);
        let keys = keys_for(&reward, &TOKEN_2022_PROGRAM_ID);
        assert!(
            require_reward_transfer_bound(&reward, &keys, &vault(), &TOKEN_2022_PROGRAM_ID).is_ok()
        );
    }

    #[test]
    fn reward_mint_equal_to_pair_mint_uses_the_pair_ata() {
        // A reward in mint 0 lands in the vault's mint-0 ATA — the same
        // account as the principal recipient. Still a vault-owned ATA.
        let reward = slot(0xa1, 0xc2);
        let keys = keys_for(&reward, &SPL_TOKEN_PROGRAM_ID);
        assert_eq!(
            keys.recipient,
            vault_ata(&fixed(0xa1), &SPL_TOKEN_PROGRAM_ID)
        );
        assert!(
            require_reward_transfer_bound(&reward, &keys, &vault(), &SPL_TOKEN_PROGRAM_ID).is_ok()
        );
    }

    #[test]
    fn rejects_foreign_reward_vault() {
        let reward = slot(0xc1, 0xc2);
        let mut keys = keys_for(&reward, &SPL_TOKEN_PROGRAM_ID);
        keys.reward_vault = fixed(0xFF);
        let err = require_reward_transfer_bound(&reward, &keys, &vault(), &SPL_TOKEN_PROGRAM_ID)
            .unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidPoolReserve)
        );
    }

    #[test]
    fn rejects_foreign_reward_mint() {
        let reward = slot(0xc1, 0xc2);
        let mut keys = keys_for(&reward, &SPL_TOKEN_PROGRAM_ID);
        keys.reward_mint = fixed(0xFF);
        let err = require_reward_transfer_bound(&reward, &keys, &vault(), &SPL_TOKEN_PROGRAM_ID)
            .unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidPoolMint)
        );
    }

    #[test]
    fn rejects_attacker_recipient() {
        // Raydium would accept any account of the right mint here.
        let reward = slot(0xc1, 0xc2);
        let mut keys = keys_for(&reward, &SPL_TOKEN_PROGRAM_ID);
        keys.recipient = fixed(0xAA);
        let err = require_reward_transfer_bound(&reward, &keys, &vault(), &SPL_TOKEN_PROGRAM_ID)
            .unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidRewardOwner)
        );
    }

    #[test]
    fn rejects_non_token_program_owner() {
        let reward = slot(0xc1, 0xc2);
        let fake_program = fixed(0xEE);
        let keys = keys_for(&reward, &fake_program);
        let err =
            require_reward_transfer_bound(&reward, &keys, &vault(), &fake_program).unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidRewardOwner)
        );
    }

    #[test]
    fn rejects_ata_derived_under_the_other_token_program() {
        let reward = slot(0xc1, 0xc2);
        let keys = keys_for(&reward, &SPL_TOKEN_PROGRAM_ID);
        let err = require_reward_transfer_bound(&reward, &keys, &vault(), &TOKEN_2022_PROGRAM_ID)
            .unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidRewardOwner)
        );
    }

    // --- remaining-accounts splitting ---------------------------------------

    #[test]
    fn expected_count_is_three_per_initialized_slot() {
        let none = view_with_rewards([RaydiumRewardSlot::default(); 3]);
        assert_eq!(expected_reward_account_count(&none).unwrap(), 0);
        let two = view_with_rewards([
            slot(0xc1, 0xc2),
            slot(0xd1, 0xd2),
            RaydiumRewardSlot::default(),
        ]);
        assert_eq!(expected_reward_account_count(&two).unwrap(), 6);
    }

    #[test]
    fn no_initialized_rewards_and_no_accounts_is_ok() {
        let view = view_with_rewards([RaydiumRewardSlot::default(); 3]);
        let bound = bind_reward_transfer_accounts(&view, &vault(), &[]).unwrap();
        assert!(bound.is_empty());
    }

    #[test]
    fn missing_triple_is_rejected() {
        let view = view_with_rewards([
            slot(0xc1, 0xc2),
            RaydiumRewardSlot::default(),
            RaydiumRewardSlot::default(),
        ]);
        let err = bind_reward_transfer_accounts(&view, &vault(), &[])
            .err()
            .unwrap();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidActionPayload)
        );
    }

    /// Owned backing storage for one `AccountInfo`.
    struct TestAccount {
        key: Pubkey,
        owner: Pubkey,
        lamports: u64,
        data: Vec<u8>,
    }

    impl TestAccount {
        fn new(key: Pubkey, owner: Pubkey) -> Self {
            Self {
                key,
                owner,
                lamports: 0,
                data: Vec::new(),
            }
        }

        fn info(&mut self) -> AccountInfo<'_> {
            AccountInfo::new(
                &self.key,
                false,
                true,
                &mut self.lamports,
                &mut self.data,
                &self.owner,
                false,
            )
        }
    }

    fn triple_storage(reward: &RaydiumRewardSlot, mint_owner: Pubkey) -> [TestAccount; 3] {
        [
            TestAccount::new(reward.token_vault, SPL_TOKEN_PROGRAM_ID),
            TestAccount::new(vault_ata(&reward.token_mint, &mint_owner), mint_owner),
            TestAccount::new(reward.token_mint, mint_owner),
        ]
    }

    #[test]
    fn binds_two_triples_in_slot_order_with_per_mint_programs() {
        let first = slot(0xc1, 0xc2);
        let second = slot(0xd1, 0xd2);
        let view = view_with_rewards([first, second, RaydiumRewardSlot::default()]);

        let [mut a0, mut a1, mut a2] = triple_storage(&first, SPL_TOKEN_PROGRAM_ID);
        let [mut b0, mut b1, mut b2] = triple_storage(&second, TOKEN_2022_PROGRAM_ID);
        let infos = [
            a0.info(),
            a1.info(),
            a2.info(),
            b0.info(),
            b1.info(),
            b2.info(),
        ];

        let bound = bind_reward_transfer_accounts(&view, &vault(), &infos).unwrap();
        assert_eq!(bound.len(), 2);
        assert_eq!(bound[0].keys(), keys_for(&first, &SPL_TOKEN_PROGRAM_ID));
        assert_eq!(bound[1].keys(), keys_for(&second, &TOKEN_2022_PROGRAM_ID));
    }

    #[test]
    fn triples_out_of_slot_order_are_rejected() {
        let first = slot(0xc1, 0xc2);
        let second = slot(0xd1, 0xd2);
        let view = view_with_rewards([first, second, RaydiumRewardSlot::default()]);

        let [mut a0, mut a1, mut a2] = triple_storage(&first, SPL_TOKEN_PROGRAM_ID);
        let [mut b0, mut b1, mut b2] = triple_storage(&second, SPL_TOKEN_PROGRAM_ID);
        let infos = [
            b0.info(),
            b1.info(),
            b2.info(),
            a0.info(),
            a1.info(),
            a2.info(),
        ];

        let err = bind_reward_transfer_accounts(&view, &vault(), &infos)
            .err()
            .unwrap();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidPoolReserve)
        );
    }

    #[test]
    fn extra_trailing_account_is_rejected() {
        let first = slot(0xc1, 0xc2);
        let view = view_with_rewards([
            first,
            RaydiumRewardSlot::default(),
            RaydiumRewardSlot::default(),
        ]);
        let [mut a0, mut a1, mut a2] = triple_storage(&first, SPL_TOKEN_PROGRAM_ID);
        let mut extra = TestAccount::new(fixed(0x99), SPL_TOKEN_PROGRAM_ID);
        let infos = [a0.info(), a1.info(), a2.info(), extra.info()];

        let err = bind_reward_transfer_accounts(&view, &vault(), &infos)
            .err()
            .unwrap();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidActionPayload)
        );
    }

    #[test]
    fn recipient_bound_under_mint_owner_program() {
        // Recipient derived under Token-2022 while the mint is SPL-owned.
        let first = slot(0xc1, 0xc2);
        let view = view_with_rewards([
            first,
            RaydiumRewardSlot::default(),
            RaydiumRewardSlot::default(),
        ]);
        let mut reward_vault = TestAccount::new(first.token_vault, SPL_TOKEN_PROGRAM_ID);
        let mut recipient = TestAccount::new(
            vault_ata(&first.token_mint, &TOKEN_2022_PROGRAM_ID),
            TOKEN_2022_PROGRAM_ID,
        );
        let mut mint = TestAccount::new(first.token_mint, SPL_TOKEN_PROGRAM_ID);
        let infos = [reward_vault.info(), recipient.info(), mint.info()];

        let err = bind_reward_transfer_accounts(&view, &vault(), &infos)
            .err()
            .unwrap();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidRewardOwner)
        );
    }
}
