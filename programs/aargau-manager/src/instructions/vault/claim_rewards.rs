use crate::{errors::AargauError, state::VaultAccount};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{TokenAccount, TokenInterface};

#[derive(Accounts)]
pub struct ClaimRewards<'info> {
    pub user: Signer<'info>,

    #[account(
        seeds = [VaultAccount::SEEDS, user.key().as_ref(), vault.pool_address.as_ref()],
        bump = vault.bump,
        constraint = vault.user_authority == user.key() @ AargauError::UnauthorizedUser,
    )]
    pub vault: Account<'info, VaultAccount>,

    /// Vault's token A ATA — must be owned by vault PDA
    #[account(
        mut,
        constraint = vault_token_a.owner == vault.key() @ AargauError::InvalidVaultPda,
    )]
    pub vault_token_a: InterfaceAccount<'info, TokenAccount>,

    /// Vault's token B ATA — must be owned by vault PDA
    #[account(
        mut,
        constraint = vault_token_b.owner == vault.key() @ AargauError::InvalidVaultPda,
    )]
    pub vault_token_b: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
}

/// Stub — reserved for future implementation (requires CPI to protocols)
pub fn handler(_ctx: Context<ClaimRewards>) -> Result<()> {
    Ok(())
}
