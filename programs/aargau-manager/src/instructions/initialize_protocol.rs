use crate::{
    constants::PROGRAM_VERSION,
    errors::AargauError,
    events::ProtocolInitialized,
    state::ProtocolConfig,
};
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct InitializeProtocolParams {
    pub admin_authority: Pubkey,
    pub primary_keeper_authority: Pubkey,
    pub secondary_keeper_authority: Pubkey,
    pub fee_rate_bps: u16,
}

#[derive(Accounts)]
pub struct InitializeProtocol<'info> {
    /// The deployer / first admin — pays for the PDA rent
    #[account(mut)]
    pub payer: Signer<'info>,

    /// ProtocolConfig singleton PDA — can only be created once
    #[account(
        init,
        payer = payer,
        space = ProtocolConfig::LEN,
        seeds = [ProtocolConfig::SEEDS],
        bump,
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<InitializeProtocol>, params: InitializeProtocolParams) -> Result<()> {
    // Validate inputs — reject zero pubkeys before writing them to the account
    require!(
        params.admin_authority != Pubkey::default(),
        AargauError::InvalidPublicKey
    );
    require!(
        params.primary_keeper_authority != Pubkey::default(),
        AargauError::InvalidPublicKey
    );
    require!(
        params.secondary_keeper_authority != Pubkey::default(),
        AargauError::InvalidPublicKey
    );
    require!(
        params.primary_keeper_authority != params.secondary_keeper_authority,
        AargauError::DuplicateKeeperAuthority
    );
    require!(
        params.fee_rate_bps <= ProtocolConfig::MAX_FEE_RATE_BPS,
        AargauError::FeeRateTooHigh
    );

    let config = &mut ctx.accounts.protocol_config;
    config.admin_authority = params.admin_authority;
    config.primary_keeper_authority = params.primary_keeper_authority;
    config.secondary_keeper_authority = params.secondary_keeper_authority;
    config.is_paused = false;
    config.fee_rate_bps = params.fee_rate_bps;
    config.program_version = PROGRAM_VERSION;
    config.bump = ctx.bumps.protocol_config;

    let clock = Clock::get()?;
    emit!(ProtocolInitialized {
        admin: params.admin_authority,
        primary_keeper: params.primary_keeper_authority,
        secondary_keeper: params.secondary_keeper_authority,
        fee_rate_bps: params.fee_rate_bps,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}
