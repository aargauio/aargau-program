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
