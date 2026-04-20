use crate::{
    errors::AargauError,
    state::{ExecutionPhase, ProtocolConfig, VaultAccount},
};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct ManualRebalanceParams {
    pub phase: ExecutionPhase,
    pub new_tick_lower: i32,
    pub new_tick_upper: i32,
    pub liquidity_amount: u128,
    pub slippage_tolerance_bps: u16,
}

#[derive(Accounts)]
pub struct ManualRebalance<'info> {
    pub user: Signer<'info>,

    #[account(
        seeds = [ProtocolConfig::SEEDS],
        bump = protocol_config.bump,
        constraint = !protocol_config.is_paused @ AargauError::ProgramPaused,
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

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

    pub token_program: Interface<'info, TokenInterface>,
}

/// Stub — reserved for future implementation (requires CPI to Meteora, Orca, Raydium)
pub fn handler(_ctx: Context<ManualRebalance>, _params: ManualRebalanceParams) -> Result<()> {
    Ok(())
}
