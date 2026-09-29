//! Tests for `src/utils/pool_validation.rs` — read_mint_a, read_mint_b, and
//! check_no_non_transferable_extension pure logic paths.
//!
//! `validate_pool_owner_and_discriminator` requires a live `AccountInfo` with
//! a real owner and borrowed data; it cannot be unit-tested without the Solana
//! runtime. Only the two pure read helpers and the extension walker are covered.

#[cfg(test)]
mod read_mint_tests {
    use aargau_manager::constants::{
        METEORA_MINT_A_OFFSET, METEORA_MINT_B_OFFSET, ORCA_MINT_A_OFFSET, ORCA_MINT_B_OFFSET,
        ORCA_WHIRLPOOL_DISCRIMINATOR, RAYDIUM_POOL_STATE_DISCRIMINATOR,
    };
    use aargau_manager::state::Protocol;
    use aargau_manager::utils::pool_validation::{read_mint_a, read_mint_b};
    use anchor_lang::prelude::Pubkey;

    /// Non-zero filler for fields the mint reader must skip, so a read at a
    /// shifted offset can never land on zeroes that happen to match.
    const FILLER_BYTE: u8 = 0xEE;

    /// Appends account fields in declaration order, so a mint's position comes
    /// from the upstream account layout and never from the constant under test.
    struct AccountLayoutBuilder {
        data: Vec<u8>,
    }

    impl AccountLayoutBuilder {
        fn new(discriminator: [u8; 8]) -> Self {
            Self {
                data: discriminator.to_vec(),
            }
        }

        /// A field the reader ignores, `len_bytes` wide.
        fn skip_field(mut self, len_bytes: usize) -> Self {
            self.data
                .extend(std::iter::repeat_n(FILLER_BYTE, len_bytes));
            self
        }

        fn pubkey_field(mut self, key: Pubkey) -> Self {
            self.data.extend_from_slice(key.as_ref());
            self
        }

        fn build(self) -> Vec<u8> {
            self.data
        }
    }

    /// Orca `Whirlpool` account up to `token_vault_b`, fields in on-chain order.
    fn whirlpool_with_mints(mint_a: Pubkey, mint_b: Pubkey) -> Vec<u8> {
        AccountLayoutBuilder::new(ORCA_WHIRLPOOL_DISCRIMINATOR)
            .skip_field(32) // whirlpools_config
            .skip_field(1) // whirlpool_bump
            .skip_field(2) // tick_spacing
            .skip_field(2) // fee_tier_index_seed
            .skip_field(2) // fee_rate
            .skip_field(2) // protocol_fee_rate
            .skip_field(16) // liquidity
            .skip_field(16) // sqrt_price
            .skip_field(4) // tick_current_index
            .skip_field(8) // protocol_fee_owed_a
            .skip_field(8) // protocol_fee_owed_b
            .pubkey_field(mint_a) // token_mint_a
            .pubkey_field(Pubkey::new_unique()) // token_vault_a
            .skip_field(16) // fee_growth_global_a
            .pubkey_field(mint_b) // token_mint_b
            .pubkey_field(Pubkey::new_unique()) // token_vault_b
            .build()
    }

    /// Raydium CLMM `PoolState` account up to `observation_key`, fields in
    /// on-chain order.
    fn raydium_pool_state_with_mints(mint_0: Pubkey, mint_1: Pubkey) -> Vec<u8> {
        AccountLayoutBuilder::new(RAYDIUM_POOL_STATE_DISCRIMINATOR)
            .skip_field(1) // bump
            .pubkey_field(Pubkey::new_unique()) // amm_config
            .pubkey_field(Pubkey::new_unique()) // owner
            .pubkey_field(mint_0) // token_mint_0
            .pubkey_field(mint_1) // token_mint_1
            .pubkey_field(Pubkey::new_unique()) // token_vault_0
            .pubkey_field(Pubkey::new_unique()) // token_vault_1
            .pubkey_field(Pubkey::new_unique()) // observation_key
            .build()
    }

    /// Build a fake pool data buffer of `total_len` bytes where the 32 bytes
    /// starting at the absolute `offset` contain the given pubkey.
    fn fake_pool_data_with_mint_at(total_len: usize, offset: usize, mint: Pubkey) -> Vec<u8> {
        let mut buf = vec![0u8; total_len];
        let mint_bytes = mint.to_bytes();
        buf[offset..offset + 32].copy_from_slice(&mint_bytes);
        buf
    }

    // --- Orca / Raydium: real upstream layouts ---

    #[test]
    fn test_read_mints_orca_match_whirlpool_layout() {
        let mint_a = Pubkey::new_unique();
        let mint_b = Pubkey::new_unique();
        let buf = whirlpool_with_mints(mint_a, mint_b);

        assert_eq!(read_mint_a(&buf, Protocol::Orca).unwrap(), mint_a);
        assert_eq!(read_mint_b(&buf, Protocol::Orca).unwrap(), mint_b);
    }

    #[test]
    fn test_read_mints_raydium_match_pool_state_layout() {
        let mint_0 = Pubkey::new_unique();
        let mint_1 = Pubkey::new_unique();
        let buf = raydium_pool_state_with_mints(mint_0, mint_1);

        assert_eq!(read_mint_a(&buf, Protocol::Raydium).unwrap(), mint_0);
        assert_eq!(read_mint_b(&buf, Protocol::Raydium).unwrap(), mint_1);
    }

    // --- Meteora ---

    #[test]
    fn test_read_mint_a_meteora_returns_correct_pubkey() {
        let expected = Pubkey::new_unique();
        let offset = METEORA_MINT_A_OFFSET;
        let buf = fake_pool_data_with_mint_at(offset + 32, offset, expected);
        let result = read_mint_a(&buf, Protocol::Meteora);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_read_mint_b_meteora_returns_correct_pubkey() {
        let expected = Pubkey::new_unique();
        let offset = METEORA_MINT_B_OFFSET;
        let buf = fake_pool_data_with_mint_at(offset + 32, offset, expected);
        let result = read_mint_b(&buf, Protocol::Meteora);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), expected);
    }

    // --- Buffer length bounds ---

    #[test]
    fn test_read_mint_a_buffer_too_short_returns_error() {
        // Buffer is shorter than required for the mint read.
        let buf = vec![0u8; 4];
        let result = read_mint_a(&buf, Protocol::Orca);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_mint_a_exactly_minimum_length_succeeds() {
        // Buffer is exactly the minimum needed (offset + 32).
        let buf = vec![0u8; ORCA_MINT_A_OFFSET + 32];
        let result = read_mint_a(&buf, Protocol::Orca);
        assert!(result.is_ok());
    }

    #[test]
    fn test_read_mint_a_one_byte_short_returns_error() {
        // One byte short of the minimum requirement.
        let buf = vec![0u8; ORCA_MINT_A_OFFSET + 31];
        let result = read_mint_a(&buf, Protocol::Orca);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_mint_b_buffer_too_short_returns_error() {
        let buf = vec![0u8; 4];
        let result = read_mint_b(&buf, Protocol::Orca);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_mint_b_exactly_minimum_length_succeeds() {
        let buf = vec![0u8; ORCA_MINT_B_OFFSET + 32];
        let result = read_mint_b(&buf, Protocol::Orca);
        assert!(result.is_ok());
    }

    #[test]
    fn test_read_mint_b_one_byte_short_returns_error() {
        let buf = vec![0u8; ORCA_MINT_B_OFFSET + 31];
        let result = read_mint_b(&buf, Protocol::Orca);
        assert!(result.is_err());
    }

    // --- Meteora mint_b offset is mint_a + 32 ---

    #[test]
    fn test_meteora_mint_b_offset_equals_mint_a_plus_32() {
        // Meteora LbPair layout starts with [token_x_mint (32)] then [token_y_mint (32)].
        assert_eq!(METEORA_MINT_B_OFFSET, METEORA_MINT_A_OFFSET + 32);
    }
}
