use crate::{
    constants::{BPS_DIVISOR, BPS_DIVISOR_U16},
    errors::AargauError,
    events::WithdrawMade,
    state::VaultAccount,
    utils::signer_seeds::vault_signer_seeds,
};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    self, CloseAccount, Mint, TokenAccount, TokenInterface, TransferChecked,
};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct WithdrawParams {
    /// 1–10000 (10000 = 100% = close vault)
    pub pct_bps: u16,
    pub min_amount_a: u64,
    pub min_amount_b: u64,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    /// Vault owner
    #[account(mut)]
    pub user: Signer<'info>,

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

    /// Vault's source token A ATA — owner and mint verified
    #[account(
        mut,
        constraint = vault_token_a.owner == vault.key() @ AargauError::InvalidVaultPda,
        constraint = vault_token_a.mint == mint_a.key() @ AargauError::InvalidPoolMint,
    )]
    pub vault_token_a: InterfaceAccount<'info, TokenAccount>,

    /// Vault's source token B ATA — owner and mint verified
    #[account(
        mut,
        constraint = vault_token_b.owner == vault.key() @ AargauError::InvalidVaultPda,
        constraint = vault_token_b.mint == mint_b.key() @ AargauError::InvalidPoolMint,
    )]
    pub vault_token_b: InterfaceAccount<'info, TokenAccount>,

    /// User's destination token A account — must be owned by vault.user_authority
    #[account(
        mut,
        constraint = user_token_a.owner == vault.user_authority @ AargauError::UnauthorizedUser,
    )]
    pub user_token_a: InterfaceAccount<'info, TokenAccount>,

    /// User's destination token B account — must be owned by vault.user_authority
    #[account(
        mut,
        constraint = user_token_b.owner == vault.user_authority @ AargauError::UnauthorizedUser,
    )]
    pub user_token_b: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<Withdraw>, params: WithdrawParams) -> Result<()> {
    require!(
        params.pct_bps >= 1 && params.pct_bps <= BPS_DIVISOR_U16,
        AargauError::InvalidWithdrawPctBps
    );
    // Reject withdrawals during a pending two-phase rebalance. The old position
    // has been closed (Tx1) and tokens are in the ATAs, but the new position
    // (Tx2) has not yet been opened. Withdrawing here would silently take funds
    // that the rebalance expects to deploy. The user must cancel the pending
    // rebalance first (cancel_pending_rebalance), then withdraw.
    require!(
        ctx.accounts.vault.pending_rebalance.is_none(),
        AargauError::PendingRebalanceExists
    );

    let is_full = params.pct_bps == BPS_DIVISOR_U16;
    let amount_a = proportional_amount(ctx.accounts.vault_token_a.amount, params.pct_bps)?;
    let amount_b = proportional_amount(ctx.accounts.vault_token_b.amount, params.pct_bps)?;

    require!(amount_a >= params.min_amount_a, AargauError::SlippageExceeded);
    require!(amount_b >= params.min_amount_b, AargauError::SlippageExceeded);

    // PDA signer seeds — built via shared utility to keep seed construction
    // in one place across all instructions.
    let vault_key = ctx.accounts.vault.key();
    let user_key_bytes = ctx.accounts.vault.user_authority.to_bytes();
    let pool_key_bytes = ctx.accounts.vault.pool_address.to_bytes();
    let bump_bytes = [ctx.accounts.vault.bump];
    let seeds = vault_signer_seeds(&user_key_bytes, &pool_key_bytes, &bump_bytes);
    let signer: &[&[&[u8]]] = &[&seeds];
    // Anchor 1.0.0: CpiContext takes program_id: Pubkey as first argument
    let token_program_key = ctx.accounts.token_program.key();

    // Vault is currently idle — only tokens in vault ATAs are withdrawn.
    // When position_address is Some, decrease_liquidity CPI will be called first.
    if amount_a > 0 {
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
            amount_a,
            ctx.accounts.mint_a.decimals,
        )?;
    }

    if amount_b > 0 {
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
            amount_b,
            ctx.accounts.mint_b.decimals,
        )?;
    }

    let clock = Clock::get()?;
    emit!(WithdrawMade {
        vault: vault_key,
        amount_a_gross: amount_a,
        amount_b_gross: amount_b,
        amount_a_user_net: amount_a,
        amount_b_user_net: amount_b,
        is_full,
        lp_position_inaccessible: false,
        timestamp: clock.unix_timestamp,
    });

    if is_full {
        close_vault_token_accounts_and_pda(&ctx, token_program_key, signer)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Calculate a proportional amount from a balance using basis-point percentage.
fn proportional_amount(balance: u64, pct_bps: u16) -> Result<u64> {
    let amount = (balance as u128)
        .checked_mul(pct_bps as u128)
        .ok_or(AargauError::Overflow)?
        .checked_div(BPS_DIVISOR as u128)
        .ok_or(AargauError::DivisionByZero)? as u64;
    Ok(amount)
}

/// Close both vault token ATAs and the vault PDA, returning rent to the user.
/// Called only on a full (100%) withdrawal.
fn close_vault_token_accounts_and_pda(
    ctx: &Context<Withdraw>,
    token_program_key: Pubkey,
    signer: &[&[&[u8]]],
) -> Result<()> {
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
