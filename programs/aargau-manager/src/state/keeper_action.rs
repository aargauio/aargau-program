use anchor_lang::prelude::*;

/// Payload-bearing action enum used by `execute_action`.
///
/// Borsh serialises as `tag: u8 + payload`. **No `#[repr(u8)]`** — Rust forbids
/// it on enums whose variants carry data. This enum is an instruction parameter
/// only (never embedded in account state), so it does not affect any
/// `*::LEN` invariant.
///
/// Variant order is part of the wire format — Borsh tags follow declaration
/// order (0 = CollectFees, 1 = IncreaseLiquidity, 2 = DecreaseLiquidity,
/// 3 = OpenPosition, 4 = ClosePosition). New variants must be appended at
/// the end to preserve compatibility with existing off-chain clients.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeeperAction {
    /// Claim accrued LP fees, split with the protocol treasury.
    CollectFees,
    /// Push more `token_a` / `token_b` into the active position. Both amounts
    /// are upper bounds; the underlying CPI may consume less.
    ///
    /// `active_id_slippage` bounds how many bins the on-chain `active_id` may
    /// have moved from the keeper's quote between simulation and execution.
    /// The handler enforces `active_id_slippage <= MAX_ACTIVE_BIN_SLIPPAGE_HARD_CAP`
    /// to prevent a caller from disabling slippage protection. Typical
    /// values: 3 (calm market) to 20 (volatile rebalance).
    IncreaseLiquidity {
        amount_a_max: u64,
        amount_b_max: u64,
        active_id_slippage: u16,
    },
    /// Pull `bps` (1–10000) of the position's liquidity back into the vault
    /// ATAs. Fees stay in the position and are recovered on the next
    /// `CollectFees`.
    DecreaseLiquidity { bps: u16 },
    /// Initialise a fresh `PositionV2` owned by the vault PDA over
    /// `[lower_bin_id, upper_bin_id]`. The CPI helper converts to Meteora's
    /// `width` convention internally — callers always pass the inclusive
    /// upper bound. The vault must have no active position
    /// (`vault.position_address == None`); on success it is set to the
    /// ephemeral position keypair and `position_range_lower/upper` are
    /// recorded.
    OpenPosition {
        lower_bin_id: i32,
        upper_bin_id: i32,
    },
    /// Retire the active `PositionV2` via `close_position_if_empty`. The
    /// caller is responsible for first draining all liquidity (typically
    /// `DecreaseLiquidity { bps: 10_000 }` + `CollectFees`); the on-chain
    /// handler aborts if any per-bin liquidity share is non-zero. Rent
    /// returns to the vault owner.
    ClosePosition,
}
