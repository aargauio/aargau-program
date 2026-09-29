//! Raw CPI helpers for Raydium CLMM (concentrated liquidity).
//!
//! Like the Orca and Meteora integrations, no protocol SDK crate is pulled
//! in: each builder assembles the raw `Instruction` (8-byte Anchor
//! discriminator + Borsh args + account metas in the on-chain
//! `#[derive(Accounts)]` order) and dispatches it with `invoke` /
//! `invoke_signed`.
//!
//! Custody: the position NFT is a Token-2022 NFT minted into the vault PDA's
//! ATA. The vault PDA is `nft_owner` for increase / decrease / close and
//! signs with the vault signer seeds; open is paid and signed by the user
//! only.
//!
//! Lifecycle differences from Orca that callers must handle:
//! - open is always empty; liquidity is added with `increase_liquidity_v2`;
//! - `decrease_liquidity_v2` pays out every fee and reward owed (a
//!   zero-liquidity decrease is the collect) and needs one reward triple per
//!   initialized reward slot;
//! - `close_position` refunds all rent to the vault PDA, which the caller
//!   sweeps back to the user;
//! - the tick-array bitmap extension is always forwarded.
//!
//! `vault_position` composes the builders with the custodial checks that must
//! travel with them (fee split, min-out, max-in); `position_guards` holds the
//! pure rules every Raydium instruction shares.

pub mod accounts;
pub mod close_position;
pub mod decrease_liquidity;
pub mod increase_liquidity;
pub mod lp_fee;
pub mod open_position;
pub mod personal_position_view;
pub mod pool_state_view;
pub mod position_guards;
pub mod reward_accounts;
pub mod vault_position;

pub use accounts::*;
pub use close_position::*;
pub use decrease_liquidity::*;
pub use increase_liquidity::*;
pub use lp_fee::*;
pub use open_position::*;
pub use personal_position_view::*;
pub use pool_state_view::*;
pub use position_guards::*;
pub use reward_accounts::*;
pub use vault_position::*;
