use crate::{
    errors::AargauError,
    events::EmergencyWithdraw as EmergencyWithdrawEvent,
    state::VaultAccount,
    utils::signer_seeds::vault_signer_seeds,
};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    self, CloseAccount, Mint, TokenAccount, TokenInterface, TransferChecked,
};

/// HARD REQUIREMENT: This instruction NEVER checks `is_paused`.
/// It is always available regardless of protocol pause state.
/// The only check that cannot be bypassed is `vault.user_authority == user.key()`.
///
/// protocol_config is NOT included in this struct — no is_paused constraint possible.
#[derive(Accounts)]
pub struct EmergencyWithdraw<'info> {
    /// Vault owner — must match vault.user_authority
    #[account(mut)]
    pub user: Signer<'info>,

    /// CRITICAL: no protocol_config account here — cannot be gated by is_paused
    #[account(
        mut,
        seeds = [VaultAccount::SEEDS, user.key().as_ref(), vault.pool_address.as_ref()],
        bump = vault.bump,
        constraint = vault.user_authority == user.key() @ AargauError::UnauthorizedUser,
    )]
    pub vault: Account<'info, VaultAccount>,

    /// Token A mint
    pub mint_a: InterfaceAccount<'info, Mint>,
    /// Token B mint
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

    /// Destination for token A — MUST be user_authority's ATA (hardcoded destination)
    #[account(
        mut,
        constraint = user_token_a.owner == vault.user_authority @ AargauError::UnauthorizedUser,
    )]
    pub user_token_a: InterfaceAccount<'info, TokenAccount>,

    /// Destination for token B — MUST be user_authority's ATA
    #[account(
        mut,
        constraint = user_token_b.owner == vault.user_authority @ AargauError::UnauthorizedUser,
    )]
    pub user_token_b: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<EmergencyWithdraw>) -> Result<()> {
    // No active LP position in scope — lp_position_inaccessible is always false here.
    // When CPI integrations are added: attempt decrease_liquidity + close_position;
    // if CPI fails, set lp_position_inaccessible = true and continue regardless.
    let lp_position_inaccessible = false;

    let balance_a = ctx.accounts.vault_token_a.amount;
    let balance_b = ctx.accounts.vault_token_b.amount;
    let vault_key = ctx.accounts.vault.key();
    let user_key = ctx.accounts.vault.user_authority;

    // PDA signer seeds — built via shared utility
    let user_key_bytes = ctx.accounts.vault.user_authority.to_bytes();
    let pool_key_bytes = ctx.accounts.vault.pool_address.to_bytes();
    let bump_bytes = [ctx.accounts.vault.bump];
    let seeds = vault_signer_seeds(&user_key_bytes, &pool_key_bytes, &bump_bytes);
    let signer: &[&[&[u8]]] = &[&seeds];
    // Anchor 1.0.0: CpiContext takes program_id: Pubkey
    let token_program_key = ctx.accounts.token_program.key();

    transfer_all_tokens_to_user(&ctx, token_program_key, balance_a, balance_b, signer)?;

    let clock = Clock::get()?;
    emit!(EmergencyWithdrawEvent {
        vault: vault_key,
        user: user_key,
        amount_a: balance_a,
        amount_b: balance_b,
        fee_skipped: true, // emergency_withdraw always skips Aargau fee
        lp_position_inaccessible,
        timestamp: clock.unix_timestamp,
    });

    // Emit event before closing so listeners see the final balances first.
    // Close both token ATAs and the vault PDA — rent returned to user.
    let vault_info = ctx.accounts.vault.to_account_info();
    let user_info = ctx.accounts.user.to_account_info();

    token_interface::close_account(CpiContext::new_with_signer(
        token_program_key,
        CloseAccount {
            account: ctx.accounts.vault_token_a.to_account_info(),
            destination: user_info.clone(),
            authority: vault_info.clone(),
        },
        signer,
    ))?;

    token_interface::close_account(CpiContext::new_with_signer(
        token_program_key,
        CloseAccount {
            account: ctx.accounts.vault_token_b.to_account_info(),
            destination: user_info.clone(),
            authority: vault_info,
        },
        signer,
    ))?;

    ctx.accounts.vault.close(user_info)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Transfer the full idle balances of both tokens from the vault ATAs to the user.
fn transfer_all_tokens_to_user(
    ctx: &Context<EmergencyWithdraw>,
    token_program_key: Pubkey,
    balance_a: u64,
    balance_b: u64,
    signer: &[&[&[u8]]],
) -> Result<()> {
    if balance_a > 0 {
        token_interface::transfer_checked(
            CpiContext::new_with_signer(
                token_program_key,
                TransferChecked {
                    from: ctx.accounts.vault_token_a.to_account_info(),
                    mint: ctx.accounts.mint_a.to_account_info(),
                    to: ctx.accounts.user_token_a.to_account_info(),
                    authority: ctx.accounts.vault.to_account_info(),
                },
                signer,
            ),
            balance_a,
            ctx.accounts.mint_a.decimals,
        )?;
    }

    if balance_b > 0 {
        token_interface::transfer_checked(
            CpiContext::new_with_signer(
                token_program_key,
                TransferChecked {
                    from: ctx.accounts.vault_token_b.to_account_info(),
                    mint: ctx.accounts.mint_b.to_account_info(),
                    to: ctx.accounts.user_token_b.to_account_info(),
                    authority: ctx.accounts.vault.to_account_info(),
                },
                signer,
            ),
            balance_b,
            ctx.accounts.mint_b.decimals,
        )?;
    }

    Ok(())
}
