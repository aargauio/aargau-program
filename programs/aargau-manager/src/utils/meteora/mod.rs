//! Raw CPI helpers for Meteora DLMM.
//!
//! Meteora does not publish a stable on-chain CPI crate. Instead of pulling a
//! git dependency that drifts from the deployed program, we build the raw
//! `Instruction` (discriminator + Borsh args + account metas) and invoke it
//! with `invoke_signed`. Discriminators and account orderings mirror the
//! Meteora DLMM IDL.
//!
//! All helpers assume **SPL Token classic** (no Token-2022 transfer hooks).
//! Hook support is not yet wired; the
//! `remaining_accounts_info` Borsh tail is always serialised as
//! `{ slices: [{type: 0, len: 0}, {type: 1, len: 0}] }`.

pub mod accounts;
pub mod add_liquidity;
pub mod claim_fee;
pub mod close_position;
pub mod lb_pair_view;
pub mod open_position;
pub mod position_view;
pub mod rebalance_liquidity;
pub mod remove_liquidity;

pub use accounts::*;
pub use add_liquidity::*;
pub use claim_fee::*;
pub use close_position::*;
pub use lb_pair_view::*;
pub use open_position::*;
pub use position_view::*;
pub use rebalance_liquidity::*;
pub use remove_liquidity::*;
