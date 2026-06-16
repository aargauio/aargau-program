//! Wire-format tests for `src/utils/orca/open_position.rs`.
//!
//! Pins the `open_position_with_token_extensions` Borsh body byte-for-byte and
//! the account-meta ordering/flags against the on-chain Whirlpools
//! `#[derive(Accounts)]`. Discriminators are re-derived via
//! `sha256("global:<name>")[..8]` and asserted against the constant, which was
//! cross-checked against the on-chain-verified backend bytes.

mod discriminator_tests {
    use aargau_manager::constants::ORCA_OPEN_POSITION_WITH_TOKEN_EXTENSIONS_DISCRIMINATOR;
    use sha2::{Digest, Sha256};

    /// Anchor instruction discriminator = `sha256("global:<name>")[..8]`.
    fn anchor_disc(name: &str) -> [u8; 8] {
        let mut hasher = Sha256::new();
        hasher.update(format!("global:{name}").as_bytes());
        let mut out = [0u8; 8];
        out.copy_from_slice(&hasher.finalize()[..8]);
        out
    }

    #[test]
    fn open_position_discriminator_matches_sha256_and_backend() {
        assert_eq!(
            ORCA_OPEN_POSITION_WITH_TOKEN_EXTENSIONS_DISCRIMINATOR,
            anchor_disc("open_position_with_token_extensions"),
        );
        // Cross-check against the on-chain-verified backend bytes.
        assert_eq!(
            ORCA_OPEN_POSITION_WITH_TOKEN_EXTENSIONS_DISCRIMINATOR,
            [212, 47, 95, 92, 114, 102, 131, 250],
        );
    }
}

mod build_open_position_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::constants::ORCA_OPEN_POSITION_WITH_TOKEN_EXTENSIONS_DISCRIMINATOR;
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::orca::open_position::build_open_position_instruction_data;
    use anchor_lang::prelude::Pubkey;
    use anchor_lang::solana_program::instruction::AccountMeta;

    fn fixed(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    #[allow(clippy::type_complexity)]
    fn build(
        lower: i32,
        upper: i32,
        with_meta: bool,
    ) -> anchor_lang::Result<(Vec<u8>, Vec<AccountMeta>)> {
        build_open_position_instruction_data(
            &fixed(0x01), // funder
            &fixed(0x02), // owner (vault)
            &fixed(0x03), // position
            &fixed(0x04), // position_mint
            &fixed(0x05), // position_token_account
            &fixed(0x06), // whirlpool
            &fixed(0x07), // token_2022_program
            &fixed(0x08), // system_program
            &fixed(0x09), // associated_token_program
            &fixed(0x0a), // metadata_update_auth
            lower,
            upper,
            with_meta,
        )
    }

    #[test]
    fn data_starts_with_discriminator() {
        let (data, _) = build(-100, 100, false).unwrap();
        assert_eq!(
            &data[..8],
            &ORCA_OPEN_POSITION_WITH_TOKEN_EXTENSIONS_DISCRIMINATOR
        );
    }

    #[test]
    fn encodes_ticks_le_and_metadata_flag() {
        let (data, _) = build(-7, 13, false).unwrap();
        assert_eq!(i32::from_le_bytes(data[8..12].try_into().unwrap()), -7);
        assert_eq!(i32::from_le_bytes(data[12..16].try_into().unwrap()), 13);
        assert_eq!(
            data[16], 0u8,
            "with_token_metadata_extension defaults false"
        );

        let (data_true, _) = build(-7, 13, true).unwrap();
        assert_eq!(data_true[16], 1u8);
    }

    #[test]
    fn body_length_is_disc_plus_two_i32s_plus_bool() {
        let (data, _) = build(0, 1, false).unwrap();
        assert_eq!(data.len(), 8 + 4 + 4 + 1);
    }

    #[test]
    fn rejects_upper_not_greater_than_lower() {
        let err = build(50, 50, false).unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidActionPayload),
        );
    }

    #[test]
    fn account_meta_order_matches_on_chain_layout() {
        let (_, metas) = build(0, 1, false).unwrap();
        assert_eq!(metas.len(), 10);
        assert_eq!(metas[0].pubkey, fixed(0x01)); // funder
        assert_eq!(metas[1].pubkey, fixed(0x02)); // owner (vault)
        assert_eq!(metas[2].pubkey, fixed(0x03)); // position
        assert_eq!(metas[3].pubkey, fixed(0x04)); // position_mint
        assert_eq!(metas[4].pubkey, fixed(0x05)); // position_token_account
        assert_eq!(metas[5].pubkey, fixed(0x06)); // whirlpool
        assert_eq!(metas[6].pubkey, fixed(0x07)); // token_2022_program
        assert_eq!(metas[7].pubkey, fixed(0x08)); // system_program
        assert_eq!(metas[8].pubkey, fixed(0x09)); // associated_token_program
        assert_eq!(metas[9].pubkey, fixed(0x0a)); // metadata_update_auth
    }

    #[test]
    fn signer_and_writable_flags() {
        let (_, metas) = build(0, 1, false).unwrap();
        // funder: writable + signer
        assert!(metas[0].is_signer && metas[0].is_writable);
        // owner (vault PDA): readonly, NOT signer at the meta level — the vault
        // signs via invoke_signed seeds, not as a tx signer in the meta.
        assert!(!metas[1].is_signer && !metas[1].is_writable);
        // position: writable, not signer
        assert!(!metas[2].is_signer && metas[2].is_writable);
        // position_mint: writable + signer (ephemeral mint kp)
        assert!(metas[3].is_signer && metas[3].is_writable);
        // position_token_account: writable, not signer
        assert!(!metas[4].is_signer && metas[4].is_writable);
        // remaining are readonly + non-signer
        for idx in [5, 6, 7, 8, 9] {
            assert!(!metas[idx].is_signer, "slot {idx} must not sign");
            assert!(!metas[idx].is_writable, "slot {idx} must be readonly");
        }
    }
}
