use crate::state::{Protocol, TriggeredBy};
use anchor_lang::prelude::*;

// ---------------------------------------------------------------------------
// 26 events — complete list from vault-program-architecture.md §9
// ---------------------------------------------------------------------------

// --- Setup ---

#[event]
pub struct ProtocolInitialized {
    pub admin: Pubkey,
    pub primary_keeper: Pubkey,
    pub secondary_keeper: Pubkey,
    pub fee_rate_bps: u16,
    pub timestamp: i64,
}

// --- Vault lifecycle ---

#[event]
pub struct VaultCreated {
    pub vault: Pubkey,
    pub user: Pubkey,
    pub pool: Pubkey,
    pub protocol: Protocol,
    pub timestamp: i64,
}

#[event]
pub struct VaultClosed {
    pub vault: Pubkey,
    pub user: Pubkey,
    pub timestamp: i64,
}

// --- Deposit and withdraw ---

#[event]
pub struct DepositMade {
    pub vault: Pubkey,
    pub amount_a: u64,
    pub amount_b: u64,
    pub entry_value_usd_delta: u64,
    pub timestamp: i64,
}

#[event]
pub struct WithdrawMade {
    pub vault: Pubkey,
    pub amount_a_gross: u64,
    pub amount_b_gross: u64,
    pub amount_a_user_net: u64,
    pub amount_b_user_net: u64,
    pub is_full: bool,
    pub lp_position_inaccessible: bool,
    pub timestamp: i64,
}

// --- Rebalance ---

#[event]
pub struct RebalancePending {
    pub vault: Pubkey,
    pub new_range_lower: i32,
    pub new_range_upper: i32,
    pub initiated_at: i64,
    pub triggered_by: TriggeredBy,
}

#[event]
pub struct RebalanceExecuted {
    pub vault: Pubkey,
    pub protocol: Protocol,
    pub old_range_lower: i32,
    pub old_range_upper: i32,
    pub new_range_lower: i32,
    pub new_range_upper: i32,
    pub triggered_by: TriggeredBy,
    pub timestamp: i64,
}

#[event]
pub struct RebalanceAttemptFailed {
    pub vault: Pubkey,
    pub retry_count: u8,
    pub failure_reason: u8,
    pub timestamp: i64,
}

#[event]
pub struct RebalanceCancelled {
    pub vault: Pubkey,
    pub cancelled_by: TriggeredBy,
    pub timestamp: i64,
}

#[event]
pub struct RebalanceRetryRequested {
    pub vault: Pubkey,
    pub new_tick_lower: i32,
    pub new_tick_upper: i32,
    pub timestamp: i64,
}

// --- Fees ---

#[event]
pub struct FeesClaimed {
    pub vault: Pubkey,
    pub gross_a: u64,
    pub gross_b: u64,
    pub aargau_fee_a: u64,
    pub aargau_fee_b: u64,
    pub user_net_a: u64,
    pub user_net_b: u64,
    pub timestamp: i64,
}

#[event]
pub struct FeesCompounded {
    pub vault: Pubkey,
    pub compounded_a: u64,
    pub compounded_b: u64,
    pub timestamp: i64,
}

#[event]
pub struct RewardsClaimed {
    pub vault: Pubkey,
    pub user: Pubkey,
    pub amounts: [u64; 3],
    pub reward_mints: [Option<Pubkey>; 3],
    pub timestamp: i64,
}

/// Emitted by `execute_action_orca::CollectFees` when an active reward slot is
/// skipped because its mint carries a Token-2022 extension the v2 CPI cannot
/// service with `remaining_accounts_info = None` (`TransferHook` /
/// `NonTransferable`). Reward collection is best-effort: the slot is left
/// uncollected in the Orca position (no funds lost — the vault still owns the
/// position) so one incompatible reward mint can never revert the whole
/// `CollectFees` transaction and block LP-fee collection. The skipped reward
/// can be collected later off-chain or once hook support is wired.
#[event]
pub struct RewardCollectionSkipped {
    pub vault: Pubkey,
    pub reward_index: u8,
    pub reward_mint: Pubkey,
    pub timestamp: i64,
}

// --- Liquidity changes (single-sided ops outside a full rebalance) ---

/// Direction of a liquidity change emitted by `execute_action`.
/// `#[repr(u8)]` so Borsh serialises as 1 byte.
#[repr(u8)]
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum LiquidityOp {
    Increase,
    Decrease,
}

/// Emitted by `execute_action::IncreaseLiquidity` and
/// `execute_action::DecreaseLiquidity`.
///
/// `amount_a` / `amount_b` are the gross amounts moved between the vault ATAs
/// and the pool reserves, observed from balance diffs on the vault token
/// accounts before and after the underlying CPI. The direction (grew vs.
/// shrank) is encoded in `op`; consumers derive the signed flow off-chain.
///
/// The DLMM-internal `liquidity` field of the position (Q64.64 bin liquidity)
/// is intentionally not surfaced here — it would require parsing the DLMM
/// position account and gives no extra information beyond `op + amounts` for
/// indexer / P&L reconstruction.
#[event]
pub struct LiquidityChanged {
    pub vault: Pubkey,
    pub position_address: Pubkey,
    pub op: LiquidityOp,
    pub amount_a: u64,
    pub amount_b: u64,
    pub timestamp: i64,
    pub triggered_by: TriggeredBy,
}

// --- Position ---

#[event]
pub struct PositionOpened {
    pub vault: Pubkey,
    pub position_address: Pubkey,
    pub range_lower: i32,
    pub range_upper: i32,
    pub timestamp: i64,
}

#[event]
pub struct PositionClosed {
    pub vault: Pubkey,
    pub position_address: Pubkey,
    pub timestamp: i64,
}

// --- Automation ---

#[event]
pub struct StopLossTriggered {
    pub vault: Pubkey,
    pub total_value_usd: u64,
    pub limit_usd: u64,
    pub pending_cleared: bool,
    pub timestamp: i64,
}

#[event]
pub struct TakeProfitTriggered {
    pub vault: Pubkey,
    pub total_value_usd: u64,
    pub target_usd: u64,
    pub pending_cleared: bool,
    pub timestamp: i64,
}

#[event]
pub struct AutomationFailed {
    pub vault: Pubkey,
    pub failure_reason: u8,
    pub last_attempt_at: i64,
}

// --- Emergency and admin ---

#[event]
pub struct EmergencyWithdraw {
    pub vault: Pubkey,
    pub user: Pubkey,
    pub amount_a: u64,
    pub amount_b: u64,
    pub fee_skipped: bool,
    pub lp_position_inaccessible: bool,
    pub timestamp: i64,
}

#[event]
pub struct AdminEmergencyTransfer {
    pub vault: Pubkey,
    pub user: Pubkey,
    pub amount_a: u64,
    pub amount_b: u64,
    pub admin: Pubkey,
    pub lp_position_inaccessible: bool,
    pub timestamp: i64,
}

#[event]
pub struct ProtocolPaused {
    pub admin: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct ProtocolResumed {
    pub admin: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct AdminTransferred {
    pub old_admin: Pubkey,
    pub new_admin: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct KeeperRotated {
    pub old_primary: Pubkey,
    pub new_primary: Pubkey,
    pub old_secondary: Pubkey,
    pub new_secondary: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct FeeRateChanged {
    pub old_bps: u16,
    pub new_bps: u16,
    pub effective_at: i64,
    pub timestamp: i64,
}

// --- Treasury ---

#[event]
pub struct TreasuryWithdrawn {
    pub amount: u64,
    pub destination: Pubkey,
    pub admin: Pubkey,
    pub timestamp: i64,
}
