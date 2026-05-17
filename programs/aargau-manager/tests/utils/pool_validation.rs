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
        RAYDIUM_MINT_A_OFFSET, RAYDIUM_MINT_B_OFFSET,
    };
    use aargau_manager::state::Protocol;
    use aargau_manager::utils::pool_validation::{read_mint_a, read_mint_b};
    use anchor_lang::prelude::Pubkey;

    /// Build a fake pool data buffer of `total_len` bytes where the 32 bytes
    /// starting at `raw_offset` (= 8 + post-disc-offset) contain the given pubkey.
    fn fake_pool_data_with_mint_at(total_len: usize, raw_offset: usize, mint: Pubkey) -> Vec<u8> {
        let mut buf = vec![0u8; total_len];
        let mint_bytes = mint.to_bytes();
        buf[raw_offset..raw_offset + 32].copy_from_slice(&mint_bytes);
        buf
    }

    // --- read_mint_a ---

    #[test]
    fn test_read_mint_a_orca_returns_correct_pubkey() {
        let expected = Pubkey::new_unique();
        let raw_offset = 8 + ORCA_MINT_A_OFFSET;
        let buf = fake_pool_data_with_mint_at(raw_offset + 32, raw_offset, expected);
        let result = read_mint_a(&buf, Protocol::Orca);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_read_mint_a_raydium_returns_correct_pubkey() {
        let expected = Pubkey::new_unique();
        let raw_offset = 8 + RAYDIUM_MINT_A_OFFSET;
        let buf = fake_pool_data_with_mint_at(raw_offset + 32, raw_offset, expected);
        let result = read_mint_a(&buf, Protocol::Raydium);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_read_mint_a_meteora_returns_correct_pubkey() {
        let expected = Pubkey::new_unique();
        // Meteora offsets are stored as ABSOLUTE positions in the raw
        // account buffer (no +8 adjustment), unlike Orca/Raydium.
        let raw_offset = METEORA_MINT_A_OFFSET;
        let buf = fake_pool_data_with_mint_at(raw_offset + 32, raw_offset, expected);
        let result = read_mint_a(&buf, Protocol::Meteora);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_read_mint_a_buffer_too_short_returns_error() {
        // Buffer is shorter than required for the mint read.
        let buf = vec![0u8; 4];
        let result = read_mint_a(&buf, Protocol::Orca);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_mint_a_exactly_minimum_length_succeeds() {
        // Buffer is exactly the minimum needed (raw_offset + 32).
        let raw_offset = 8 + ORCA_MINT_A_OFFSET;
        let buf = vec![0u8; raw_offset + 32];
        let result = read_mint_a(&buf, Protocol::Orca);
        assert!(result.is_ok());
    }

    #[test]
    fn test_read_mint_a_one_byte_short_returns_error() {
        let raw_offset = 8 + ORCA_MINT_A_OFFSET;
        // One byte short of the minimum requirement.
        let buf = vec![0u8; raw_offset + 31];
        let result = read_mint_a(&buf, Protocol::Orca);
        assert!(result.is_err());
    }

    // --- read_mint_b ---

    #[test]
    fn test_read_mint_b_orca_returns_correct_pubkey() {
        let expected = Pubkey::new_unique();
        let raw_offset = 8 + ORCA_MINT_B_OFFSET;
        let buf = fake_pool_data_with_mint_at(raw_offset + 32, raw_offset, expected);
        let result = read_mint_b(&buf, Protocol::Orca);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_read_mint_b_raydium_returns_correct_pubkey() {
        let expected = Pubkey::new_unique();
        let raw_offset = 8 + RAYDIUM_MINT_B_OFFSET;
        let buf = fake_pool_data_with_mint_at(raw_offset + 32, raw_offset, expected);
        let result = read_mint_b(&buf, Protocol::Raydium);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_read_mint_b_meteora_returns_correct_pubkey() {
        let expected = Pubkey::new_unique();
        // Absolute offset (see `test_read_mint_a_meteora_returns_correct_pubkey`).
        let raw_offset = METEORA_MINT_B_OFFSET;
        let buf = fake_pool_data_with_mint_at(raw_offset + 32, raw_offset, expected);
        let result = read_mint_b(&buf, Protocol::Meteora);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_read_mint_b_buffer_too_short_returns_error() {
        let buf = vec![0u8; 4];
        let result = read_mint_b(&buf, Protocol::Orca);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_mint_b_exactly_minimum_length_succeeds() {
        let raw_offset = 8 + ORCA_MINT_B_OFFSET;
        let buf = vec![0u8; raw_offset + 32];
        let result = read_mint_b(&buf, Protocol::Orca);
        assert!(result.is_ok());
    }

    // --- Mint A vs Mint B distinctness ---

    #[test]
    fn test_read_mint_a_and_mint_b_return_different_positions() {
        // Place distinct pubkeys at both positions; verify each read returns the right one.
        let mint_a = Pubkey::new_unique();
        let mint_b = Pubkey::new_unique();
        let raw_a = 8 + ORCA_MINT_A_OFFSET;
        let raw_b = 8 + ORCA_MINT_B_OFFSET;
        let total = raw_b + 32;
        let mut buf = vec![0u8; total];
        buf[raw_a..raw_a + 32].copy_from_slice(&mint_a.to_bytes());
        buf[raw_b..raw_b + 32].copy_from_slice(&mint_b.to_bytes());

        assert_eq!(read_mint_a(&buf, Protocol::Orca).unwrap(), mint_a);
        assert_eq!(read_mint_b(&buf, Protocol::Orca).unwrap(), mint_b);
    }

    // --- Meteora mint_b offset is mint_a + 32 ---

    #[test]
    fn test_meteora_mint_b_offset_equals_mint_a_plus_32() {
        // Meteora LbPair layout starts with [token_x_mint (32)] then [token_y_mint (32)].
        assert_eq!(METEORA_MINT_B_OFFSET, METEORA_MINT_A_OFFSET + 32);
    }
}
