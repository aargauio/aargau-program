use crate::{errors::AargauError, state::VaultAccount};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct CloseVaultParams {
    pub min_amount_a: u64,
    pub min_amount_b: u64,
}

#[derive(Accounts)]
pub struct CloseVault<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [VaultAccount::SEEDS, user.key().as_ref(), vault.pool_address.as_ref()],
        bump = vault.bump,
        constraint = vault.user_authority == user.key() @ AargauError::UnauthorizedUser,
    )]
    pub vault: Account<'info, VaultAccount>,

    pub mint_a: InterfaceAccount<'info, Mint>,
    pub mint_b: InterfaceAccount<'info, Mint>,

    /// Vault's token A ATA — owner and mint verified
    #[account(
        mut,
        constraint = vault_token_a.owner == vault.key() @ AargauError::InvalidVaultPda,
        constraint = vault_token_a.mint == mint_a.key() @ AargauError::InvalidPoolMint,
    )]
    pub vault_token_a: InterfaceAccount<'info, TokenAccount>,

    /// Vault's token B ATA — owner and mint verified
    #[account(
        mut,
        constraint = vault_token_b.owner == vault.key() @ AargauError::InvalidVaultPda,
        constraint = vault_token_b.mint == mint_b.key() @ AargauError::InvalidPoolMint,
    )]
    pub vault_token_b: InterfaceAccount<'info, TokenAccount>,

    /// Destination for token A — must be owned by user_authority
    #[account(
        mut,
        constraint = user_token_a.owner == vault.user_authority @ AargauError::UnauthorizedUser,
    )]
    pub user_token_a: InterfaceAccount<'info, TokenAccount>,

    /// Destination for token B — must be owned by user_authority
    #[account(
        mut,
        constraint = user_token_b.owner == vault.user_authority @ AargauError::UnauthorizedUser,
    )]
    pub user_token_b: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

/// Stub — reserved for future implementation (requires CPI to protocols)
pub fn handler(_ctx: Context<CloseVault>, _params: CloseVaultParams) -> Result<()> {
    Ok(())
}
