use crate::{
    errors::AargauError,
    state::{ActionType, ExecutionPhase, ProtocolConfig, VaultAccount},
};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{TokenAccount, TokenInterface};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct ExecuteActionParams {
    pub action: ActionType,
    pub phase: ExecutionPhase,
    pub new_tick_lower: i32,
    pub new_tick_upper: i32,
    pub liquidity_amount: u128,
    pub slippage_tolerance_bps: u16,
    pub estimated_cost_usd_cents: u64,
    pub estimated_benefit_usd_cents: u64,
    pub current_price_bps: u64,
}

/// Dual-authority keeper instruction — both keeper accounts must sign.
#[derive(Accounts)]
pub struct ExecuteAction<'info> {
    /// Primary keeper authority
    pub primary_keeper: Signer<'info>,

    /// Secondary keeper authority — dual-authority required
    pub secondary_keeper: Signer<'info>,

    #[account(
        seeds = [ProtocolConfig::SEEDS],
        bump = protocol_config.bump,
        constraint = !protocol_config.is_paused @ AargauError::ProgramPaused,
        constraint = protocol_config.primary_keeper_authority == primary_keeper.key() @ AargauError::UnauthorizedKeeper,
        constraint = protocol_config.secondary_keeper_authority == secondary_keeper.key() @ AargauError::UnauthorizedKeeper,
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    #[account(
        mut,
        seeds = [VaultAccount::SEEDS, vault.user_authority.as_ref(), vault.pool_address.as_ref()],
        bump = vault.bump,
    )]
    pub vault: Account<'info, VaultAccount>,

    /// Vault's token A ATA — must be owned by vault PDA; prevents keeper draining arbitrary accounts
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

/// Stub — reserved for future implementation
pub fn handler(_ctx: Context<ExecuteAction>, _params: ExecuteActionParams) -> Result<()> {
    Ok(())
}
