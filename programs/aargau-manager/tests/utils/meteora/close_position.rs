//! Tests for `src/utils/meteora/close_position.rs` — pure byte validation
//! of the `close_position_if_empty` instruction wire format.

mod build_close_position_if_empty_instruction_data_tests {
    use aargau_manager::constants::METEORA_CLOSE_POSITION_IF_EMPTY_DISCRIMINATOR;
    use aargau_manager::utils::meteora::close_position::build_close_position_if_empty_instruction_data;
    use anchor_lang::prelude::Pubkey;

    fn fixed_pubkey(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn build() -> (
        Vec<u8>,
        Vec<anchor_lang::solana_program::instruction::AccountMeta>,
    ) {
        build_close_position_if_empty_instruction_data(
            &fixed_pubkey(0x11), // position
            &fixed_pubkey(0x12), // vault
            &fixed_pubkey(0x13), // rent_receiver
            &fixed_pubkey(0x14), // event_authority
            &fixed_pubkey(0x15), // dlmm_program
        )
    }

    #[test]
    fn data_is_only_the_discriminator() {
        let (data, _) = build();
        assert_eq!(data.len(), 8);
        assert_eq!(&data[..], &METEORA_CLOSE_POSITION_IF_EMPTY_DISCRIMINATOR);
    }

    #[test]
    fn discriminator_matches_expected_value() {
        // Hard-coded as a regression check against the Meteora DLMM IDL.
        assert_eq!(
            METEORA_CLOSE_POSITION_IF_EMPTY_DISCRIMINATOR,
            [59, 124, 212, 118, 91, 152, 110, 157],
        );
    }

    #[test]
    fn account_meta_order_matches_meteora_layout() {
        // Per module doc: 0=position, 1=sender (vault PDA),
        // 2=rent_receiver, 3=event_authority, 4=dlmm_program.
        let (_, metas) = build();
        assert_eq!(metas.len(), 5);
        assert_eq!(metas[0].pubkey, fixed_pubkey(0x11));
        assert_eq!(metas[1].pubkey, fixed_pubkey(0x12));
        assert_eq!(metas[2].pubkey, fixed_pubkey(0x13));
        assert_eq!(metas[3].pubkey, fixed_pubkey(0x14));
        assert_eq!(metas[4].pubkey, fixed_pubkey(0x15));
    }

    #[test]
    fn only_vault_signs_position_and_rent_receiver_are_writable() {
        let (_, metas) = build();
        assert!(!metas[0].is_signer);
        assert!(metas[0].is_writable, "position must be writable");
        assert!(metas[1].is_signer, "vault PDA must sign");
        assert!(metas[1].is_writable, "vault must be writable");
        assert!(!metas[2].is_signer);
        assert!(metas[2].is_writable, "rent_receiver must be writable");
        assert!(!metas[3].is_signer);
        assert!(!metas[3].is_writable, "event_authority readonly");
        assert!(!metas[4].is_signer);
        assert!(!metas[4].is_writable, "dlmm_program readonly");
    }
}
