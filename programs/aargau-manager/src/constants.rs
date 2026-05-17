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
// Pool layout byte offsets for mint validation in create_vault (layer 3).
//
// Semantics differ per protocol — `pool_validation` selects the right
// arithmetic per `Protocol` variant:
//   - Orca / Raydium: offsets are RELATIVE to the byte AFTER the 8-byte
//     Anchor discriminator (handler adds 8 before slicing).
//   - Meteora: offsets are ABSOLUTE into the raw account data buffer (the
//     8-byte discriminator already counts as bytes 0..8). This matches the
//     parser at `utils/meteora/lb_pair_view`
//     (`LB_PAIR_TOKEN_X_MINT_OFFSET = 88`, `LB_PAIR_TOKEN_Y_MINT_OFFSET = 120`).
// ---------------------------------------------------------------------------

/// Byte offset of token_mint_a inside Orca Whirlpool data (post-discriminator)
pub const ORCA_MINT_A_OFFSET: usize = 101;
/// Byte offset of token_mint_b inside Orca Whirlpool data (post-discriminator)
pub const ORCA_MINT_B_OFFSET: usize = 181;

/// Byte offset of token_mint_0 inside Raydium PoolState data (post-discriminator)
pub const RAYDIUM_MINT_A_OFFSET: usize = 73;
/// Byte offset of token_mint_1 inside Raydium PoolState data (post-discriminator)
pub const RAYDIUM_MINT_B_OFFSET: usize = 105;

/// Absolute byte offset of token_x_mint inside Meteora LbPair data
/// (the 8-byte Anchor discriminator is included in the count).
pub const METEORA_MINT_A_OFFSET: usize = 88;
/// Absolute byte offset of token_y_mint inside Meteora LbPair data.
pub const METEORA_MINT_B_OFFSET: usize = 120;

// ---------------------------------------------------------------------------
// Meteora DLMM instruction discriminators (Anchor 8-byte: sha256("global:<name>")[0..8])
// ---------------------------------------------------------------------------

/// `claim_fee2(min_bin_id: i32, max_bin_id: i32, remaining_accounts_info)`
pub const METEORA_CLAIM_FEE2_DISCRIMINATOR: [u8; 8] = [112, 191, 101, 171, 28, 144, 127, 187];

/// `add_liquidity_by_strategy2(amount_x, amount_y, active_id, max_active_bin_slippage,
///                             strategy_parameters, remaining_accounts_info)`
pub const METEORA_ADD_LIQUIDITY_BY_STRATEGY2_DISCRIMINATOR: [u8; 8] =
    [3, 221, 149, 218, 111, 141, 118, 213];

/// `remove_liquidity_by_range2(from_bin_id, to_bin_id, bps_to_remove, remaining_accounts_info)`
pub const METEORA_REMOVE_LIQUIDITY_BY_RANGE2_DISCRIMINATOR: [u8; 8] =
    [204, 2, 195, 145, 53, 145, 145, 205];

/// `initialize_position(lower_bin_id: i32, width: i32)`
/// Derived: `sha256("global:initialize_position")[..8]`.
pub const METEORA_INITIALIZE_POSITION_DISCRIMINATOR: [u8; 8] =
    [219, 192, 234, 71, 190, 191, 102, 80];

/// `close_position_if_empty()` (no args beyond discriminator).
pub const METEORA_CLOSE_POSITION_IF_EMPTY_DISCRIMINATOR: [u8; 8] =
    [59, 124, 212, 118, 91, 152, 110, 157];

/// `rebalance_liquidity(...)` — atomic close-old-bins + open-new-bins in a
/// single CPI. Derived: `sha256("global:rebalance_liquidity")[..8]`.
pub const METEORA_REBALANCE_LIQUIDITY_DISCRIMINATOR: [u8; 8] = [92, 4, 176, 193, 119, 185, 83, 9];

/// Anchor account discriminator for Meteora's `PositionV2` zero-copy account.
/// Derived: `sha256("account:PositionV2")[..8]`. Used by callers to
/// pre-validate `vault.position_address` before any CPI fires.
pub const METEORA_POSITION_V2_DISCRIMINATOR: [u8; 8] = [117, 176, 212, 199, 245, 180, 133, 182];

// ---------------------------------------------------------------------------
// Meteora bin layout / DLMM constants
// ---------------------------------------------------------------------------

/// Number of bins packed inside a single Meteora `BinArray` PDA.
pub const METEORA_BINS_PER_ARRAY: i64 = 70;

/// Hard upper bound on the per-call `active_id_slippage` accepted by
/// `IncreaseLiquidity` (and `manual_rebalance`). Meteora's SDK default is 3
/// bins; the cap stays generous enough to absorb high-volatility rebalances
/// while preventing a buggy / hostile caller from disabling slippage
/// protection entirely (e.g. by passing `i32::MAX`).
pub const MAX_ACTIVE_BIN_SLIPPAGE_HARD_CAP: u16 = 50;

/// Largest absolute `bin_id` reachable by Meteora's inline `LbPair` bitmap
/// (~78 BinArray indices × `METEORA_BINS_PER_ARRAY`). Anything outside
/// `[-LIMIT, LIMIT]` requires the separate `bin_array_bitmap_extension`
/// PDA, which is not wired in this program. Callers reject such ranges
/// upfront with `BitmapExtensionRequired` to fail fast instead of letting
/// the CPI return a less actionable error.
pub const METEORA_INLINE_BITMAP_BIN_LIMIT: i32 = 5460;

// ---------------------------------------------------------------------------
// External program IDs used as account placeholders for Meteora CPIs
// ---------------------------------------------------------------------------

/// SPL Memo program — passed as `memo_program` account in `claim_fee2` and
/// `remove_liquidity_by_range2`. Meteora forwards a memo when fees are paid.
pub const MEMO_PROGRAM_ID: Pubkey = pubkey!("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");

// ---------------------------------------------------------------------------
// PDA seeds
// ---------------------------------------------------------------------------

/// Seed of the protocol treasury PDA (`[b"treasury", protocol_config]`).
/// Centralised so every consumer derives the same address; never inline the
/// literal in account constraints.
pub const TREASURY_SEED: &[u8] = b"treasury";

// ---------------------------------------------------------------------------
// Meteora `rebalance_liquidity` invariants
// ---------------------------------------------------------------------------

/// The Aargau vault collects fees explicitly via `claim_fee2` and routes the
/// performance split to the treasury BEFORE calling `rebalance_liquidity`.
/// Passing `should_claim_fee = true` to Meteora would double-credit the user
/// with the fees we already extracted, so this constant pins the flag to
/// `false` at every CPI call site.
pub const METEORA_REBALANCE_SHOULD_CLAIM_FEE: bool = false;

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
