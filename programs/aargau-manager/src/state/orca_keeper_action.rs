use anchor_lang::prelude::*;

/// Payload-bearing action enum used by `execute_action_orca`.
///
/// Orca's concentrated-liquidity model is tick-based and NFT-backed, so it
/// needs a distinct action enum from the Meteora `KeeperAction` (which carries
/// bin ids / `active_id_slippage`). `OpenPosition` takes ticks; `Increase` /
/// `Decrease` take a raw `liquidity_amount` (Q-less u128 position liquidity)
/// plus token min/max bounds for slippage protection.
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
pub enum OrcaKeeperAction {
    /// Open a fresh position over `[tick_lower, tick_upper]`. The Whirlpools
    /// program mints a Token-2022 NFT into the vault PDA's ATA and initialises
    /// the `Position` PDA. The vault must have no active position. Tick arrays
    /// covering both ticks must be pre-initialised by the caller (off-chain).
    OpenPosition { tick_lower: i32, tick_upper: i32 },
    /// Add `liquidity_amount` to the active position, bounded by `token_max_a`
    /// / `token_max_b` (slippage maxima pulled from the vault ATAs).
    IncreaseLiquidity {
        liquidity_amount: u128,
        token_max_a: u64,
        token_max_b: u64,
    },
    /// Remove `liquidity_amount` from the active position; `token_min_a` /
    /// `token_min_b` are slippage floors the CPI must return into the vault
    /// ATAs.
    DecreaseLiquidity {
        liquidity_amount: u128,
        token_min_a: u64,
        token_min_b: u64,
    },
    /// Sweep accrued LP fees (`collect_fees_v2`) and every active reward slot
    /// (`collect_reward_v2`) into the vault ATAs, splitting the LP-fee portion
    /// with the protocol treasury.
    CollectFees,
    /// Burn the empty position NFT and close the position
    /// (`close_position_with_token_extensions`). The caller must have drained
    /// liquidity and swept fees/rewards first; the Whirlpools program aborts on
    /// a non-empty position. Rent returns to the vault owner.
    ClosePosition,
}
