use anchor_lang::prelude::*;

// ---------------------------------------------------------------------------
// Protocol program IDs
// ---------------------------------------------------------------------------

/// Orca Whirlpools CLMM program (mainnet)
pub const ORCA_WHIRLPOOL_PROGRAM_ID: Pubkey =
    pubkey!("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");

/// Raydium CLMM program (mainnet)
pub const RAYDIUM_CLMM_PROGRAM_ID: Pubkey = pubkey!("CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK");

/// Meteora DLMM program (mainnet)
pub const METEORA_DLMM_PROGRAM_ID: Pubkey = pubkey!("LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo");

// ---------------------------------------------------------------------------
// Account discriminators
// Anchor 8-byte discriminators: sha256("account:<AccountName>")[0..8]
// These are used in create_vault pool validation (layer 2).
// ---------------------------------------------------------------------------

/// Orca Whirlpool account discriminator
pub const ORCA_WHIRLPOOL_DISCRIMINATOR: [u8; 8] = [63, 149, 209, 12, 225, 128, 99, 9];

/// Raydium PoolState account discriminator
pub const RAYDIUM_POOL_STATE_DISCRIMINATOR: [u8; 8] = [247, 237, 227, 245, 215, 195, 222, 70];

/// Meteora LbPair account discriminator
pub const METEORA_LB_PAIR_DISCRIMINATOR: [u8; 8] = [33, 11, 49, 98, 181, 101, 177, 13];

// ---------------------------------------------------------------------------
// Pool layout byte offsets for mint validation in create_vault (layer 3)
// Offsets are into the data slice AFTER the 8-byte discriminator.
// ---------------------------------------------------------------------------

/// Byte offset of token_mint_a inside Orca Whirlpool data (post-discriminator)
pub const ORCA_MINT_A_OFFSET: usize = 101;
/// Byte offset of token_mint_b inside Orca Whirlpool data (post-discriminator)
pub const ORCA_MINT_B_OFFSET: usize = 181;

/// Byte offset of token_mint_0 inside Raydium PoolState data (post-discriminator)
pub const RAYDIUM_MINT_A_OFFSET: usize = 73;
/// Byte offset of token_mint_1 inside Raydium PoolState data (post-discriminator)
pub const RAYDIUM_MINT_B_OFFSET: usize = 105;

/// Byte offset of token_x_mint inside Meteora LbPair data (post-discriminator)
pub const METEORA_MINT_A_OFFSET: usize = 0;
/// Byte offset of token_y_mint inside Meteora LbPair data (post-discriminator)
pub const METEORA_MINT_B_OFFSET: usize = 32;

// ---------------------------------------------------------------------------
// Operational limits
// ---------------------------------------------------------------------------

/// Maximum bins per Meteora position — validated upfront, never silent fail
pub const MAX_METEORA_BINS: i32 = 69;

/// Pending rebalance timeout in seconds (4 minutes)
pub const PENDING_REBALANCE_TIMEOUT_SECS: i64 = 240;

/// Basis points denominator
pub const BPS_DIVISOR: u64 = 10_000;

/// Basis points denominator as u16 — used for pct_bps boundary checks
pub const BPS_DIVISOR_U16: u16 = 10_000;

// ---------------------------------------------------------------------------
// Program version
// ---------------------------------------------------------------------------

/// On-chain program version stored in ProtocolConfig.
/// Encoded as major * 10_000 + minor * 100 + patch.
/// Examples: v0.1.0 → 100, v1.0.0 → 10_000, v1.2.3 → 10_203.
pub const PROGRAM_VERSION: u32 = 100; // v0.1.0
