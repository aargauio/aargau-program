use crate::{errors::AargauError, state::VaultAccount};
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct RetryPendingRebalanceParams {
    pub new_tick_lower: i32,
    pub new_tick_upper: i32,
}

#[derive(Accounts)]
pub struct RetryPendingRebalance<'info> {
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
pub fn handler(
    _ctx: Context<RetryPendingRebalance>,
    _params: RetryPendingRebalanceParams,
) -> Result<()> {
    Ok(())
}
