//! Tests for `src/state/user_config.rs` — UserConfig account sizing and seeds.

#[cfg(test)]
mod account_size_tests {
    use aargau_manager::state::UserConfig;
    use anchor_lang::prelude::*;

    #[test]
    fn test_user_config_len_constant() {
        // 8 discriminator + 32 (user_authority) + 1 (notifications_enabled) + 1 (bump) + 8 (padding) = 50
        assert_eq!(UserConfig::LEN, 50);
    }

    #[test]
    fn test_user_config_runtime_size() {
        let config = UserConfig {
            user_authority: Pubkey::default(),
            notifications_enabled: false,
            bump: 0,
            _padding: [0u8; 8],
        };
        let mut buf = Vec::new();
        config.try_serialize(&mut buf).unwrap();
        assert_eq!(
            buf.len(),
            UserConfig::LEN,
            "Serialized UserConfig must equal UserConfig::LEN ({})",
            UserConfig::LEN
        );
    }

    #[test]
    fn test_user_config_runtime_size_with_notification_enabled() {
        // Toggling notifications_enabled must not change serialized length.
        let config = UserConfig {
            user_authority: Pubkey::default(),
            notifications_enabled: true,
            bump: 255,
            _padding: [0xFFu8; 8],
        };
        let mut buf = Vec::new();
        config.try_serialize(&mut buf).unwrap();
        assert_eq!(buf.len(), UserConfig::LEN);
    }
}

#[cfg(test)]
mod seeds_tests {
    use aargau_manager::state::UserConfig;

    #[test]
    fn test_user_config_seeds_constant() {
        assert_eq!(UserConfig::SEEDS, b"user_config");
    }

    #[test]
    fn test_user_config_seeds_length() {
        assert_eq!(UserConfig::SEEDS.len(), 11);
    }
}
