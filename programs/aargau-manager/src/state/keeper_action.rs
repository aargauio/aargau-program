use anchor_lang::prelude::*;

/// Payload-bearing action enum used by `execute_action`.
///
/// Borsh serialises as `tag: u8 + payload`. **No `#[repr(u8)]`** — Rust forbids
/// it on enums whose variants carry data. This enum is an instruction parameter
/// only (never embedded in account state), so it does not affect any
/// `*::LEN` invariant.
///
/// Variants intentionally form a superset of `ActionType` (the legacy enum is
/// still used by other reserved code paths). Adding `OpenPosition` /
/// `ClosePosition` here is a non-breaking extension.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeeperAction {
    /// Claim accrued LP fees, split with the protocol treasury.
    CollectFees,
    /// Push more `token_a` / `token_b` into the active position. Both amounts
    /// are upper bounds; the underlying CPI may consume less.
    IncreaseLiquidity {
        amount_a_max: u64,
        amount_b_max: u64,
    },
    /// Pull `bps` (1–10000) of the position's liquidity back into the vault
    /// ATAs. Fees stay in the position and are recovered on the next
    /// `CollectFees`.
    DecreaseLiquidity { bps: u16 },
}
