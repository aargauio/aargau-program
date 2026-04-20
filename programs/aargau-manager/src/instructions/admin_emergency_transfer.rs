use crate::{
    errors::AargauError,
    state::{ProtocolConfig, VaultAccount},
};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

#[derive(Accounts)]
pub struct AdminEmergencyTransfer<'info> {
    pub admin: Signer<'info>,

    #[account(
        seeds = [ProtocolConfig::SEEDS],
        bump = protocol_config.bump,
        constraint = protocol_config.admin_authority == admin.key() @ AargauError::UnauthorizedAdmin,
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    #[account(
        mut,
        seeds = [VaultAccount::SEEDS, vault.user_authority.as_ref(), vault.pool_address.as_ref()],
        bump = vault.bump,
    )]
    pub vault: Account<'info, VaultAccount>,

    pub mint_a: InterfaceAccount<'info, Mint>,
    pub mint_b: InterfaceAccount<'info, Mint>,

    /// Vault's token A ATA — owner and mint verified; no arbitrary drain
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

    /// Destination MUST be vault.user_authority — hard-coded destination, no override
    #[account(
        mut,
        constraint = user_token_a.owner == vault.user_authority @ AargauError::UnauthorizedUser,
    )]
    pub user_token_a: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        constraint = user_token_b.owner == vault.user_authority @ AargauError::UnauthorizedUser,
    )]
    pub user_token_b: InterfaceAccount<'info, TokenAccount>,

    /// CHECK: must be vault.user_authority for rent return
    #[account(
        mut,
        constraint = user_authority.key() == vault.user_authority @ AargauError::UnauthorizedUser,
    )]
    pub user_authority: UncheckedAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

/// Stub — reserved for future implementation (admin multisig + timelock enforcement)
pub fn handler(_ctx: Context<AdminEmergencyTransfer>) -> Result<()> {
    Ok(())
}
