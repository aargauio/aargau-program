//! Tests for `src/utils/meteora/open_position.rs` — pure byte validation of
//! the `initialize_position` instruction wire format.
//!
//! The `invoke_signed` path is exercised by the mollusk-backed integration
//! tests under `tests/instructions/keeper/execute_action_meteora.rs`. Here
//! we lock the byte layout that the helper would emit, so any accidental
//! reorder of account metas or change in the Borsh body fails fast.

mod build_initialize_position_instruction_data_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::constants::METEORA_INITIALIZE_POSITION_DISCRIMINATOR;
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::meteora::open_position::build_initialize_position_instruction_data;
    use anchor_lang::prelude::Pubkey;

    fn fixed_pubkey(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn invoke(
        lower: i32,
        upper: i32,
    ) -> anchor_lang::Result<(
        Vec<u8>,
        Vec<anchor_lang::solana_program::instruction::AccountMeta>,
    )> {
        build_initialize_position_instruction_data(
            &fixed_pubkey(0x01), // user_authority
            &fixed_pubkey(0x02), // position
            &fixed_pubkey(0x03), // lb_pair
            &fixed_pubkey(0x04), // vault
            &fixed_pubkey(0x05), // system_program
            &fixed_pubkey(0x06), // rent_sysvar
            &fixed_pubkey(0x07), // event_authority
            &fixed_pubkey(0x08), // dlmm_program
            lower,
            upper,
        )
    }

    #[test]
    fn data_starts_with_initialize_position_discriminator() {
        let (data, _) = invoke(-5, 5).unwrap();
        assert_eq!(&data[..8], &METEORA_INITIALIZE_POSITION_DISCRIMINATOR);
    }

    #[test]
    fn encodes_lower_bin_id_le_after_discriminator() {
        let (data, _) = invoke(-7, 3).unwrap();
        let lower_bytes: [u8; 4] = data[8..12].try_into().unwrap();
        assert_eq!(i32::from_le_bytes(lower_bytes), -7);
    }

    #[test]
    fn width_is_upper_minus_lower_plus_one_inclusive() {
        // [-5, 5] inclusive = 11 bins. This locks the width semantics:
        // Meteora's `initialize_position` takes (lower_bin_id, width) and
        // every known wallet-mode builder uses `upper - lower + 1`.
        let (data, _) = invoke(-5, 5).unwrap();
        let width_bytes: [u8; 4] = data[12..16].try_into().unwrap();
        assert_eq!(i32::from_le_bytes(width_bytes), 11);
    }

    #[test]
    fn width_for_single_bin_range_equals_one() {
        let (data, _) = invoke(42, 42).unwrap();
        let width_bytes: [u8; 4] = data[12..16].try_into().unwrap();
        assert_eq!(i32::from_le_bytes(width_bytes), 1);
    }

    #[test]
    fn data_body_length_matches_disc_plus_two_i32s() {
        let (data, _) = invoke(0, 1).unwrap();
        assert_eq!(data.len(), 8 + 4 + 4);
    }

    #[test]
    fn rejects_upper_less_than_lower() {
        let err = invoke(5, -5).unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidActionPayload),
        );
    }

    #[test]
    fn account_meta_order_matches_meteora_layout() {
        // Slots fixed by the Meteora DLMM IDL (mirrored in the helper module doc):
        // 0=payer, 1=position, 2=lb_pair, 3=owner, 4=system, 5=rent,
        // 6=event_authority, 7=dlmm_program.
        let (_, metas) = invoke(0, 0).unwrap();
        assert_eq!(metas.len(), 8);
        assert_eq!(metas[0].pubkey, fixed_pubkey(0x01)); // user_authority
        assert_eq!(metas[1].pubkey, fixed_pubkey(0x02)); // position
        assert_eq!(metas[2].pubkey, fixed_pubkey(0x03)); // lb_pair
        assert_eq!(metas[3].pubkey, fixed_pubkey(0x04)); // vault (owner)
        assert_eq!(metas[4].pubkey, fixed_pubkey(0x05)); // system_program
        assert_eq!(metas[5].pubkey, fixed_pubkey(0x06)); // rent_sysvar
        assert_eq!(metas[6].pubkey, fixed_pubkey(0x07)); // event_authority
        assert_eq!(metas[7].pubkey, fixed_pubkey(0x08)); // dlmm_program
    }

    #[test]
    fn payer_position_and_vault_owner_are_signers() {
        let (_, metas) = invoke(0, 0).unwrap();
        assert!(metas[0].is_signer, "payer (slot 0) must sign");
        assert!(metas[1].is_signer, "position (slot 1) must sign");
        assert!(metas[3].is_signer, "vault PDA / owner (slot 3) must sign");
    }

    #[test]
    fn payer_and_position_are_writable_owner_is_readonly() {
        let (_, metas) = invoke(0, 0).unwrap();
        assert!(metas[0].is_writable, "payer (slot 0) must be writable");
        assert!(metas[1].is_writable, "position (slot 1) must be writable");
        assert!(
            !metas[3].is_writable,
            "vault owner (slot 3) must be readonly + signer"
        );
        for (idx, slot) in [(4, "system"), (5, "rent"), (6, "event_auth"), (7, "dlmm")] {
            assert!(
                !metas[idx].is_writable,
                "slot {idx} ({slot}) must be readonly",
            );
            assert!(!metas[idx].is_signer, "slot {idx} ({slot}) must not sign",);
        }
    }
}
