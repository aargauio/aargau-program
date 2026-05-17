//! Tests for `src/state/keeper_action.rs` — `KeeperAction` is the only public
//! input shape for `execute_action`. If its Borsh layout drifts (e.g. a future
//! `#[repr(u8)]` change or a variant reorder) every off-chain caller breaks.
//! These pin the wire format and the variant tag assignment.

mod keeper_action_serde_tests {
    use aargau_manager::state::KeeperAction;
    use anchor_lang::AnchorDeserialize;

    #[test]
    fn collect_fees_roundtrip() {
        let action = KeeperAction::CollectFees;
        let bytes = borsh::to_vec(&action).expect("serialize");
        // Tag byte only — no payload.
        assert_eq!(bytes, vec![0]);
        let decoded = KeeperAction::try_from_slice(&bytes).expect("deserialize");
        assert_eq!(decoded, action);
    }

    #[test]
    fn increase_liquidity_roundtrip() {
        let action = KeeperAction::IncreaseLiquidity {
            amount_a_max: 1_000_000,
            amount_b_max: 2_500_000,
            active_id_slippage: 5,
        };
        let bytes = borsh::to_vec(&action).expect("serialize");
        // Tag (1) + 8 bytes amount_a_max + 8 bytes amount_b_max
        //        + 2 bytes active_id_slippage = 19 bytes.
        assert_eq!(bytes.len(), 19);
        assert_eq!(bytes[0], 1);
        let decoded = KeeperAction::try_from_slice(&bytes).expect("deserialize");
        assert_eq!(decoded, action);
    }

    #[test]
    fn decrease_liquidity_roundtrip() {
        let action = KeeperAction::DecreaseLiquidity { bps: 5_000 };
        let bytes = borsh::to_vec(&action).expect("serialize");
        // Tag (2) + 2 bytes bps = 3 bytes.
        assert_eq!(bytes.len(), 3);
        assert_eq!(bytes[0], 2);
        let decoded = KeeperAction::try_from_slice(&bytes).expect("deserialize");
        assert_eq!(decoded, action);
    }

    #[test]
    fn open_position_roundtrip() {
        let action = KeeperAction::OpenPosition {
            lower_bin_id: -120,
            upper_bin_id: 200,
        };
        let bytes = borsh::to_vec(&action).expect("serialize");
        // Tag (3) + 4 bytes lower_bin_id + 4 bytes upper_bin_id = 9 bytes.
        assert_eq!(bytes.len(), 9);
        assert_eq!(bytes[0], 3);
        let decoded = KeeperAction::try_from_slice(&bytes).expect("deserialize");
        assert_eq!(decoded, action);
    }

    #[test]
    fn close_position_roundtrip() {
        let action = KeeperAction::ClosePosition;
        let bytes = borsh::to_vec(&action).expect("serialize");
        // Tag byte only — no payload.
        assert_eq!(bytes, vec![4]);
        let decoded = KeeperAction::try_from_slice(&bytes).expect("deserialize");
        assert_eq!(decoded, action);
    }

    #[test]
    fn unknown_tag_byte_fails_deserialization() {
        // Tag 99 is not assigned to any variant; Borsh must reject.
        let bytes = vec![99u8];
        let result = KeeperAction::try_from_slice(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn truncated_increase_liquidity_payload_fails_deserialization() {
        // Tag 1 (IncreaseLiquidity) requires 18 bytes of payload
        // (two u64 + one u16 fields). Only 4 bytes provided — Borsh must
        // reject rather than reading past the end of the buffer.
        let bytes = vec![0x01, 0x00, 0x00, 0x00, 0x00];
        let result = KeeperAction::try_from_slice(&bytes);
        assert!(result.is_err());
    }
}
