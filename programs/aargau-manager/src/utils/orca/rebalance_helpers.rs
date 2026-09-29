//! Shared glue helpers for the Orca Whirlpools instruction handlers
//! (`execute_action_orca`, `start_rebalance_orca`, `retry_pending_rebalance`).
//!
//! These three handlers all bind tick-array accounts to a range. The logic is
//! identical across the three, so it lives here once instead of being
//! copy-pasted per file.

use crate::{
    errors::AargauError,
    utils::orca::accounts::{derive_tick_array_pda, tick_array_start_index},
};
use anchor_lang::prelude::*;

/// Bind a passed tick-array account to the PDA covering `tick` for the given
/// whirlpool and tick spacing. Rejects a mismatch with `InvalidPool`.
pub fn require_tick_array(
    whirlpool: &Pubkey,
    tick: i32,
    tick_spacing: u16,
    passed: &Pubkey,
) -> Result<()> {
    let start = tick_array_start_index(tick, tick_spacing)?;
    let expected = derive_tick_array_pda(whirlpool, start);
    require_keys_eq!(*passed, expected, AargauError::InvalidPool);
    Ok(())
}
