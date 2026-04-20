use crate::{
    errors::AargauError,
    state::{ProtocolConfig, VaultAccount},
};
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct ReportRebalanceAttemptParams {
    /// 0=none 1=slippage 2=funds 3=compute 4=range 5=other
    pub failure_reason: u8,
}

/// Dual-authority keeper instruction — both keeper accounts must sign.
#[derive(Accounts)]
pub struct ReportRebalanceAttempt<'info> {
    pub primary_keeper: Signer<'info>,
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
}

/// Stub — reserved for future implementation
pub fn handler(
    _ctx: Context<ReportRebalanceAttempt>,
    _params: ReportRebalanceAttemptParams,
) -> Result<()> {
    Ok(())
}
