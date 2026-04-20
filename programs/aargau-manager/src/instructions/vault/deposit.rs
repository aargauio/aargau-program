use crate::{
    errors::AargauError,
    events::DepositMade,
    state::{ProtocolConfig, VaultAccount},
};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct DepositParams {
    pub amount_a: u64,
    pub amount_b: u64,
    /// Slippage protection for increase_liquidity when vault has an active position.
    /// Reserved — not used when vault is idle (no active position).
    pub min_liquidity: u128,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    /// Vault owner — must match vault.user_authority
    pub user: Signer<'info>,

    /// Deposits are blocked while the protocol is paused
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

    /// Token A mint (used for checked transfer)
    pub mint_a: InterfaceAccount<'info, Mint>,
    /// Token B mint (used for checked transfer)
    pub mint_b: InterfaceAccount<'info, Mint>,

    /// User's source token A account — must be owned by the signer
    #[account(
        mut,
        constraint = user_token_a.owner == user.key() @ AargauError::UnauthorizedUser,
    )]
    pub user_token_a: InterfaceAccount<'info, TokenAccount>,
    /// User's source token B account — must be owned by the signer
    #[account(
        mut,
        constraint = user_token_b.owner == user.key() @ AargauError::UnauthorizedUser,
    )]
    pub user_token_b: InterfaceAccount<'info, TokenAccount>,

    /// Vault's destination token A ATA — owner and mint verified
    #[account(
        mut,
        constraint = vault_token_a.owner == vault.key() @ AargauError::InvalidVaultPda,
        constraint = vault_token_a.mint == mint_a.key() @ AargauError::InvalidPoolMint,
    )]
    pub vault_token_a: InterfaceAccount<'info, TokenAccount>,

    /// Vault's destination token B ATA — owner and mint verified
    #[account(
        mut,
        constraint = vault_token_b.owner == vault.key() @ AargauError::InvalidVaultPda,
        constraint = vault_token_b.mint == mint_b.key() @ AargauError::InvalidPoolMint,
    )]
    pub vault_token_b: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handler(ctx: Context<Deposit>, params: DepositParams) -> Result<()> {
    require!(
        params.amount_a > 0 || params.amount_b > 0,
        AargauError::DepositAmountZero
    );

    // Reject deposits during a pending two-phase rebalance (Tx1 complete, Tx2 pending).
    // Tokens deposited at this point would not be included in the incoming position
    // and could be stranded if the rebalance is later cancelled.
    require!(
        ctx.accounts.vault.pending_rebalance.is_none(),
        AargauError::PendingRebalanceExists
    );

    // Tokens are deposited into vault ATAs only.
    // When CPI integrations are added, an active position_address will trigger
    // increase_liquidity to route tokens directly into the position.

    let clock = Clock::get()?;

    if params.amount_a > 0 {
        let cpi_accounts = TransferChecked {
            from: ctx.accounts.user_token_a.to_account_info(),
            mint: ctx.accounts.mint_a.to_account_info(),
            to: ctx.accounts.vault_token_a.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        };
        // Anchor 1.0.0: CpiContext::new takes program_id: Pubkey (not AccountInfo)
        let cpi_ctx = CpiContext::new(ctx.accounts.token_program.key(), cpi_accounts);
        token_interface::transfer_checked(cpi_ctx, params.amount_a, ctx.accounts.mint_a.decimals)?;
    }

    if params.amount_b > 0 {
        let cpi_accounts = TransferChecked {
            from: ctx.accounts.user_token_b.to_account_info(),
            mint: ctx.accounts.mint_b.to_account_info(),
            to: ctx.accounts.vault_token_b.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(ctx.accounts.token_program.key(), cpi_accounts);
        token_interface::transfer_checked(cpi_ctx, params.amount_b, ctx.accounts.mint_b.decimals)?;
    }

    emit!(DepositMade {
        vault: ctx.accounts.vault.key(),
        amount_a: params.amount_a,
        amount_b: params.amount_b,
        entry_value_usd_delta: 0, // reserved: requires oracle price integration
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}
