use anchor_lang::prelude::*;

/// Payload-bearing action enum used by `execute_action_raydium`.
///
/// Same five variants, field names and Borsh order as `OrcaKeeperAction`, so
/// off-chain builders can share the payload shape across the two tick-based
/// protocols. `OpenPosition` takes ticks; `Increase` / `Decrease` take a raw
/// `liquidity_amount` (u128 position liquidity) plus token min/max bounds for
/// slippage protection. Token A / B are the pool's mint 0 / mint 1.
///
/// Borsh serialises as `tag: u8 + payload`. **No `#[repr(u8)]`** — Rust forbids
/// it on data-bearing enums. This enum is an instruction parameter only (never
/// embedded in account state), so it does not affect any `*::LEN` invariant.
///
/// Variant order is part of the wire format — Borsh tags follow declaration
/// order (0 = OpenPosition, 1 = IncreaseLiquidity, 2 = DecreaseLiquidity,
/// 3 = CollectFees, 4 = ClosePosition). New variants must be appended to
/// preserve compatibility with existing off-chain clients.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum RaydiumKeeperAction {
    /// Open an **empty** position over `[tick_lower, tick_upper]`: the NFT is
    /// minted into the vault PDA's Token-2022 ATA and no tokens move. Fund it
    /// afterwards with `IncreaseLiquidity`. The vault must have no active
    /// position. Missing tick arrays are created by Raydium at the user's
    /// expense.
    OpenPosition { tick_lower: i32, tick_upper: i32 },
    /// Add `liquidity_amount` (> 0) to the active position, bounded by
    /// `token_max_a` / `token_max_b` (slippage maxima pulled from the vault
    /// ATAs).
    IncreaseLiquidity {
        liquidity_amount: u128,
        token_max_a: u64,
        token_max_b: u64,
    },
    /// Remove `liquidity_amount` from the active position; `token_min_a` /
    /// `token_min_b` are slippage floors on the returned principal. Raydium
    /// pays out owed fees and rewards together with any decrease, so the LP
    /// fees are collected (and split with the treasury) before the principal
    /// is removed.
    DecreaseLiquidity {
        liquidity_amount: u128,
        token_min_a: u64,
        token_min_b: u64,
    },
    /// Collect accrued LP fees and every initialized reward slot into the
    /// vault ATAs via a zero-liquidity decrease, splitting the LP-fee portion
    /// with the protocol treasury.
    CollectFees,
    /// Burn the empty position NFT and close the position. The caller must
    /// have drained liquidity, fees and rewards first; Raydium aborts on a
    /// non-empty position. Raydium refunds the rent to the vault PDA, which
    /// returns it to the vault owner.
    ClosePosition,
}
