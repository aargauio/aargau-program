//! `retry_pending_rebalance` — phase two of a two-transaction Orca Whirlpools
//! rebalance (OpenNew). User-signed; the vault PDA is the position authority.
//!
//! Completes a rebalance started by `start_rebalance_orca`: opens a NEW
//! position over the range stored in `vault.pending_rebalance` and funds it via
//! `increase_liquidity`, then clears the pending state. The new range comes
//! STRICTLY from `pending_rebalance` — the caller supplies only the amounts, so
//! the target cannot be retargeted on retry (idempotency invariant).
//!
//! The same instruction serves the first OpenNew attempt and every subsequent
//! retry. `open_position` + `increase_liquidity` are atomic: a failed attempt
//! persists no new position, so a retry with a freshly generated mint is safe.
//! **Each retry MUST use a fresh ephemeral `position_mint` keypair** — reusing
//! the prior attempt's mint would collide on the position PDA derivation. Once
//! the open leg succeeds and `pending_rebalance` is cleared, a subsequent call
//! hits `NoPendingRebalance` (re-running after success is a no-op error).

use crate::{
    constants::{
        MEMO_PROGRAM_ID, ORCA_METADATA_UPDATE_AUTH, ORCA_WHIRLPOOL_PROGRAM_ID,
        TOKEN_2022_PROGRAM_ID,
    },
    errors::AargauError,
    events::RebalanceExecuted,
    state::{Protocol, ProtocolConfig, TriggeredBy, VaultAccount},
    utils::orca::{
        accounts::{
            derive_position_pda, is_token_2022, read_transfer_fee_config,
            require_v2_transferable_mint,
        },
        increase_liquidity::{invoke_increase_liquidity_v2, IncreaseLiquidityV2Cpi},
        open_position::{
            invoke_open_position_with_token_extensions, OpenPositionWithTokenExtensionsCpi,
        },
        rebalance_helpers::{require_tick_array, VaultSignerSeedBytes},
        whirlpool_view::{
            parse_whirlpool_view_from_bytes, require_whirlpool_bindings, WhirlpoolView,
        },
    },
};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct RetryPendingRebalanceParams {
    /// Liquidity to add to the freshly opened position. Computed off-chain
    /// against the current pool `sqrt_price`; only the range is fixed by the
    /// stored `pending_rebalance`.
    pub liquidity_amount: u128,
    /// Slippage ceiling for token A consumed by the increase.
    pub token_max_a: u64,
    /// Slippage ceiling for token B consumed by the increase.
    pub token_max_b: u64,
}

/// Open-side Orca account set. The `protocol_config` pause gate is enforced —
/// this is a CPI path and respects the global kill-switch.
#[derive(Accounts)]
pub struct RetryPendingRebalance<'info> {
    /// Vault owner — must equal `vault.user_authority`. Funds the new position.
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        seeds = [ProtocolConfig::SEEDS],
        bump = protocol_config.bump,
        constraint = !protocol_config.is_paused @ AargauError::ProgramPaused,
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    #[account(
        mut,
        seeds = [VaultAccount::SEEDS, vault.user_authority.as_ref(), vault.pool_address.as_ref()],
        bump = vault.bump,
        constraint = vault.user_authority == user.key() @ AargauError::UnauthorizedUser,
    )]
    pub vault: Account<'info, VaultAccount>,

    pub mint_a: InterfaceAccount<'info, Mint>,
    pub mint_b: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        constraint = vault_token_a.owner == vault.key() @ AargauError::InvalidVaultPda,
        constraint = vault_token_a.mint == mint_a.key() @ AargauError::InvalidPoolMint,
    )]
    pub vault_token_a: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        constraint = vault_token_b.owner == vault.key() @ AargauError::InvalidVaultPda,
        constraint = vault_token_b.mint == mint_b.key() @ AargauError::InvalidPoolMint,
    )]
    pub vault_token_b: InterfaceAccount<'info, TokenAccount>,

    // ── Orca Whirlpools accounts ─────────────────────────────────────────────
    /// CHECK: validated against `vault.pool_address` + Whirlpools ownership.
    #[account(
        mut,
        constraint = whirlpool.key() == vault.pool_address @ AargauError::InvalidPool,
        constraint = whirlpool.owner == &ORCA_WHIRLPOOL_PROGRAM_ID @ AargauError::InvalidPool,
    )]
    pub whirlpool: UncheckedAccount<'info>,

    /// CHECK: the (uninit) NEW position PDA `[b"position", position_mint]`
    /// initialised by the open CPI; bound to `position_mint` in the handler.
    #[account(mut)]
    pub position: UncheckedAccount<'info>,

    /// CHECK: fresh ephemeral Token-2022 position-NFT mint keypair, co-signed
    /// off-chain. Each retry MUST pass a freshly generated mint.
    #[account(mut)]
    pub position_mint: Signer<'info>,

    /// CHECK: ATA(vault, position_mint) on Token-2022 — custodies the new NFT.
    #[account(mut)]
    pub position_token_account: UncheckedAccount<'info>,

    /// CHECK: pool token vault A — bound to whirlpool via the view parser.
    #[account(mut)]
    pub token_vault_a: UncheckedAccount<'info>,

    /// CHECK: pool token vault B.
    #[account(mut)]
    pub token_vault_b: UncheckedAccount<'info>,

    /// Token program for mint A (SPL classic or Token-2022).
    pub token_program_a: Interface<'info, TokenInterface>,
    /// Token program for mint B (SPL classic or Token-2022).
    pub token_program_b: Interface<'info, TokenInterface>,

    /// CHECK: must equal the Token-2022 program (position NFT mint + ATA).
    #[account(constraint = token_2022_program.key() == TOKEN_2022_PROGRAM_ID @ AargauError::InvalidPool)]
    pub token_2022_program: UncheckedAccount<'info>,

    /// CHECK: must equal `MEMO_PROGRAM_ID`.
    #[account(constraint = memo_program.key() == MEMO_PROGRAM_ID @ AargauError::InvalidPool)]
    pub memo_program: UncheckedAccount<'info>,

    /// CHECK: must equal `ORCA_WHIRLPOOL_PROGRAM_ID`.
    #[account(constraint = whirlpool_program.key() == ORCA_WHIRLPOOL_PROGRAM_ID @ AargauError::InvalidPool)]
    pub whirlpool_program: UncheckedAccount<'info>,

    /// CHECK: position-NFT metadata update authority — pinned to the Whirlpools
    /// const; account #9 of `open_position_with_token_extensions`.
    #[account(constraint = metadata_update_auth.key() == ORCA_METADATA_UPDATE_AUTH @ AargauError::InvalidPool)]
    pub metadata_update_auth: UncheckedAccount<'info>,

    /// CHECK: TickArray covering the NEW lower tick — bound to the stored
    /// pending range in the handler.
    #[account(mut)]
    pub tick_array_lower: UncheckedAccount<'info>,

    /// CHECK: TickArray covering the NEW upper tick.
    #[account(mut)]
    pub tick_array_upper: UncheckedAccount<'info>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handler<'info>(
    mut ctx: Context<'info, RetryPendingRebalance<'info>>,
    params: RetryPendingRebalanceParams,
) -> Result<()> {
    require!(
        ctx.accounts.vault.protocol == Protocol::Orca,
        AargauError::InvalidPool
    );

    // The new range comes strictly from the pending state — not from params.
    let pending = ctx
        .accounts
        .vault
        .pending_rebalance
        .ok_or(AargauError::NoPendingRebalance)?;
    let new_lower = pending.new_tick_lower;
    let new_upper = pending.new_tick_upper;
    require!(new_upper > new_lower, AargauError::InvalidActionPayload);

    // Tx1 cleared the old position; there must be no active position now.
    require!(
        ctx.accounts.vault.position_address.is_none(),
        AargauError::VaultHasActivePosition
    );

    require!(
        params.liquidity_amount > 0,
        AargauError::InvalidActionPayload
    );
    require!(
        params.token_max_a > 0 || params.token_max_b > 0,
        AargauError::InvalidActionPayload
    );
    require!(
        params.token_max_a <= ctx.accounts.vault_token_a.amount,
        AargauError::InsufficientFunds
    );
    require!(
        params.token_max_b <= ctx.accounts.vault_token_b.amount,
        AargauError::InsufficientFunds
    );

    // Parse the Whirlpool view once and bind mints + vaults to the pool.
    let view = parse_whirlpool_view(&ctx.accounts.whirlpool)?;
    require_whirlpool_bindings(
        &view,
        &ctx.accounts.mint_a.key(),
        &ctx.accounts.mint_b.key(),
        &ctx.accounts.token_vault_a.key(),
        &ctx.accounts.token_vault_b.key(),
    )?;

    // Bind the new position PDA to the fresh mint and tick arrays to the stored
    // range.
    let expected_position = derive_position_pda(&ctx.accounts.position_mint.key());
    require_keys_eq!(
        ctx.accounts.position.key(),
        expected_position,
        AargauError::InvalidVaultPda
    );
    require_tick_array(
        &ctx.accounts.whirlpool.key(),
        new_lower,
        view.tick_spacing,
        &ctx.accounts.tick_array_lower.key(),
    )?;
    require_tick_array(
        &ctx.accounts.whirlpool.key(),
        new_upper,
        view.tick_spacing,
        &ctx.accounts.tick_array_upper.key(),
    )?;

    // Reject pair mints carrying a TransferHook / NonTransferable extension —
    // the v2 CPIs serialise `remaining_accounts_info = None` and cannot service
    // them, so the position would be openable but never decreased / closed.
    require_pair_mints_v2_transferable(&ctx)?;

    open_new_position(&mut ctx, new_lower, new_upper)?;
    increase_new_liquidity(
        &mut ctx,
        params.liquidity_amount,
        params.token_max_a,
        params.token_max_b,
    )?;

    let clock = Clock::get()?;
    let vault = &mut ctx.accounts.vault;
    vault.position_address = Some(ctx.accounts.position.key());
    vault.position_mint = Some(ctx.accounts.position_mint.key());
    vault.position_range_lower = Some(new_lower);
    vault.position_range_upper = Some(new_upper);
    vault.pending_rebalance = None;

    // `RebalanceExecuted` carries both an old and new range, but Tx1 cleared the
    // pre-rebalance range and no record of it survives the two-transaction split
    // (the close leg is a separate transaction with no shared state beyond the
    // pending target). Emit `old_range_* == new_range_*`; consumers reconstruct
    // the true pre-rebalance range off-chain from the earlier `RebalancePending`
    // / `PositionClosed` events.
    emit!(RebalanceExecuted {
        vault: vault.key(),
        protocol: Protocol::Orca,
        old_range_lower: new_lower,
        old_range_upper: new_upper,
        new_range_lower: new_lower,
        new_range_upper: new_upper,
        triggered_by: TriggeredBy::User,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// Open-side legs
// ---------------------------------------------------------------------------

fn open_new_position(
    ctx: &mut Context<RetryPendingRebalance>,
    tick_lower: i32,
    tick_upper: i32,
) -> Result<()> {
    let seed_bytes = VaultSignerSeedBytes::new(&ctx.accounts.vault);
    let seeds = seed_bytes.seeds();

    let cpi = OpenPositionWithTokenExtensionsCpi {
        whirlpool_program: ctx.accounts.whirlpool_program.to_account_info(),
        funder: ctx.accounts.user.to_account_info(),
        vault: ctx.accounts.vault.to_account_info(),
        position: ctx.accounts.position.to_account_info(),
        position_mint: ctx.accounts.position_mint.to_account_info(),
        position_token_account: ctx.accounts.position_token_account.to_account_info(),
        whirlpool: ctx.accounts.whirlpool.to_account_info(),
        token_2022_program: ctx.accounts.token_2022_program.to_account_info(),
        system_program: ctx.accounts.system_program.to_account_info(),
        associated_token_program: ctx.accounts.associated_token_program.to_account_info(),
        metadata_update_auth: ctx.accounts.metadata_update_auth.to_account_info(),
    };
    invoke_open_position_with_token_extensions(&cpi, tick_lower, tick_upper, &seeds)?;

    // Re-snapshot the per-mint Token-2022 transfer-fee config from the live
    // mints. A Token-2022 mint can update its transfer-fee config between
    // epochs, so the snapshot taken at the original open (Tx1's source position)
    // can be stale by the time this Tx2 runs. Refresh before `increase_liquidity`
    // so the fee path accounts for the current deductions — mirrors the Slice-1
    // open leg in `execute_action_orca::handle_open_position`.
    snapshot_transfer_fees(ctx)
}

fn increase_new_liquidity<'info>(
    ctx: &mut Context<'info, RetryPendingRebalance<'info>>,
    liquidity_amount: u128,
    token_max_a: u64,
    token_max_b: u64,
) -> Result<()> {
    let pre_a = ctx.accounts.vault_token_a.amount;
    let pre_b = ctx.accounts.vault_token_b.amount;

    let seed_bytes = VaultSignerSeedBytes::new(&ctx.accounts.vault);
    let seeds = seed_bytes.seeds();

    let cpi = IncreaseLiquidityV2Cpi {
        whirlpool_program: ctx.accounts.whirlpool_program.to_account_info(),
        whirlpool: ctx.accounts.whirlpool.to_account_info(),
        token_program_a: ctx.accounts.token_program_a.to_account_info(),
        token_program_b: ctx.accounts.token_program_b.to_account_info(),
        memo_program: ctx.accounts.memo_program.to_account_info(),
        vault: ctx.accounts.vault.to_account_info(),
        position: ctx.accounts.position.to_account_info(),
        position_token_account: ctx.accounts.position_token_account.to_account_info(),
        token_mint_a: ctx.accounts.mint_a.to_account_info(),
        token_mint_b: ctx.accounts.mint_b.to_account_info(),
        vault_token_a: ctx.accounts.vault_token_a.to_account_info(),
        vault_token_b: ctx.accounts.vault_token_b.to_account_info(),
        token_vault_a: ctx.accounts.token_vault_a.to_account_info(),
        token_vault_b: ctx.accounts.token_vault_b.to_account_info(),
        tick_array_lower: ctx.accounts.tick_array_lower.to_account_info(),
        tick_array_upper: ctx.accounts.tick_array_upper.to_account_info(),
    };
    invoke_increase_liquidity_v2(&cpi, liquidity_amount, token_max_a, token_max_b, &seeds)?;

    // Defence-in-depth: the consumed amounts must not exceed the caller ceilings
    // (the v2 CPI also enforces them).
    ctx.accounts.vault_token_a.reload()?;
    ctx.accounts.vault_token_b.reload()?;
    let consumed_a = pre_a.saturating_sub(ctx.accounts.vault_token_a.amount);
    let consumed_b = pre_b.saturating_sub(ctx.accounts.vault_token_b.amount);
    require!(consumed_a <= token_max_a, AargauError::SlippageExceeded);
    require!(consumed_b <= token_max_b, AargauError::SlippageExceeded);

    Ok(())
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn require_pair_mints_v2_transferable(ctx: &Context<RetryPendingRebalance>) -> Result<()> {
    for mint in [&ctx.accounts.mint_a, &ctx.accounts.mint_b] {
        let info = mint.to_account_info();
        if is_token_2022(info.owner) {
            let data = info.try_borrow_data()?;
            require_v2_transferable_mint(&data)?;
        }
    }
    Ok(())
}

/// Snapshot the Token-2022 transfer-fee config for both pair mints onto the
/// vault, refreshed from the live mints at the moment of the new open. A
/// Token-2022 mint can update its transfer-fee config between epochs, so the
/// snapshot must be re-read here rather than inherited from the original open.
/// `uses_token_2022` is set when either mint is Token-2022; SPL classic mints
/// (or Token-2022 without the extension) leave the fields at zero. Mirrors
/// `execute_action_orca::snapshot_transfer_fees`.
fn snapshot_transfer_fees(ctx: &mut Context<RetryPendingRebalance>) -> Result<()> {
    let mint_a_is_2022 = is_token_2022(ctx.accounts.mint_a.to_account_info().owner);
    let mint_b_is_2022 = is_token_2022(ctx.accounts.mint_b.to_account_info().owner);

    let (a_bps, a_max) = read_fee(&ctx.accounts.mint_a.to_account_info())?;
    let (b_bps, b_max) = read_fee(&ctx.accounts.mint_b.to_account_info())?;

    let vault = &mut ctx.accounts.vault;
    vault.uses_token_2022 = mint_a_is_2022 || mint_b_is_2022;
    vault.token_a_transfer_fee_bps = a_bps;
    vault.token_a_maximum_fee = a_max;
    vault.token_b_transfer_fee_bps = b_bps;
    vault.token_b_maximum_fee = b_max;
    Ok(())
}

fn read_fee(mint: &AccountInfo<'_>) -> Result<(u16, u64)> {
    let data = mint.try_borrow_data()?;
    Ok(read_transfer_fee_config(&data)
        .map(|c| (c.transfer_fee_bps, c.maximum_fee))
        .unwrap_or((0, 0)))
}

fn parse_whirlpool_view(whirlpool: &UncheckedAccount<'_>) -> Result<WhirlpoolView> {
    let data = whirlpool.try_borrow_data()?;
    parse_whirlpool_view_from_bytes(&data)
}
