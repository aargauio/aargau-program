use crate::{
    errors::AargauError,
    events::{ProtocolPaused, ProtocolResumed},
    state::ProtocolConfig,
};
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct SetProtocolPauseParams {
    pub paused: bool,
}

#[derive(Accounts)]
pub struct SetProtocolPause<'info> {
    /// Must be admin_authority
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [ProtocolConfig::SEEDS],
        bump = protocol_config.bump,
        constraint = protocol_config.admin_authority == admin.key() @ AargauError::UnauthorizedAdmin,
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,
}

pub fn handler(ctx: Context<SetProtocolPause>, params: SetProtocolPauseParams) -> Result<()> {
    ctx.accounts.protocol_config.is_paused = params.paused;

    let clock = Clock::get()?;
    let admin_key = ctx.accounts.admin.key();

    if params.paused {
        emit!(ProtocolPaused {
            admin: admin_key,
            timestamp: clock.unix_timestamp,
        });
    } else {
        emit!(ProtocolResumed {
            admin: admin_key,
            timestamp: clock.unix_timestamp,
        });
    }

    Ok(())
}
