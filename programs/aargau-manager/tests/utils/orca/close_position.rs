//! Wire-format tests for `src/utils/orca/close_position.rs`
//! (`close_position_with_token_extensions`).

mod discriminator_tests {
    use aargau_manager::constants::ORCA_CLOSE_POSITION_WITH_TOKEN_EXTENSIONS_DISCRIMINATOR;
    use sha2::{Digest, Sha256};

    fn anchor_disc(name: &str) -> [u8; 8] {
        let mut hasher = Sha256::new();
        hasher.update(format!("global:{name}").as_bytes());
        let mut out = [0u8; 8];
        out.copy_from_slice(&hasher.finalize()[..8]);
        out
    }

    #[test]
    fn close_position_discriminator_matches_sha256_and_backend() {
        assert_eq!(
            ORCA_CLOSE_POSITION_WITH_TOKEN_EXTENSIONS_DISCRIMINATOR,
            anchor_disc("close_position_with_token_extensions"),
        );
        assert_eq!(
            ORCA_CLOSE_POSITION_WITH_TOKEN_EXTENSIONS_DISCRIMINATOR,
            [1, 182, 135, 59, 155, 25, 99, 223],
        );
    }
}

mod build_close_position_tests {
    use aargau_manager::constants::ORCA_CLOSE_POSITION_WITH_TOKEN_EXTENSIONS_DISCRIMINATOR;
    use aargau_manager::utils::orca::close_position::build_close_position_instruction_data;
    use anchor_lang::prelude::Pubkey;

    fn fixed(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    #[test]
    fn body_is_discriminator_only() {
        let (data, _) = build_close_position_instruction_data(
            &fixed(0x01),
            &fixed(0x02),
            &fixed(0x03),
            &fixed(0x04),
            &fixed(0x05),
            &fixed(0x06),
        );
        assert_eq!(
            data,
            ORCA_CLOSE_POSITION_WITH_TOKEN_EXTENSIONS_DISCRIMINATOR
        );
    }

    #[test]
    fn account_meta_order_and_flags_match_on_chain_layout() {
        let (_, m) = build_close_position_instruction_data(
            &fixed(0x01), // position_authority (vault)
            &fixed(0x02), // receiver (user)
            &fixed(0x03), // position
            &fixed(0x04), // position_mint
            &fixed(0x05), // position_token_account
            &fixed(0x06), // token_2022_program
        );
        assert_eq!(m.len(), 6);
        assert_eq!(m[0].pubkey, fixed(0x01));
        assert_eq!(m[1].pubkey, fixed(0x02));
        assert_eq!(m[2].pubkey, fixed(0x03));
        assert_eq!(m[3].pubkey, fixed(0x04));
        assert_eq!(m[4].pubkey, fixed(0x05));
        assert_eq!(m[5].pubkey, fixed(0x06));

        // position_authority: readonly + signer (signs via seeds)
        assert!(m[0].is_signer && !m[0].is_writable);
        // receiver: writable (gets rent)
        assert!(!m[1].is_signer && m[1].is_writable);
        // position / position_mint / position_token_account: writable
        for idx in [2, 3, 4] {
            assert!(m[idx].is_writable && !m[idx].is_signer, "slot {idx}");
        }
        // token_2022_program: readonly
        assert!(!m[5].is_writable && !m[5].is_signer);
    }
}
