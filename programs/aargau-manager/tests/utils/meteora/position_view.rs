//! Tests for `src/utils/meteora/position_view.rs` — pure byte validation of
//! the Meteora `PositionV2` Anchor discriminator.

mod validate_position_v2_discriminator_tests {
    use crate::common::error_helpers::{aargau_err_code, err_code};
    use aargau_manager::constants::METEORA_POSITION_V2_DISCRIMINATOR;
    use aargau_manager::errors::AargauError;
    use aargau_manager::utils::meteora::position_view::validate_position_v2_discriminator;

    #[test]
    fn accepts_buffer_with_matching_discriminator() {
        let mut data = vec![0u8; 64];
        data[..8].copy_from_slice(&METEORA_POSITION_V2_DISCRIMINATOR);

        assert!(validate_position_v2_discriminator(&data).is_ok());
    }

    #[test]
    fn rejects_buffer_shorter_than_discriminator() {
        let data = vec![0u8; 4];

        let err = validate_position_v2_discriminator(&data).unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidPositionDiscriminator),
        );
    }

    #[test]
    fn rejects_buffer_with_wrong_discriminator() {
        let mut data = vec![0u8; 64];
        // Mutate one byte of the discriminator prefix.
        data[..8].copy_from_slice(&METEORA_POSITION_V2_DISCRIMINATOR);
        data[0] ^= 0xFF;

        let err = validate_position_v2_discriminator(&data).unwrap_err();
        assert_eq!(
            err_code(&err),
            aargau_err_code(AargauError::InvalidPositionDiscriminator),
        );
    }

    #[test]
    fn discriminator_matches_anchor_derivation() {
        // Hard-coded as a regression check: any change to the constant must
        // be intentional and audited (renaming the on-chain account would
        // break every prior-deployed PositionV2 read).
        assert_eq!(
            METEORA_POSITION_V2_DISCRIMINATOR,
            [117, 176, 212, 199, 245, 180, 133, 182],
        );
    }
}
