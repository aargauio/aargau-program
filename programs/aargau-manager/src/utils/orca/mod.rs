//! Raw CPI helpers for Orca Whirlpools (concentrated liquidity).
//!
//! Orca's published CPI crate (`whirlpool-cpi`) targets Anchor 0.30 and
//! conflicts with this program's Anchor 1.0 / solana-program 4.0 toolchain, so
//! instead of pulling a git dependency that drifts from the deployed program
//! we build the raw `Instruction` (8-byte discriminator + Borsh args + account
//! metas) and dispatch it with `invoke_signed`. This mirrors the Meteora DLMM
//! integration under `utils/meteora/`.
//!
//! Discriminators and account orderings follow the on-chain Whirlpools
//! `#[derive(Accounts)]` order. The `_v2` modify/collect variants accept
//! Token-2022 pair mints and carry two token programs + the SPL Memo program;
//! the position NFT itself is always a Token-2022 NFT (open/close via the
//! `*_with_token_extensions` instructions, no Metaplex).
//!
//! The vault PDA is the `position_authority` for every liquidity / collect /
//! close CPI and signs via `invoke_signed` with the vault signer seeds. The
//! NFT is custodied in the vault PDA's Token-2022 ATA — holding the NFT is the
//! control invariant for the position.
//!
//! Transfer hooks are not yet wired: for plain Token-2022 (transfer-fee only)
//! the `remaining_accounts_info` Option is serialised as `None` (single `0u8`)
//! and no transfer-hook remaining accounts are appended.

pub mod accounts;
pub mod close_position;
pub mod collect_fees;
pub mod decrease_liquidity;
pub mod increase_liquidity;
pub mod open_position;
pub mod rebalance_helpers;
pub mod whirlpool_view;

pub use accounts::*;
pub use close_position::*;
pub use collect_fees::*;
pub use decrease_liquidity::*;
pub use increase_liquidity::*;
pub use open_position::*;
pub use rebalance_helpers::*;
pub use whirlpool_view::*;
