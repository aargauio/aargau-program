use crate::{errors::AargauError, state::ProtocolConfig};
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct TransferAdminParams {
    pub new_admin: Pubkey,
}

#[derive(Accounts)]
pub struct TransferAdmin<'info> {
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [ProtocolConfig::SEEDS],
        bump = protocol_config.bump,
        constraint = protocol_config.admin_authority == admin.key() @ AargauError::UnauthorizedAdmin,
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,
}

/// Stub — reserved for future implementation.
///
/// When implemented, this MUST be a two-step transfer:
///   1. Admin calls `transfer_admin(new_admin)` → stores `new_admin` in a
///      `pending_admin: Option<Pubkey>` field on `ProtocolConfig` (not yet allocated).
///   2. The new admin calls a separate `accept_admin` instruction, which moves
///      `pending_admin → admin_authority` and clears `pending_admin`.
///
/// Single-step transfer is not acceptable: a typo in `new_admin` would permanently
/// lock the admin role. `ProtocolConfig` must be reallocated to add the
/// `pending_admin` field before this instruction can be safely implemented.
pub fn handler(_ctx: Context<TransferAdmin>, _params: TransferAdminParams) -> Result<()> {
    Ok(())
}
