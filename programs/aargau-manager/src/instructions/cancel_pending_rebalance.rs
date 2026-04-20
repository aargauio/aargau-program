use crate::{errors::AargauError, state::VaultAccount};
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

/// Stub — reserved for future implementation (two-phase rebalance for Orca/Raydium)
pub fn handler(_ctx: Context<CancelPendingRebalance>) -> Result<()> {
    Ok(())
}
