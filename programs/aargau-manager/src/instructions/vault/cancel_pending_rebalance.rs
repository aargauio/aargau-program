//! `cancel_pending_rebalance` — emergency recovery for a stuck two-phase
//! rebalance. User-signed.
//!
//! After `start_rebalance_orca` (Tx1) the old position is closed and its tokens
//! live in the vault ATAs, with `pending_rebalance` set. If the open leg (Tx2)
//! never lands, the user clears the pending flag here and the vault returns to
//! idle — the tokens are already in the vault ATAs and immediately withdrawable.
//! No CPI is performed.
//!
//! Deliberately carries NO `protocol_config`: there is no pause gate, mirroring
//! `emergency_withdraw`, so a user can always clear a transient pending state
//! and withdraw even while the protocol is paused.

use crate::{
    errors::AargauError,
    events::RebalanceCancelled,
    state::{TriggeredBy, VaultAccount},
};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct CancelPendingRebalance<'info> {
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [VaultAccount::SEEDS, user.key().as_ref(), vault.pool_address.as_ref()],
        bump = vault.bump,
        constraint = vault.user_authority == user.key() @ AargauError::UnauthorizedUser,
    )]
    pub vault: Account<'info, VaultAccount>,
}

pub fn handler(ctx: Context<CancelPendingRebalance>) -> Result<()> {
    let vault = &mut ctx.accounts.vault;
    require!(
        vault.pending_rebalance.is_some(),
        AargauError::NoPendingRebalance
    );

    vault.pending_rebalance = None;

    let clock = Clock::get()?;
    emit!(RebalanceCancelled {
        vault: vault.key(),
        cancelled_by: TriggeredBy::User,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}
