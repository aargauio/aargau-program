//! Tests for `src/state/vault_account.rs` — VaultAccount, Protocol, PendingRebalance,
//! AutoRebalanceStrategy, and all program enums defined in the state module.

#[cfg(test)]
mod account_size_tests {
    use aargau_manager::state::{
        AutoRebalanceStrategy, PendingRebalance, ProtocolConfig, VaultAccount,
    };

    /// CRITICAL: these asserts are the source of truth for account sizing.
    /// If any field is added or removed, these tests will fail.
    #[test]
    fn test_account_sizes() {
        // VaultAccount: 8 discriminator + 308 data = 316 total
        assert_eq!(
            VaultAccount::LEN,
            316,
            "VaultAccount::LEN must be 316 (8 disc + 308 data)"
        );

        // ProtocolConfig: 8 discriminator + 104 data = 112 total
        assert_eq!(
            ProtocolConfig::LEN,
            112,
            "ProtocolConfig::LEN must be 112 (8 disc + 104 data)"
        );

        // PendingRebalance: 18 data bytes
        assert_eq!(
            PendingRebalance::LEN,
            18,
            "PendingRebalance::LEN must be 18"
        );

        // Option<PendingRebalance>: 1 discriminant + 18 = 19
        assert_eq!(
            PendingRebalance::OPTION_LEN,
            19,
            "PendingRebalance::OPTION_LEN must be 19"
        );

        // AutoRebalanceStrategy: 46 bytes
        assert_eq!(
            AutoRebalanceStrategy::LEN,
            46,
            "AutoRebalanceStrategy::LEN must be 46"
        );
    }

    #[test]
    fn test_vault_account_runtime_size() {
        // VaultAccount::LEN represents the MAXIMUM account allocation (all Option fields = Some).
        // Borsh serializes Options as 1 byte for None, 1+size for Some.
        // This test verifies the all-Some case matches LEN exactly.
        use aargau_manager::state::{
            AutoRebalanceStrategy, PendingRebalance, Protocol, VaultAccount,
        };
        use anchor_lang::prelude::*;

        let vault = VaultAccount {
            user_authority: Pubkey::default(),
            pool_address: Pubkey::default(),
            protocol: Protocol::Orca,
            // All Option fields populated as Some — this is the worst case for account sizing
            position_address: Some(Pubkey::default()),
            position_mint: Some(Pubkey::default()),
            position_range_lower: Some(0i32),
            position_range_upper: Some(0i32),
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
            pending_rebalance: Some(PendingRebalance {
                new_tick_lower: 0,
                new_tick_upper: 0,
                initiated_at: 0,
                retry_count: 0,
                last_failure_reason: 0,
            }),
            created_at: 0,
            bump: 0,
            _padding: [0u8; 8],
        };

        let mut buf = Vec::new();
        vault.try_serialize(&mut buf).unwrap();
        // try_serialize includes 8-byte discriminator + Borsh data
        // LEN = 316 = 8 disc + 308 max Borsh size (all Options as Some)
        assert_eq!(
            buf.len(),
            VaultAccount::LEN,
            "Serialized VaultAccount (all Some) must equal VaultAccount::LEN ({})",
            VaultAccount::LEN
        );
    }
}

#[cfg(test)]
mod enum_size_tests {
    use aargau_manager::state::{
        ActionType, ExecutionPhase, Protocol, RangeStrategy, RangeUnit, RebalanceDirection,
        RebalanceTrigger, TriggeredBy,
    };

    /// CRITICAL: all enums must be exactly 1 byte due to #[repr(u8)].
    /// Without #[repr(u8)], Borsh would serialize as u32 (4 bytes),
    /// invalidating all account size calculations.
    #[test]
    fn test_enum_sizes() {
        assert_eq!(
            std::mem::size_of::<Protocol>(),
            1,
            "Protocol must be 1 byte (#[repr(u8)])"
        );
        assert_eq!(
            std::mem::size_of::<ExecutionPhase>(),
            1,
            "ExecutionPhase must be 1 byte (#[repr(u8)])"
        );
        assert_eq!(
            std::mem::size_of::<TriggeredBy>(),
            1,
            "TriggeredBy must be 1 byte (#[repr(u8)])"
        );
        assert_eq!(
            std::mem::size_of::<RebalanceTrigger>(),
            1,
            "RebalanceTrigger must be 1 byte (#[repr(u8)])"
        );
        assert_eq!(
            std::mem::size_of::<RebalanceDirection>(),
            1,
            "RebalanceDirection must be 1 byte (#[repr(u8)])"
        );
        assert_eq!(
            std::mem::size_of::<RangeUnit>(),
            1,
            "RangeUnit must be 1 byte (#[repr(u8)])"
        );
        assert_eq!(
            std::mem::size_of::<RangeStrategy>(),
            1,
            "RangeStrategy must be 1 byte (#[repr(u8)])"
        );
        assert_eq!(
            std::mem::size_of::<ActionType>(),
            1,
            "ActionType must be 1 byte (#[repr(u8)])"
        );
    }
}

#[cfg(test)]
mod enum_variant_tests {
    use aargau_manager::state::{
        ActionType, ExecutionPhase, Protocol, RangeStrategy, RangeUnit, RebalanceDirection,
        RebalanceTrigger, TriggeredBy,
    };

    // Protocol discriminants — order matters for Borsh round-trip compatibility.
    #[test]
    fn test_protocol_orca_discriminant() {
        assert_eq!(Protocol::Orca as u8, 0);
    }

    #[test]
    fn test_protocol_raydium_discriminant() {
        assert_eq!(Protocol::Raydium as u8, 1);
    }

    #[test]
    fn test_protocol_meteora_discriminant() {
        assert_eq!(Protocol::Meteora as u8, 2);
    }

    // RebalanceTrigger discriminants
    #[test]
    fn test_rebalance_trigger_out_of_range_discriminant() {
        assert_eq!(RebalanceTrigger::OutOfRange as u8, 0);
    }

    #[test]
    fn test_rebalance_trigger_near_edge_discriminant() {
        assert_eq!(RebalanceTrigger::NearEdge as u8, 1);
    }

    #[test]
    fn test_rebalance_trigger_follow_price_discriminant() {
        assert_eq!(RebalanceTrigger::FollowPrice as u8, 2);
    }

    // RebalanceDirection discriminants
    #[test]
    fn test_rebalance_direction_both_discriminant() {
        assert_eq!(RebalanceDirection::Both as u8, 0);
    }

    #[test]
    fn test_rebalance_direction_up_only_discriminant() {
        assert_eq!(RebalanceDirection::UpOnly as u8, 1);
    }

    #[test]
    fn test_rebalance_direction_down_only_discriminant() {
        assert_eq!(RebalanceDirection::DownOnly as u8, 2);
    }

    // RangeUnit discriminants
    #[test]
    fn test_range_unit_pct_discriminant() {
        assert_eq!(RangeUnit::Pct as u8, 0);
    }

    #[test]
    fn test_range_unit_bins_discriminant() {
        assert_eq!(RangeUnit::Bins as u8, 1);
    }

    // RangeStrategy discriminants
    #[test]
    fn test_range_strategy_symmetric_discriminant() {
        assert_eq!(RangeStrategy::Symmetric as u8, 0);
    }

    #[test]
    fn test_range_strategy_biased_discriminant() {
        assert_eq!(RangeStrategy::Biased as u8, 1);
    }

    #[test]
    fn test_range_strategy_skewed_discriminant() {
        assert_eq!(RangeStrategy::Skewed as u8, 2);
    }

    // ActionType discriminants
    #[test]
    fn test_action_type_compound_discriminant() {
        assert_eq!(ActionType::Compound as u8, 0);
    }

    #[test]
    fn test_action_type_rebalance_discriminant() {
        assert_eq!(ActionType::Rebalance as u8, 1);
    }

    #[test]
    fn test_action_type_stop_loss_discriminant() {
        assert_eq!(ActionType::StopLoss as u8, 2);
    }

    #[test]
    fn test_action_type_take_profit_discriminant() {
        assert_eq!(ActionType::TakeProfit as u8, 3);
    }

    #[test]
    fn test_action_type_collect_fees_discriminant() {
        assert_eq!(ActionType::CollectFees as u8, 4);
    }

    // ExecutionPhase discriminants
    #[test]
    fn test_execution_phase_single_discriminant() {
        assert_eq!(ExecutionPhase::Single as u8, 0);
    }

    #[test]
    fn test_execution_phase_close_old_discriminant() {
        assert_eq!(ExecutionPhase::CloseOld as u8, 1);
    }

    #[test]
    fn test_execution_phase_open_new_discriminant() {
        assert_eq!(ExecutionPhase::OpenNew as u8, 2);
    }

    // TriggeredBy discriminants
    #[test]
    fn test_triggered_by_user_discriminant() {
        assert_eq!(TriggeredBy::User as u8, 0);
    }

    #[test]
    fn test_triggered_by_keeper_discriminant() {
        assert_eq!(TriggeredBy::Keeper as u8, 1);
    }
}

#[cfg(test)]
mod vault_account_seeds_tests {
    use aargau_manager::state::VaultAccount;

    #[test]
    fn test_vault_account_seeds_value() {
        assert_eq!(VaultAccount::SEEDS, b"vault");
    }

    #[test]
    fn test_vault_account_seeds_length() {
        assert_eq!(VaultAccount::SEEDS.len(), 5);
    }
}

#[cfg(test)]
mod vault_none_variant_tests {
    use aargau_manager::state::{AutoRebalanceStrategy, Protocol, VaultAccount};
    use anchor_lang::prelude::*;

    #[test]
    fn test_vault_account_all_none_serializes_smaller_than_len() {
        // When all Option fields are None, serialized size is smaller than LEN.
        // LEN is the maximum (all-Some) allocation.
        let vault = VaultAccount {
            user_authority: Pubkey::default(),
            pool_address: Pubkey::default(),
            protocol: Protocol::Meteora,
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
            bump: 0,
            _padding: [0u8; 8],
        };

        let mut buf = Vec::new();
        vault.try_serialize(&mut buf).unwrap();
        // All-None serializes smaller than all-Some (which matches LEN).
        assert!(buf.len() < VaultAccount::LEN);
        // But it must still include the 8-byte discriminator.
        assert!(buf.len() >= 8);
    }
}

#[cfg(test)]
mod strategy_size_tests {
    use aargau_manager::state::{AutoRebalanceStrategy, PendingRebalance};
    use borsh::BorshSerialize;

    #[test]
    fn test_auto_rebalance_strategy_borsh_size() {
        let s = AutoRebalanceStrategy::default();
        let mut buf = Vec::new();
        s.serialize(&mut buf).unwrap();
        // Borsh size should match LEN constant
        assert_eq!(buf.len(), AutoRebalanceStrategy::LEN);
    }

    #[test]
    fn test_pending_rebalance_borsh_size() {
        let pr = PendingRebalance {
            new_tick_lower: 0,
            new_tick_upper: 0,
            initiated_at: 0,
            retry_count: 0,
            last_failure_reason: 0,
        };
        let mut buf = Vec::new();
        pr.serialize(&mut buf).unwrap();
        assert_eq!(buf.len(), PendingRebalance::LEN);
    }

    #[test]
    fn test_option_pending_rebalance_borsh_size() {
        let none: Option<PendingRebalance> = None;
        let mut buf = Vec::new();
        none.serialize(&mut buf).unwrap();
        assert_eq!(buf.len(), 1); // just the None discriminant

        let some: Option<PendingRebalance> = Some(PendingRebalance {
            new_tick_lower: 0,
            new_tick_upper: 0,
            initiated_at: 0,
            retry_count: 0,
            last_failure_reason: 0,
        });
        let mut buf2 = Vec::new();
        some.serialize(&mut buf2).unwrap();
        assert_eq!(buf2.len(), PendingRebalance::OPTION_LEN);
    }
}
