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

mod reward_owner_bind_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::constants::TOKEN_2022_PROGRAM_ID;
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::vault_ops::require_reward_owner_is_vault_ata;
    use anchor_lang::prelude::Pubkey;

    fn fixed(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn spl_token_program() -> Pubkey {
        "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
            .parse()
            .unwrap()
    }

    /// The vault's ATA for the reward mint (SPL classic) is accepted.
    #[test]
    fn accepts_vault_ata_under_spl_token() {
        let vault = fixed(0x21);
        let reward_mint = fixed(0x22);
        let token_program = spl_token_program();
        let expected = anchor_spl::associated_token::get_associated_token_address_with_program_id(
            &vault,
            &reward_mint,
            &token_program,
        );
        assert!(
            require_reward_owner_is_vault_ata(&expected, &vault, &reward_mint, &token_program)
                .is_ok()
        );
    }

    /// The same derivation under the Token-2022 program yields a DIFFERENT ATA
    /// and the bind accepts the Token-2022 one — pinning the program-id-aware
    /// derivation for mixed pools.
    #[test]
    fn accepts_vault_ata_under_token_2022_and_differs_from_spl() {
        let vault = fixed(0x31);
        let reward_mint = fixed(0x32);
        let spl = spl_token_program();

        let ata_spl = anchor_spl::associated_token::get_associated_token_address_with_program_id(
            &vault,
            &reward_mint,
            &spl,
        );
        let ata_t22 = anchor_spl::associated_token::get_associated_token_address_with_program_id(
            &vault,
            &reward_mint,
            &TOKEN_2022_PROGRAM_ID,
        );
        // The token program is part of the ATA derivation, so the two differ.
        assert_ne!(ata_spl, ata_t22);

        assert!(require_reward_owner_is_vault_ata(
            &ata_t22,
            &vault,
            &reward_mint,
            &TOKEN_2022_PROGRAM_ID,
        )
        .is_ok());
    }

    /// An arbitrary destination (not the vault's ATA) is rejected — this is the
    /// custody gap the bind closes for the `remaining_accounts` reward path.
    #[test]
    fn rejects_arbitrary_reward_owner() {
        let vault = fixed(0x41);
        let reward_mint = fixed(0x42);
        let token_program = spl_token_program();
        let attacker_owned = fixed(0xAA);

        let err = require_reward_owner_is_vault_ata(
            &attacker_owned,
            &vault,
            &reward_mint,
            &token_program,
        )
        .unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidRewardOwner),
        );
    }

    /// The vault's ATA for the wrong mint is also rejected — binding is per
    /// (vault, mint, program), not just per vault.
    #[test]
    fn rejects_vault_ata_of_wrong_mint() {
        let vault = fixed(0x51);
        let reward_mint = fixed(0x52);
        let other_mint = fixed(0x53);
        let token_program = spl_token_program();

        let wrong_mint_ata =
            anchor_spl::associated_token::get_associated_token_address_with_program_id(
                &vault,
                &other_mint,
                &token_program,
            );
        let err = require_reward_owner_is_vault_ata(
            &wrong_mint_ata,
            &vault,
            &reward_mint,
            &token_program,
        )
        .unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidRewardOwner),
        );
    }

    /// The correctly-derived ATA but under the WRONG token program is rejected
    /// — covers the mixed-pool case where the program id is mis-supplied.
    #[test]
    fn rejects_correct_mint_wrong_token_program() {
        let vault = fixed(0x61);
        let reward_mint = fixed(0x62);
        let spl = spl_token_program();

        // Caller claims Token-2022 but passes the SPL-derived ATA.
        let spl_ata = anchor_spl::associated_token::get_associated_token_address_with_program_id(
            &vault,
            &reward_mint,
            &spl,
        );
        let err = require_reward_owner_is_vault_ata(
            &spl_ata,
            &vault,
            &reward_mint,
            &TOKEN_2022_PROGRAM_ID,
        )
        .unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidRewardOwner),
        );
    }
}

mod excess_lamports_tests {
    use aargau_manager::utils::vault_ops::excess_lamports_above_rent;

    #[test]
    fn zero_when_at_or_below_minimum() {
        assert_eq!(excess_lamports_above_rent(0, 0), 0);
        assert_eq!(excess_lamports_above_rent(1_000, 1_000), 0);
        assert_eq!(excess_lamports_above_rent(999, 1_000), 0);
        assert_eq!(excess_lamports_above_rent(0, u64::MAX), 0);
    }

    #[test]
    fn excess_above_minimum() {
        assert_eq!(excess_lamports_above_rent(1_001, 1_000), 1);
        assert_eq!(excess_lamports_above_rent(u64::MAX, 0), u64::MAX);
        assert_eq!(excess_lamports_above_rent(u64::MAX, 1), u64::MAX - 1);
    }
}

mod sweep_excess_vault_lamports_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::errors::AargauError;
    use aargau_manager::state::{AutoRebalanceStrategy, Protocol, VaultAccount};
    use aargau_manager::utils::vault_ops::sweep_excess_vault_lamports;
    use anchor_lang::prelude::{Account, AccountInfo, Pubkey, Rent};
    use anchor_lang::AccountSerialize;

    fn fixed(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn user_key() -> Pubkey {
        fixed(0x0B)
    }

    fn system_program() -> Pubkey {
        Pubkey::default()
    }

    /// Serialized `VaultAccount` at its full allocated size.
    fn vault_data() -> Vec<u8> {
        let vault = VaultAccount {
            user_authority: user_key(),
            pool_address: fixed(0x0C),
            protocol: Protocol::Raydium,
            position_address: None,
            position_mint: None,
            position_range_lower: None,
            position_range_upper: None,
            uses_token_2022: false,
            token_a_transfer_fee_bps: 0,
            token_a_maximum_fee: 0,
            token_b_transfer_fee_bps: 0,
            token_b_maximum_fee: 0,
            reward_token_0_transfer_fee_bps: 0,
            reward_token_0_maximum_fee: 0,
            reward_token_1_transfer_fee_bps: 0,
            reward_token_1_maximum_fee: 0,
            reward_token_2_transfer_fee_bps: 0,
            reward_token_2_maximum_fee: 0,
            allowed_ops: 0,
            strategy: AutoRebalanceStrategy::default(),
            last_rebalance_at: 0,
            rebalances_today: 0,
            gas_spent_today_usd_cents: 0,
            last_day_reset: 0,
            entry_value_usd: 0,
            pending_rebalance: None,
            created_at: 0,
            bump: 255,
            _padding: [0u8; 8],
        };
        let mut data = Vec::new();
        vault.try_serialize(&mut data).unwrap();
        data.resize(VaultAccount::LEN, 0);
        data
    }

    fn rent_exempt_minimum() -> u64 {
        Rent::default().minimum_balance(VaultAccount::LEN)
    }

    /// Runs the sweep against a vault holding `vault_lamports` and a user
    /// account `user` holding `user_lamports`; returns (result, vault after,
    /// user after).
    fn run_sweep(
        vault_lamports: u64,
        user: Pubkey,
        user_lamports: u64,
    ) -> (anchor_lang::Result<u64>, u64, u64) {
        let program_id = aargau_manager::ID;
        let vault_key = fixed(0x0A);
        let mut vault_balance = vault_lamports;
        let mut data = vault_data();
        let vault_info = AccountInfo::new(
            &vault_key,
            false,
            true,
            &mut vault_balance,
            &mut data,
            &program_id,
            false,
        );
        let vault = Account::<VaultAccount>::try_from(&vault_info).unwrap();

        let system = system_program();
        let mut user_balance = user_lamports;
        let mut user_data: Vec<u8> = Vec::new();
        let user_info = AccountInfo::new(
            &user,
            true,
            true,
            &mut user_balance,
            &mut user_data,
            &system,
            false,
        );

        let result = sweep_excess_vault_lamports(&vault, &user_info, &Rent::default());
        let vault_after = vault_info.lamports();
        let user_after = user_info.lamports();
        (result, vault_after, user_after)
    }

    #[test]
    fn moves_close_rent_refund_to_the_user() {
        // e.g. personal_position + NFT account + NFT mint rent refunded by
        // Raydium `close_position` to the vault PDA.
        let refund = 5_000_000;
        let min = rent_exempt_minimum();
        let (result, vault_after, user_after) = run_sweep(min + refund, user_key(), 1_000);
        assert_eq!(result.unwrap(), refund);
        assert_eq!(vault_after, min);
        assert_eq!(user_after, 1_000 + refund);
    }

    #[test]
    fn single_lamport_excess_is_swept() {
        let min = rent_exempt_minimum();
        let (result, vault_after, user_after) = run_sweep(min + 1, user_key(), 0);
        assert_eq!(result.unwrap(), 1);
        assert_eq!(vault_after, min);
        assert_eq!(user_after, 1);
    }

    #[test]
    fn no_excess_is_a_no_op() {
        let min = rent_exempt_minimum();
        let (result, vault_after, user_after) = run_sweep(min, user_key(), 7);
        assert_eq!(result.unwrap(), 0);
        assert_eq!(vault_after, min);
        assert_eq!(user_after, 7);
    }

    #[test]
    fn below_minimum_is_a_no_op() {
        let (result, vault_after, user_after) = run_sweep(1, user_key(), 7);
        assert_eq!(result.unwrap(), 0);
        assert_eq!(vault_after, 1);
        assert_eq!(user_after, 7);
    }

    #[test]
    fn rejects_destination_other_than_user_authority() {
        let min = rent_exempt_minimum();
        let attacker = fixed(0xAA);
        let (result, vault_after, user_after) = run_sweep(min + 10, attacker, 0);
        assert_eq!(
            err_code(&result.unwrap_err()),
            aargau_err_code(AargauError::UnauthorizedUser)
        );
        assert_eq!(vault_after, min + 10, "vault must be untouched");
        assert_eq!(user_after, 0);
    }

    #[test]
    fn user_balance_overflow_is_an_error_and_moves_nothing() {
        let min = rent_exempt_minimum();
        let (result, vault_after, user_after) = run_sweep(min + 1, user_key(), u64::MAX);
        assert_eq!(
            err_code(&result.unwrap_err()),
            aargau_err_code(AargauError::Overflow)
        );
        assert_eq!(vault_after, min + 1);
        assert_eq!(user_after, u64::MAX);
    }
}
