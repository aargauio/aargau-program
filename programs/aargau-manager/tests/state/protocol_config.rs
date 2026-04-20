//! Tests for `src/state/protocol_config.rs` — ProtocolConfig singleton account sizing,
//! seeds, and compile-time constants.

#[cfg(test)]
mod account_size_tests {
    use aargau_manager::state::ProtocolConfig;
    use anchor_lang::prelude::*;

    #[test]
    fn test_protocol_config_len_constant() {
        // 8 discriminator + 32+32+32 (keys) + 1 (is_paused) + 2 (fee_rate_bps) + 4 (program_version) + 1 (bump) = 112
        assert_eq!(ProtocolConfig::LEN, 112);
    }

    #[test]
    fn test_protocol_config_runtime_size() {
        let config = ProtocolConfig {
            admin_authority: Pubkey::default(),
            primary_keeper_authority: Pubkey::default(),
            secondary_keeper_authority: Pubkey::default(),
            is_paused: false,
            fee_rate_bps: 600,
            program_version: 100,
            bump: 0,
        };

        let mut buf = Vec::new();
        config.try_serialize(&mut buf).unwrap();
        assert_eq!(
            buf.len(),
            ProtocolConfig::LEN,
            "Serialized ProtocolConfig length must equal ProtocolConfig::LEN"
        );
    }

    #[test]
    fn test_protocol_config_runtime_size_is_paused_true() {
        // Toggling is_paused must not change serialized length.
        let config = ProtocolConfig {
            admin_authority: Pubkey::default(),
            primary_keeper_authority: Pubkey::default(),
            secondary_keeper_authority: Pubkey::default(),
            is_paused: true,
            fee_rate_bps: 2000,
            program_version: 10_203,
            bump: 255,
        };
        let mut buf = Vec::new();
        config.try_serialize(&mut buf).unwrap();
        assert_eq!(buf.len(), ProtocolConfig::LEN);
    }
}

#[cfg(test)]
mod seeds_tests {
    use aargau_manager::state::ProtocolConfig;

    #[test]
    fn test_protocol_config_seeds_value() {
        assert_eq!(ProtocolConfig::SEEDS, b"protocol_config");
    }

    #[test]
    fn test_protocol_config_seeds_length() {
        assert_eq!(ProtocolConfig::SEEDS.len(), 15);
    }
}

#[cfg(test)]
mod fee_constants_tests {
    use aargau_manager::state::ProtocolConfig;

    #[test]
    fn test_max_fee_rate_bps_is_2000() {
        // 2000 bps = 20% — the protocol hard cap on performance fees.
        assert_eq!(ProtocolConfig::MAX_FEE_RATE_BPS, 2000);
    }

    #[test]
    fn test_default_fee_rate_bps_is_below_max() {
        // The deployment default (600 = 6%) must always be <= MAX_FEE_RATE_BPS.
        let default_fee_bps: u16 = 600;
        assert!(default_fee_bps <= ProtocolConfig::MAX_FEE_RATE_BPS);
    }
}
