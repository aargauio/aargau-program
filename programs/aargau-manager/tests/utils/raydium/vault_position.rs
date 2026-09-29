//! Tests for `src/utils/raydium/vault_position.rs` that need no SVM runtime:
//! the mapping from the shared account bundle into each CPI struct.
//!
//! The wire order of every CPI is pinned by the builder tests; these pin the
//! step before it. Leg 0 must reach every `*_0` slot and leg 1 every `*_1`
//! slot: a swapped pair would send principal or fees to the wrong vault ATA
//! and break the pool-vault fee measurement. The CPIs themselves run only in
//! the replay against the Raydium `.so`.

mod bundle_mapping_tests {
    use aargau_manager::utils::raydium::vault_position::{
        OpenPositionFundingAccounts, PairLegAccounts, PositionPayoutAccounts,
        RaydiumPositionAccounts,
    };
    use anchor_lang::prelude::{AccountInfo, Pubkey};

    /// Test-only account with a distinct key; leaked so it lives for `'static`.
    fn account(byte: u8) -> AccountInfo<'static> {
        let key = Box::leak(Box::new(Pubkey::new_from_array([byte; 32])));
        let owner = Box::leak(Box::new(Pubkey::default()));
        let lamports = Box::leak(Box::new(0u64));
        let data: &'static mut [u8] = Box::leak(Vec::new().into_boxed_slice());
        AccountInfo::new(key, false, true, lamports, data, owner, false)
    }

    fn key(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn bundle() -> RaydiumPositionAccounts<'static> {
        RaydiumPositionAccounts {
            clmm_program: account(1),
            vault: account(2),
            pool_state: account(3),
            personal_position: account(4),
            position_nft_mint: account(5),
            position_nft_account: account(6),
            tick_array_lower: account(7),
            tick_array_upper: account(8),
            tick_array_bitmap_extension: account(9),
            token_program: account(10),
            token_program_2022: account(11),
            leg_0: PairLegAccounts {
                mint: account(20),
                decimals: 6,
                vault_ata: account(21),
                pool_vault: account(22),
            },
            leg_1: PairLegAccounts {
                mint: account(30),
                decimals: 9,
                vault_ata: account(31),
                pool_vault: account(32),
            },
        }
    }

    #[test]
    fn open_routes_each_leg_and_the_funding_accounts() {
        let funding = OpenPositionFundingAccounts {
            payer: account(40),
            rent: account(41),
            system_program: account(42),
            associated_token_program: account(43),
        };
        let cpi = bundle().open_cpi(&funding);
        assert_eq!(*cpi.clmm_program.key, key(1));
        assert_eq!(*cpi.payer.key, key(40));
        assert_eq!(*cpi.vault.key, key(2));
        assert_eq!(*cpi.position_nft_mint.key, key(5));
        assert_eq!(*cpi.position_nft_account.key, key(6));
        assert_eq!(*cpi.pool_state.key, key(3));
        assert_eq!(*cpi.tick_array_lower.key, key(7));
        assert_eq!(*cpi.tick_array_upper.key, key(8));
        assert_eq!(*cpi.personal_position.key, key(4));
        assert_eq!(*cpi.token_account_0.key, key(21));
        assert_eq!(*cpi.token_account_1.key, key(31));
        assert_eq!(*cpi.token_vault_0.key, key(22));
        assert_eq!(*cpi.token_vault_1.key, key(32));
        assert_eq!(*cpi.rent.key, key(41));
        assert_eq!(*cpi.system_program.key, key(42));
        assert_eq!(*cpi.token_program.key, key(10));
        assert_eq!(*cpi.associated_token_program.key, key(43));
        assert_eq!(*cpi.token_program_2022.key, key(11));
        assert_eq!(*cpi.vault_0_mint.key, key(20));
        assert_eq!(*cpi.vault_1_mint.key, key(30));
        assert_eq!(*cpi.tick_array_bitmap_extension.key, key(9));
    }

    #[test]
    fn increase_routes_each_leg_to_its_side() {
        let cpi = bundle().increase_cpi();
        assert_eq!(*cpi.clmm_program.key, key(1));
        assert_eq!(*cpi.vault.key, key(2));
        assert_eq!(*cpi.nft_account.key, key(6));
        assert_eq!(*cpi.pool_state.key, key(3));
        assert_eq!(*cpi.personal_position.key, key(4));
        assert_eq!(*cpi.tick_array_lower.key, key(7));
        assert_eq!(*cpi.tick_array_upper.key, key(8));
        assert_eq!(*cpi.vault_token_0.key, key(21));
        assert_eq!(*cpi.vault_token_1.key, key(31));
        assert_eq!(*cpi.token_vault_0.key, key(22));
        assert_eq!(*cpi.token_vault_1.key, key(32));
        assert_eq!(*cpi.token_program.key, key(10));
        assert_eq!(*cpi.token_program_2022.key, key(11));
        assert_eq!(*cpi.vault_0_mint.key, key(20));
        assert_eq!(*cpi.vault_1_mint.key, key(30));
        assert_eq!(*cpi.tick_array_bitmap_extension.key, key(9));
    }

    #[test]
    fn decrease_routes_each_leg_the_memo_and_the_reward_triples() {
        let rewards = [];
        let payout = PositionPayoutAccounts {
            memo_program: account(50),
            rewards: &rewards,
        };
        let cpi = bundle().decrease_cpi(&payout);
        assert_eq!(*cpi.clmm_program.key, key(1));
        assert_eq!(*cpi.vault.key, key(2));
        assert_eq!(*cpi.nft_account.key, key(6));
        assert_eq!(*cpi.personal_position.key, key(4));
        assert_eq!(*cpi.pool_state.key, key(3));
        assert_eq!(*cpi.token_vault_0.key, key(22));
        assert_eq!(*cpi.token_vault_1.key, key(32));
        assert_eq!(*cpi.tick_array_lower.key, key(7));
        assert_eq!(*cpi.tick_array_upper.key, key(8));
        assert_eq!(*cpi.vault_token_0.key, key(21));
        assert_eq!(*cpi.vault_token_1.key, key(31));
        assert_eq!(*cpi.token_program.key, key(10));
        assert_eq!(*cpi.token_program_2022.key, key(11));
        assert_eq!(*cpi.memo_program.key, key(50));
        assert_eq!(*cpi.vault_0_mint.key, key(20));
        assert_eq!(*cpi.vault_1_mint.key, key(30));
        assert_eq!(*cpi.tick_array_bitmap_extension.key, key(9));
        assert!(cpi.reward_accounts.is_empty());
    }

    #[test]
    fn close_routes_the_nft_accounts_and_the_pool_for_a_thaw() {
        let cpi = bundle().close_cpi(&account(60));
        assert_eq!(*cpi.clmm_program.key, key(1));
        assert_eq!(*cpi.vault.key, key(2));
        assert_eq!(*cpi.position_nft_mint.key, key(5));
        assert_eq!(*cpi.position_nft_account.key, key(6));
        assert_eq!(*cpi.personal_position.key, key(4));
        assert_eq!(*cpi.system_program.key, key(60));
        assert_eq!(*cpi.token_program_2022.key, key(11));
        assert_eq!(*cpi.pool_state.key, key(3));
    }
}
