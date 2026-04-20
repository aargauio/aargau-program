use crate::{errors::AargauError, state::ProtocolConfig};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{TokenAccount, TokenInterface};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct WithdrawTreasuryParams {
    /// Amount to withdraw; u64::MAX means "all available"
    pub amount: u64,
}

#[derive(Accounts)]
pub struct WithdrawTreasury<'info> {
    pub admin: Signer<'info>,

    #[account(
        seeds = [ProtocolConfig::SEEDS],
        bump = protocol_config.bump,
        constraint = protocol_config.admin_authority == admin.key() @ AargauError::UnauthorizedAdmin,
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    /// CHECK: treasury PDA — validated by seeds constraint
    #[account(
        seeds = [b"treasury", protocol_config.key().as_ref()],
        bump,
    )]
    pub treasury_pda: UncheckedAccount<'info>,

    /// Source token account must be owned by the treasury PDA — prevents arbitrary drain
    #[account(
        mut,
        constraint = treasury_token_source.owner == treasury_pda.key() @ AargauError::InvalidTreasury,
    )]
    pub treasury_token_source: InterfaceAccount<'info, TokenAccount>,
    #[account(mut)]
    pub admin_token_dest: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
}

/// Stub — reserved for future implementation
pub fn handler(_ctx: Context<WithdrawTreasury>, _params: WithdrawTreasuryParams) -> Result<()> {
    Ok(())
}
