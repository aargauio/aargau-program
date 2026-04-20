use crate::{
    constants::MAX_METEORA_BINS,
    errors::AargauError,
    events::VaultCreated,
    state::{AutoRebalanceStrategy, Protocol, VaultAccount},
    utils::pool_validation::{
        check_no_non_transferable_extension, read_mint_a, read_mint_b,
        validate_pool_owner_and_discriminator,
    },
};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct CreateVaultParams {
    pub protocol: Protocol,
    pub allowed_ops: u8,
    pub strategy: AutoRebalanceStrategy,
}

#[derive(Accounts)]
pub struct CreateVault<'info> {
    /// User who will own this vault — pays rent
    #[account(mut)]
    pub user: Signer<'info>,

    /// VaultAccount PDA — one per user per pool
    #[account(
        init,
        payer = user,
        space = VaultAccount::LEN,
        seeds = [VaultAccount::SEEDS, user.key().as_ref(), pool.key().as_ref()],
        bump,
    )]
    pub vault: Account<'info, VaultAccount>,

    /// Pool account — validated on-chain (owner, discriminator, mints, NonTransferable)
    /// CHECK: validated manually in handler (4-layer validation); read-only
    pub pool: UncheckedAccount<'info>,

    /// Token A mint — must match pool's mint_a
    pub mint_a: InterfaceAccount<'info, Mint>,

    /// Token B mint — must match pool's mint_b
    pub mint_b: InterfaceAccount<'info, Mint>,

    /// Vault's token A ATA — owned by vault PDA
    #[account(
        init,
        payer = user,
        associated_token::mint = mint_a,
        associated_token::authority = vault,
        associated_token::token_program = token_program,
    )]
    pub vault_token_a: InterfaceAccount<'info, TokenAccount>,

    /// Vault's token B ATA — owned by vault PDA
    #[account(
        init,
        payer = user,
        associated_token::mint = mint_b,
        associated_token::authority = vault,
        associated_token::token_program = token_program,
    )]
    pub vault_token_b: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<CreateVault>, params: CreateVaultParams) -> Result<()> {
    // Reject same-mint pairs — vault ATAs would alias, causing double-accounting.
    // Real CLMM pools never have identical mints, but an adversarial pool account
    // with matching bytes would otherwise pass layer-3 validation.
    require_keys_neq!(
        ctx.accounts.mint_a.key(),
        ctx.accounts.mint_b.key(),
        AargauError::InvalidPoolMint
    );

    validate_pool(
        &ctx.accounts.pool,
        &ctx.accounts.mint_a,
        &ctx.accounts.mint_b,
        params.protocol,
    )?;
    validate_meteora_bin_count(&params)?;

    let uses_token_2022 = detect_token_2022(&ctx.accounts.mint_a, &ctx.accounts.mint_b);
    let clock = Clock::get()?;

    initialize_vault_fields(
        &mut ctx.accounts.vault,
        &ctx.accounts.user,
        &ctx.accounts.pool,
        uses_token_2022,
        ctx.bumps.vault,
        clock.unix_timestamp,
        &params,
    );

    emit!(VaultCreated {
        vault: ctx.accounts.vault.key(),
        user: ctx.accounts.user.key(),
        pool: ctx.accounts.pool.key(),
        protocol: params.protocol,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// Private helpers — each owns exactly one concern
// ---------------------------------------------------------------------------

/// Run all 4 pool validation layers: owner, discriminator, mints, NonTransferable.
fn validate_pool(
    pool: &UncheckedAccount,
    mint_a: &InterfaceAccount<Mint>,
    mint_b: &InterfaceAccount<Mint>,
    protocol: Protocol,
) -> Result<()> {
    // Layer 1 + 2: owner and discriminator
    validate_pool_owner_and_discriminator(pool.as_ref(), protocol)?;

    // Layer 3: mint match against pool data
    {
        let pool_data = pool.try_borrow_data()?;
        let pool_mint_a = read_mint_a(&pool_data, protocol)?;
        let pool_mint_b = read_mint_b(&pool_data, protocol)?;
        require!(pool_mint_a == mint_a.key(), AargauError::InvalidPoolMint);
        require!(pool_mint_b == mint_b.key(), AargauError::InvalidPoolMint);
    }

    // Layer 4: NonTransferable extension check
    check_no_non_transferable_extension(mint_a.as_ref())?;
    check_no_non_transferable_extension(mint_b.as_ref())?;

    Ok(())
}

/// Reject Meteora strategies that exceed the maximum bin count upfront.
fn validate_meteora_bin_count(params: &CreateVaultParams) -> Result<()> {
    if params.protocol == Protocol::Meteora {
        let bins =
            params.strategy.range_bins_above as i32 + params.strategy.range_bins_below as i32;
        require!(bins <= MAX_METEORA_BINS, AargauError::TooManyBins);
    }
    Ok(())
}

/// Return true if either mint lives under the Token-2022 program.
fn detect_token_2022(mint_a: &InterfaceAccount<Mint>, mint_b: &InterfaceAccount<Mint>) -> bool {
    let token_2022_id = anchor_spl::token_2022::ID;
    mint_a.to_account_info().owner == &token_2022_id
        || mint_b.to_account_info().owner == &token_2022_id
}

/// Write every field of a freshly-allocated VaultAccount to its initial value.
///
/// TransferFee fields are stored as zeros; actual extension parsing is added
/// with CPI integrations to avoid realloc of the account layout later.
fn initialize_vault_fields(
    vault: &mut VaultAccount,
    user: &Signer,
    pool: &UncheckedAccount,
    uses_token_2022: bool,
    bump: u8,
    created_at: i64,
    params: &CreateVaultParams,
) {
    vault.user_authority = user.key();
    vault.pool_address = pool.key();
    vault.protocol = params.protocol;
    vault.position_address = None;
    vault.position_mint = None;
    vault.position_range_lower = None;
    vault.position_range_upper = None;
    vault.uses_token_2022 = uses_token_2022;
    vault.token_a_transfer_fee_bps = 0;
    vault.token_a_maximum_fee = 0;
    vault.token_b_transfer_fee_bps = 0;
    vault.token_b_maximum_fee = 0;
    vault.reward_token_0_transfer_fee_bps = 0;
    vault.reward_token_0_maximum_fee = 0;
    vault.reward_token_1_transfer_fee_bps = 0;
    vault.reward_token_1_maximum_fee = 0;
    vault.reward_token_2_transfer_fee_bps = 0;
    vault.reward_token_2_maximum_fee = 0;
    vault.allowed_ops = params.allowed_ops;
    vault.strategy = params.strategy;
    vault.last_rebalance_at = 0;
    vault.rebalances_today = 0;
    vault.gas_spent_today_usd_cents = 0;
    vault.last_day_reset = 0;
    vault.entry_value_usd = 0;
    vault.pending_rebalance = None;
    vault.created_at = created_at;
    vault.bump = bump;
    vault._padding = [0u8; 8];
}
