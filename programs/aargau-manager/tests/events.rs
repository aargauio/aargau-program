//! Tests for `src/events.rs` — Borsh roundtrip pins event field sets so any
//! indexer that decodes by offset breaks loudly when the shape changes
//! (e.g. accidental re-introduction of a previously trimmed field).

mod liquidity_changed_event_tests {
    use aargau_manager::events::{LiquidityChanged, LiquidityOp};
    use aargau_manager::state::TriggeredBy;
    use anchor_lang::prelude::Pubkey;
    use anchor_lang::AnchorDeserialize;

    #[test]
    fn liquidity_changed_borsh_roundtrip_increase() {
        let vault = Pubkey::new_unique();
        let position = Pubkey::new_unique();
        let event = LiquidityChanged {
            vault,
            position_address: position,
            op: LiquidityOp::Increase,
            amount_a: 111,
            amount_b: 222,
            timestamp: 1_700_000_000,
            triggered_by: TriggeredBy::User,
        };
        let bytes = borsh::to_vec(&event).expect("serialize");
        // 32 vault + 32 position + 1 op + 8 amount_a + 8 amount_b
        // + 8 timestamp + 1 triggered_by = 90 bytes. Asserting this size
        // is what catches a silently re-added field.
        assert_eq!(bytes.len(), 90);
        let decoded = LiquidityChanged::try_from_slice(&bytes).expect("deserialize");
        assert_eq!(decoded.vault, vault);
        assert_eq!(decoded.position_address, position);
        assert_eq!(decoded.op, LiquidityOp::Increase);
        assert_eq!(decoded.amount_a, 111);
        assert_eq!(decoded.amount_b, 222);
        assert_eq!(decoded.timestamp, 1_700_000_000);
    }

    #[test]
    fn liquidity_op_serializes_as_single_byte() {
        // `#[repr(u8)]` is the load-bearing attribute on `LiquidityOp`.
        // Borsh must emit one byte per variant.
        assert_eq!(borsh::to_vec(&LiquidityOp::Increase).unwrap(), vec![0]);
        assert_eq!(borsh::to_vec(&LiquidityOp::Decrease).unwrap(), vec![1]);
    }
}
