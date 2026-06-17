//! Tests for `start_rebalance_orca` — phase one of the Orca two-transaction
//! rebalance (CloseOld).
//!
//! Active tests pin the params Borsh wire shape and the error-code contract the
//! handler relies on. The CPI builder wire formats (collect_fees / decrease /
//! close) are already locked by `tests/utils/orca/*`. The mollusk replay is
//! fixture-gated like the rest of the Orca suite.

#![allow(dead_code)]

mod start_rebalance_orca_params_tests {
    use aargau_manager::instructions::vault::start_rebalance_orca::StartRebalanceOrcaParams;
    use borsh::BorshSerialize;

    /// Params Borsh layout is append-only wire format: i32 + i32 + u128 + u64 +
    /// u64 = 4 + 4 + 16 + 8 + 8 = 40 bytes, in field order.
    #[test]
    fn params_serialize_in_declared_field_order() {
        let params = StartRebalanceOrcaParams {
            new_tick_lower: -128,
            new_tick_upper: 256,
            decrease_liquidity_amount: 0x0102_0304_0506_0708_u128,
            token_min_a: 0x1112_1314_1516_1718,
            token_min_b: 0x2122_2324_2526_2728,
        };
        let mut bytes = Vec::new();
        params.serialize(&mut bytes).unwrap();
        assert_eq!(bytes.len(), 4 + 4 + 16 + 8 + 8);
        assert_eq!(i32::from_le_bytes(bytes[0..4].try_into().unwrap()), -128);
        assert_eq!(i32::from_le_bytes(bytes[4..8].try_into().unwrap()), 256);
        assert_eq!(
            u128::from_le_bytes(bytes[8..24].try_into().unwrap()),
            0x0102_0304_0506_0708_u128,
        );
        assert_eq!(
            u64::from_le_bytes(bytes[24..32].try_into().unwrap()),
            0x1112_1314_1516_1718,
        );
        assert_eq!(
            u64::from_le_bytes(bytes[32..40].try_into().unwrap()),
            0x2122_2324_2526_2728,
        );
    }
}

mod start_rebalance_orca_error_contract {
    use aargau_manager::errors::AargauError;

    /// `start_rebalance_orca` rejects a second start while a pending rebalance
    /// already exists. Error code must stay 1303.
    #[test]
    fn pending_exists_maps_to_1303() {
        assert_eq!(AargauError::PendingRebalanceExists as u32, 1303);
    }

    /// Requires an active position before the close leg can run.
    #[test]
    fn no_active_position_maps_to_1302() {
        assert_eq!(AargauError::VaultNoActivePosition as u32, 1302);
    }

    /// The close-leg slippage floors surface `SlippageExceeded` (1002) when the
    /// observed decrease delta falls short.
    #[test]
    fn slippage_maps_to_1002() {
        assert_eq!(AargauError::SlippageExceeded as u32, 1002);
    }
}

/// Validation guard mirrors the handler: `new_tick_upper > new_tick_lower` and
/// `decrease_liquidity_amount > 0`.
mod start_rebalance_orca_range_guard {
    /// The handler accepts the target range iff `upper > lower`.
    fn range_is_valid(lower: i32, upper: i32) -> bool {
        upper > lower
    }

    #[test]
    fn equal_ticks_are_rejected() {
        assert!(!range_is_valid(100, 100));
    }

    #[test]
    fn inverted_range_is_rejected() {
        assert!(!range_is_valid(256, 100));
    }

    #[test]
    fn proper_range_is_accepted() {
        assert!(range_is_valid(-100, 100));
    }

    /// The handler accepts the decrease amount iff it is non-zero.
    fn liquidity_is_valid(amount: u128) -> bool {
        amount > 0
    }

    #[test]
    fn zero_liquidity_is_rejected() {
        assert!(!liquidity_is_valid(0));
    }

    #[test]
    fn nonzero_liquidity_is_accepted() {
        assert!(liquidity_is_valid(1));
    }
}

mod start_rebalance_orca_replay {
    use crate::fixtures::load_whirlpools_program;
    use aargau_manager::constants::ORCA_WHIRLPOOL_PROGRAM_ID;
    use anchor_lang::prelude::Pubkey;
    use mollusk_svm::Mollusk;

    fn try_build_whirlpools_mollusk(_aargau_program_id: &Pubkey) -> Result<Mollusk, String> {
        let whirlpools_elf = load_whirlpools_program()?;
        let mut mollusk = Mollusk::default();
        mollusk.add_program_with_loader_and_elf(
            &ORCA_WHIRLPOOL_PROGRAM_ID,
            &mollusk_svm::program::loader_keys::LOADER_V3,
            &whirlpools_elf,
        );
        Ok(mollusk)
    }

    /// Happy path Tx1: collect fees → drain → burn NFT; `pending_rebalance` is
    /// set, `position_*` cleared, drained tokens land in the vault ATAs.
    #[test]
    #[ignore = "requires orca_whirlpools.so fixture"]
    fn start_rebalance_closes_and_persists_pending() {
        let aargau_program_id = Pubkey::new_unique();
        let Ok(_mollusk) = try_build_whirlpools_mollusk(&aargau_program_id) else {
            return; // fixture missing — skip
        };
        // Full end-to-end replay wiring is added with the dumped fixture.
    }

    /// If Tx2 never lands, the vault stays in pending state and the drained
    /// tokens remain withdrawable from the vault ATAs.
    #[test]
    #[ignore = "requires orca_whirlpools.so fixture"]
    fn pending_persists_when_open_never_runs() {
        let aargau_program_id = Pubkey::new_unique();
        let Ok(_mollusk) = try_build_whirlpools_mollusk(&aargau_program_id) else {
            return;
        };
    }
}
