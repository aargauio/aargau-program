use crate::{errors::AargauError, state::ProtocolConfig};
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct UpdateProtocolConfigParams {
    pub primary_keeper_authority: Option<Pubkey>,
    pub secondary_keeper_authority: Option<Pubkey>,
    pub fee_rate_bps: Option<u16>,
}

#[derive(Accounts)]
pub struct UpdateProtocolConfig<'info> {
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [ProtocolConfig::SEEDS],
        bump = protocol_config.bump,
        constraint = protocol_config.admin_authority == admin.key() @ AargauError::UnauthorizedAdmin,
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,
}

/// Stub — reserved for future implementation
pub fn handler(
    _ctx: Context<UpdateProtocolConfig>,
    _params: UpdateProtocolConfigParams,
) -> Result<()> {
    Ok(())
}
