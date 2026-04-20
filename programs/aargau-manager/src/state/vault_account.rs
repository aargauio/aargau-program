use anchor_lang::prelude::*;

// ---------------------------------------------------------------------------
// Enums — ALL must have #[repr(u8)] so Borsh serialises as 1 byte, not 4.
// ---------------------------------------------------------------------------

/// Protocol discriminator stored in VaultAccount.
#[repr(u8)]
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Protocol {
    Orca,
    Raydium,
    Meteora,
}

/// What triggers an automated rebalance.
#[repr(u8)]
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug)]
pub enum RebalanceTrigger {
    OutOfRange,
    NearEdge,
    FollowPrice,
}

/// Direction constraint for automated rebalance.
#[repr(u8)]
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug)]
pub enum RebalanceDirection {
    Both,
    UpOnly,
    DownOnly,
}

/// Unit for range specification.
#[repr(u8)]
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug)]
pub enum RangeUnit {
    Pct,
    Bins,
}

/// Shape of the range around current price.
#[repr(u8)]
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug)]
pub enum RangeStrategy {
    Symmetric,
    Biased,
    Skewed,
}

/// Action type for execute_action.
#[repr(u8)]
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug)]
pub enum ActionType {
    Compound,
    Rebalance,
    StopLoss,
    TakeProfit,
    CollectFees,
}

/// Phase for two-phase rebalance (Orca/Raydium) or single-phase (Meteora).
#[repr(u8)]
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug)]
pub enum ExecutionPhase {
    Single,
    CloseOld,
    OpenNew,
}

/// Who triggered a rebalance or cancellation.
#[repr(u8)]
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug)]
pub enum TriggeredBy {
    User,
    Keeper,
}

// ---------------------------------------------------------------------------
// AutoRebalanceStrategy — 46 bytes (stored in VaultAccount)
// ---------------------------------------------------------------------------

/// Automation strategy stored in VaultAccount since deploy to avoid future realloc.
/// Fields are reserved for automated rebalance logic.
///
/// Layout: 1+1+1+1+2+2+1+1+2+1+2+8+8+8+7 = 46 bytes
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Default)]
pub struct AutoRebalanceStrategy {
    /// Trigger condition for auto-rebalance
    pub trigger: u8, // 1 byte (RebalanceTrigger encoded)
    /// Direction constraint
    pub direction: u8, // 1 byte
    /// Unit for range_lower/range_upper
    pub range_unit: u8, // 1 byte
    /// Range shape
    pub range_strategy: u8, // 1 byte
    /// Lower bound of range in RangeUnit
    pub range_lower: u16, // 2 bytes
    /// Upper bound of range in RangeUnit
    pub range_upper: u16, // 2 bytes
    /// Bins above current price for Meteora (validated <= 69 at create_vault)
    pub range_bins_above: u8, // 1 byte
    /// Bins below current price for Meteora
    pub range_bins_below: u8, // 1 byte
    /// Minimum minutes between rebalances
    pub cooldown_minutes: u16, // 2 bytes
    /// Maximum rebalances allowed per 24h window
    pub max_rebalances_per_day: u8, // 1 byte
    /// Minimum benefit/cost ratio × 100 bps (safety factor)
    pub safety_factor_bps: u16, // 2 bytes
    /// Max gas to spend per day in USD cents (0 = unlimited)
    pub max_daily_gas_usd_cents: u64, // 8 bytes
    /// Stop-loss trigger in USD × 1_000_000 (0 = disabled)
    pub stop_loss_usd: u64, // 8 bytes
    /// Take-profit trigger in USD × 1_000_000 (0 = disabled)
    pub take_profit_usd: u64, // 8 bytes
    /// Reserved for future strategy fields
    pub padding: [u8; 7], // 7 bytes
                          // Total: 1+1+1+1+2+2+1+1+2+1+2+8+8+8+7 = 46 ✓
}

impl AutoRebalanceStrategy {
    pub const LEN: usize = 1 + 1 + 1 + 1 + 2 + 2 + 1 + 1 + 2 + 1 + 2 + 8 + 8 + 8 + 7; // 46
}

// ---------------------------------------------------------------------------
// PendingRebalance — 18 bytes data, 19 as Option<>
// ---------------------------------------------------------------------------

/// Transient state persisted between Tx1 (CloseOld) and Tx2 (OpenNew).
/// Only relevant for Orca and Raydium. Always None for Meteora.
///
/// Layout: 4+4+8+1+1 = 18 bytes
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug)]
pub struct PendingRebalance {
    /// Target lower tick (Orca/Raydium) or bin_id (unused for Meteora)
    pub new_tick_lower: i32, // 4 bytes
    /// Target upper tick
    pub new_tick_upper: i32, // 4 bytes
    /// Unix timestamp of Tx1 — base for timeout calculation
    pub initiated_at: i64, // 8 bytes
    /// Incremented by keeper after each failed Tx2 attempt
    pub retry_count: u8, // 1 byte
    /// 0=none 1=slippage 2=funds 3=compute 4=range 5=other
    pub last_failure_reason: u8, // 1 byte
                                 // Total: 4+4+8+1+1 = 18 ✓
}

impl PendingRebalance {
    pub const LEN: usize = 4 + 4 + 8 + 1 + 1; // 18
    pub const OPTION_LEN: usize = 1 + Self::LEN; // 19
}

// ---------------------------------------------------------------------------
// VaultAccount — 8 (discriminator) + 308 (data) = 316 bytes total
// ---------------------------------------------------------------------------

/// One vault per user per pool. The PDA of this account is the direct owner
/// of all LP positions (NFTs for Orca/Raydium, position account for Meteora).
///
/// Seeds: [b"vault", user_authority, pool_address]
///
/// Layout (data only, 308 bytes):
///   Identity:         65  (user_authority 32 + pool_address 32 + protocol 1)
///   LP position:      76  (position_address 33 + position_mint 33 + range_lower 5 + range_upper 5)
///   uses_token_2022:   1
///   Transfer fees A/B: 20  (4 × (u16 + u64))
///   Transfer fees R:   30  (6 × (u16 + u64) for 3 reward tokens)
///   allowed_ops:       1
///   strategy:         46  (AutoRebalanceStrategy)
///   counters:         25  (last_rebalance_at 8 + rebalances_today 1 + gas_spent 8 + last_day_reset 8)
///   entry_value_usd:   8
///   pending_rebalance: 19  (Option<PendingRebalance>)
///   created_at:        8
///   bump:              1
///   padding:           8
///   Total:           308 ✓
#[account]
#[derive(Debug)]
pub struct VaultAccount {
    // --- Identity (65 bytes) ---
    /// Immutable destination for ALL token withdrawals — set at create_vault, never changed
    pub user_authority: Pubkey, // 32
    /// LP pool address this vault manages
    pub pool_address: Pubkey, // 32
    /// Which DEX protocol (Orca/Raydium/Meteora)
    pub protocol: Protocol, // 1

    // --- Active LP position (76 bytes) ---
    /// None = vault is idle (no open position)
    pub position_address: Option<Pubkey>, // 33
    /// NFT mint for Orca/Raydium; None for Meteora
    pub position_mint: Option<Pubkey>, // 33
    /// Lower tick (Orca/Raydium) or bin_id (Meteora); None if idle
    pub position_range_lower: Option<i32>, // 5
    /// Upper tick (Orca/Raydium) or bin_id (Meteora); None if idle
    pub position_range_upper: Option<i32>, // 5

    // --- Token-2022 flag (1 byte) ---
    /// True if either mint uses Token-2022 program
    pub uses_token_2022: bool, // 1

    // --- Transfer fee extensions for pair tokens (20 bytes) ---
    pub token_a_transfer_fee_bps: u16, // 2
    pub token_a_maximum_fee: u64,      // 8
    pub token_b_transfer_fee_bps: u16, // 2
    pub token_b_maximum_fee: u64,      // 8

    // --- Transfer fee extensions for reward tokens (30 bytes) ---
    pub reward_token_0_transfer_fee_bps: u16, // 2
    pub reward_token_0_maximum_fee: u64,      // 8
    pub reward_token_1_transfer_fee_bps: u16, // 2
    pub reward_token_1_maximum_fee: u64,      // 8
    pub reward_token_2_transfer_fee_bps: u16, // 2
    pub reward_token_2_maximum_fee: u64,      // 8

    // --- Keeper permissions bitmask (1 byte) ---
    /// Bitmask: 0x01=Compound, 0x02=Rebalance, 0x04=StopLoss, 0x08=TakeProfit
    /// Does NOT affect manual_rebalance — user always has full sovereignty.
    pub allowed_ops: u8, // 1

    // --- Automation strategy (46 bytes) ---
    /// Stored from deploy to avoid future realloc; reserved for automated rebalance
    pub strategy: AutoRebalanceStrategy, // 46

    // --- Execution counters (25 bytes) ---
    /// Unix timestamp of last successful rebalance
    pub last_rebalance_at: i64, // 8
    /// Rebalances completed in the current 24h window
    pub rebalances_today: u8, // 1
    /// Cumulative gas spent today in USD cents
    pub gas_spent_today_usd_cents: u64, // 8
    /// Unix timestamp of the last daily counter reset
    pub last_day_reset: i64, // 8

    // --- Take-profit reference (8 bytes) ---
    /// Cumulative deposit value in USD × 1_000_000; updated on each deposit
    pub entry_value_usd: u64, // 8

    // --- Pending two-phase rebalance (19 bytes) ---
    /// Some() between CloseOld (Tx1) and OpenNew (Tx2); always None for Meteora
    pub pending_rebalance: Option<PendingRebalance>, // 19

    // --- Metadata (9 bytes) ---
    /// Unix timestamp when the vault was created
    pub created_at: i64, // 8
    /// PDA bump seed
    pub bump: u8, // 1

    // --- Future-proofing padding (8 bytes) ---
    pub _padding: [u8; 8], // 8
                           // Data total: 65+76+1+20+30+1+46+25+8+19+9+8 = 308 ✓
}

impl VaultAccount {
    /// Total account size including 8-byte discriminator
    pub const LEN: usize = 8 + 308;
    pub const SEEDS: &'static [u8] = b"vault";
}
