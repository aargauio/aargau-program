//! Tests for `src/state/raydium_keeper_action.rs` — `RaydiumKeeperAction` is
//! the public input shape of the Raydium lifecycle instruction. Its Borsh tags
//! must match `OrcaKeeperAction` variant-for-variant.

mod raydium_keeper_action_serde_tests {
    use aargau_manager::state::{OrcaKeeperAction, RaydiumKeeperAction};
    use anchor_lang::AnchorDeserialize;

    fn roundtrip(action: RaydiumKeeperAction) -> Vec<u8> {
        let bytes = borsh::to_vec(&action).expect("serialize");
        let decoded = RaydiumKeeperAction::try_from_slice(&bytes).expect("deserialize");
        assert_eq!(decoded, action);
        bytes
    }

    #[test]
    fn open_position_is_tag_0_with_two_i32() {
        let bytes = roundtrip(RaydiumKeeperAction::OpenPosition {
            tick_lower: -21_240,
            tick_upper: -21_120,
        });
        assert_eq!(bytes.len(), 1 + 4 + 4);
        assert_eq!(bytes[0], 0);
        assert_eq!(&bytes[1..5], &(-21_240i32).to_le_bytes());
        assert_eq!(&bytes[5..9], &(-21_120i32).to_le_bytes());
    }

    #[test]
    fn increase_liquidity_is_tag_1() {
        let bytes = roundtrip(RaydiumKeeperAction::IncreaseLiquidity {
            liquidity_amount: u128::MAX,
            token_max_a: 1,
            token_max_b: u64::MAX,
        });
        assert_eq!(bytes.len(), 1 + 16 + 8 + 8);
        assert_eq!(bytes[0], 1);
    }

    #[test]
    fn decrease_liquidity_is_tag_2() {
        let bytes = roundtrip(RaydiumKeeperAction::DecreaseLiquidity {
            liquidity_amount: 0,
            token_min_a: 0,
            token_min_b: 0,
        });
        assert_eq!(bytes.len(), 1 + 16 + 8 + 8);
        assert_eq!(bytes[0], 2);
    }

    #[test]
    fn collect_fees_is_tag_3() {
        assert_eq!(roundtrip(RaydiumKeeperAction::CollectFees), vec![3]);
    }

    #[test]
    fn close_position_is_tag_4() {
        assert_eq!(roundtrip(RaydiumKeeperAction::ClosePosition), vec![4]);
    }

    #[test]
    fn wire_format_matches_orca_action_variant_for_variant() {
        let pairs = [
            (
                borsh::to_vec(&RaydiumKeeperAction::OpenPosition {
                    tick_lower: -7,
                    tick_upper: 13,
                }),
                borsh::to_vec(&OrcaKeeperAction::OpenPosition {
                    tick_lower: -7,
                    tick_upper: 13,
                }),
            ),
            (
                borsh::to_vec(&RaydiumKeeperAction::IncreaseLiquidity {
                    liquidity_amount: 9,
                    token_max_a: 8,
                    token_max_b: 7,
                }),
                borsh::to_vec(&OrcaKeeperAction::IncreaseLiquidity {
                    liquidity_amount: 9,
                    token_max_a: 8,
                    token_max_b: 7,
                }),
            ),
            (
                borsh::to_vec(&RaydiumKeeperAction::DecreaseLiquidity {
                    liquidity_amount: 9,
                    token_min_a: 8,
                    token_min_b: 7,
                }),
                borsh::to_vec(&OrcaKeeperAction::DecreaseLiquidity {
                    liquidity_amount: 9,
                    token_min_a: 8,
                    token_min_b: 7,
                }),
            ),
            (
                borsh::to_vec(&RaydiumKeeperAction::CollectFees),
                borsh::to_vec(&OrcaKeeperAction::CollectFees),
            ),
            (
                borsh::to_vec(&RaydiumKeeperAction::ClosePosition),
                borsh::to_vec(&OrcaKeeperAction::ClosePosition),
            ),
        ];
        for (raydium, orca) in pairs {
            assert_eq!(raydium.unwrap(), orca.unwrap());
        }
    }

    #[test]
    fn unknown_tag_fails_deserialization() {
        assert!(RaydiumKeeperAction::try_from_slice(&[5]).is_err());
    }

    #[test]
    fn truncated_payload_fails_deserialization() {
        // Tag 1 needs 32 payload bytes.
        assert!(RaydiumKeeperAction::try_from_slice(&[1, 0, 0, 0]).is_err());
    }
}
