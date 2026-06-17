//! `start_rebalance_orca` — phase one of a two-transaction Orca Whirlpools
//! rebalance (CloseOld). User-signed; the vault PDA is the position authority.
//!
//! An Orca rebalance burns the old position NFT and mints a new one, which
//! does not fit in a single transaction. This instruction performs the close
//! leg and persists the target range in `vault.pending_rebalance`; a following
//! `retry_pending_rebalance` call performs the open leg (Tx2). While
//! `pending_rebalance.is_some()` the vault is in a transient state — the tokens
//! drained from the old position live in the vault ATAs and are withdrawable,
//! and `execute_action_orca` is blocked.
//!
//! Sequence (single transaction):
//!   1. collect LP fees (+ best-effort rewards) with the treasury split.
//!   2. `decrease_liquidity_v2` for the full position liquidity (caller-supplied
//!      `decrease_liquidity_amount` — the Orca `Position` layout is not parsed
//!      on-chain).
//!   3. `close_position_with_token_extensions` (burns the NFT, rent → user).
//!   4. clear `position_*`, write `pending_rebalance` with the target range.
//!
//! The position NFT, pair-mint and tick-array set mirror the close-side subset
//! of `execute_action_orca`. The tick arrays here cover the OLD (current)
//! range; Tx2 uses the NEW-range tick arrays.
//!
//! Token-2022 / reward handling matches `execute_action_orca::CollectFees`:
//! per-mint performance-fee transfers, fee base measured before rewards, and
//! incompatible reward mints skipped (never fatal). For `CollectFees` the
//! caller appends 4 accounts per active reward slot to `remaining_accounts`:
//! `[reward_owner_ata, reward_mint, reward_vault, reward_token_program]`.

use crate::{
    constants::{
        MEMO_PROGRAM_ID, ORCA_REWARD_SLOTS, ORCA_WHIRLPOOL_PROGRAM_ID, TOKEN_2022_PROGRAM_ID,
        TREASURY_SEED,
    },
    errors::AargauError,
    events::{FeesClaimed, RebalancePending, RewardCollectionSkipped},
    state::{PendingRebalance, Protocol, ProtocolConfig, TriggeredBy, VaultAccount},
    utils::{
        fee::calc_aargau_fee,
        orca::{
            accounts::{
                is_token_2022, require_orca_position, require_reward_owner_is_vault_ata,
                require_v2_transferable_mint,
            },
            close_position::{
                invoke_close_position_with_token_extensions, ClosePositionWithTokenExtensionsCpi,
            },
            collect_fees::{
                invoke_collect_fees_v2, invoke_collect_reward_v2, CollectFeesV2Cpi,
                CollectRewardV2Cpi,
            },
            decrease_liquidity::{invoke_decrease_liquidity_v2, DecreaseLiquidityV2Cpi},
            rebalance_helpers::{
                require_tick_array, transfer_performance_fee, VaultSignerSeedBytes,
            },
            whirlpool_view::{
                parse_whirlpool_view_from_bytes, require_whirlpool_bindings, WhirlpoolView,
            },
        },
    },
};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

/// Number of accounts appended to `remaining_accounts` per active reward slot
/// (reward_owner ATA, reward_mint, reward_vault, reward_token_program).
const ORCA_REWARD_ACCOUNTS_PER_SLOT: usize = 4;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct StartRebalanceOrcaParams {
    /// Target lower tick of the new range — persisted into `pending_rebalance`
    /// and consumed by the Tx2 open leg.
    pub new_tick_lower: i32,
    /// Target upper tick of the new range.
    pub new_tick_upper: i32,
    /// Full current position liquidity to remove.
    ///
    /// LOAD-BEARING OFF-CHAIN CONTRACT: this MUST equal the position's *entire*
    /// current `liquidity`. The Orca `Position` layout is deliberately not
    /// parsed on-chain (no `Position` deserialize — keeps the close leg cheap
    /// and the account set minimal), so the program cannot verify the value is
    /// the full amount. The builder reads `Position.liquidity` off-chain and
    /// passes it verbatim.
    ///
    /// If the caller under-states this, `decrease_liquidity_v2` removes only
    /// part of the position and the subsequent `close_position` CPI reverts —
    /// Orca requires an empty position to close — surfacing as an opaque
    /// Whirlpools error rather than a typed `AargauError`. The whole
    /// transaction rolls back atomically (no partial state persists), so a
    /// retry with the correct full liquidity is safe.
    pub decrease_liquidity_amount: u128,
    /// Slippage floor for token A returned by the decrease.
    pub token_min_a: u64,
    /// Slippage floor for token B returned by the decrease.
    pub token_min_b: u64,
}

/// Close-side Orca account set. The `protocol_config` pause gate is enforced —
/// this is a CPI path and respects the global kill-switch. Treasury ATAs are
/// required for the fee split. Two per-mint token programs route each fee leg
/// through the program that owns that leg's mint.
#[derive(Accounts)]
pub struct StartRebalanceOrca<'info> {
    /// Vault owner — must equal `vault.user_authority`. Receives the position
    /// rent on close.
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

    /// CHECK: treasury PDA — validated by seeds + bump.
    #[account(seeds = [TREASURY_SEED, protocol_config.key().as_ref()], bump)]
    pub treasury_pda: UncheckedAccount<'info>,

    #[account(
        mut,
        constraint = treasury_token_a.owner == treasury_pda.key() @ AargauError::InvalidTreasury,
        constraint = treasury_token_a.mint == mint_a.key() @ AargauError::InvalidPoolMint,
    )]
    pub treasury_token_a: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        constraint = treasury_token_b.owner == treasury_pda.key() @ AargauError::InvalidTreasury,
        constraint = treasury_token_b.mint == mint_b.key() @ AargauError::InvalidPoolMint,
    )]
    pub treasury_token_b: InterfaceAccount<'info, TokenAccount>,

    // ── Orca Whirlpools accounts ─────────────────────────────────────────────
    /// CHECK: validated against `vault.pool_address` + Whirlpools ownership.
    #[account(
        mut,
        constraint = whirlpool.key() == vault.pool_address @ AargauError::InvalidPool,
        constraint = whirlpool.owner == &ORCA_WHIRLPOOL_PROGRAM_ID @ AargauError::InvalidPool,
    )]
    pub whirlpool: UncheckedAccount<'info>,

    /// CHECK: must equal `vault.position_address` and pass the `Position`
    /// discriminator check — enforced in the handler.
    #[account(mut)]
    pub position: UncheckedAccount<'info>,

    /// CHECK: the Token-2022 position NFT mint — must equal
    /// `vault.position_mint`. Closed by the CPI.
    #[account(mut)]
    pub position_mint: UncheckedAccount<'info>,

    /// CHECK: ATA(vault, position_mint) on Token-2022 — custodies the NFT.
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

    /// CHECK: TickArray covering the OLD lower tick — bound to the active
    /// position range in the handler.
    #[account(mut)]
    pub tick_array_lower: UncheckedAccount<'info>,

    /// CHECK: TickArray covering the OLD upper tick.
    #[account(mut)]
    pub tick_array_upper: UncheckedAccount<'info>,
    // For the collect leg, per active reward slot the caller appends 4 accounts
    // to `remaining_accounts`: reward_owner ATA, reward_mint, reward_vault,
    // reward_token_program.
}

pub fn handler<'info>(
    mut ctx: Context<'info, StartRebalanceOrca<'info>>,
    params: StartRebalanceOrcaParams,
) -> Result<()> {
    require!(
        ctx.accounts.vault.protocol == Protocol::Orca,
        AargauError::InvalidPool
    );
    require!(
        ctx.accounts.vault.pending_rebalance.is_none(),
        AargauError::PendingRebalanceExists
    );
    require!(
        params.new_tick_upper > params.new_tick_lower,
        AargauError::InvalidActionPayload
    );
    require!(
        params.decrease_liquidity_amount > 0,
        AargauError::InvalidActionPayload
    );
    // Reject a fully-zero slippage floor: a non-empty full close always returns
    // value on at least one side (token A below range, token B above range, both
    // in range), so a 0/0 floor would silently disable slippage protection on
    // the close leg. A legitimate single-sided floor (one side 0) is still
    // allowed.
    require!(
        params.token_min_a > 0 || params.token_min_b > 0,
        AargauError::SlippageExceeded
    );

    // The position must exist and match what the vault recorded on open.
    require_active_position(&ctx.accounts.vault, &ctx.accounts.position)?;

    // Parse the Whirlpool view once and bind mints + vaults to the pool. This
    // blocks substituting an arbitrary token account for a pool vault.
    let view = parse_whirlpool_view(&ctx.accounts.whirlpool)?;
    require_whirlpool_bindings(
        &view,
        &ctx.accounts.mint_a.key(),
        &ctx.accounts.mint_b.key(),
        &ctx.accounts.token_vault_a.key(),
        &ctx.accounts.token_vault_b.key(),
    )?;

    // Bind both tick-array accounts to the OLD (active) position range.
    require_tick_arrays_bound(&ctx, &view)?;

    // 1. Collect LP fees + best-effort rewards, route the fee split to treasury.
    collect_fees_and_route_to_treasury(&mut ctx, &view)?;

    // 2. Drain the full position liquidity back into the vault ATAs.
    decrease_full_liquidity(
        &mut ctx,
        params.decrease_liquidity_amount,
        params.token_min_a,
        params.token_min_b,
    )?;

    // 3. Burn the position NFT (rent → user). This CPI requires the position to
    //    be empty: if `decrease_liquidity_amount` under-stated the true full
    //    liquidity, residual liquidity remains and Orca's close reverts here
    //    (opaque Whirlpools error). The whole transaction rolls back — no
    //    partial state persists — so a retry with the correct full liquidity is
    //    safe. See `StartRebalanceOrcaParams::decrease_liquidity_amount`.
    close_old_position(&mut ctx)?;

    // 4. Record the pending state and clear the old position fields.
    let clock = Clock::get()?;
    let vault = &mut ctx.accounts.vault;
    vault.position_address = None;
    vault.position_mint = None;
    vault.position_range_lower = None;
    vault.position_range_upper = None;
    vault.pending_rebalance = Some(PendingRebalance {
        new_tick_lower: params.new_tick_lower,
        new_tick_upper: params.new_tick_upper,
        initiated_at: clock.unix_timestamp,
        retry_count: 0,
        last_failure_reason: 0,
    });

    emit!(RebalancePending {
        vault: vault.key(),
        new_range_lower: params.new_tick_lower,
        new_range_upper: params.new_tick_upper,
        initiated_at: clock.unix_timestamp,
        triggered_by: TriggeredBy::User,
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// Close-side legs
// ---------------------------------------------------------------------------

/// Collect LP fees (v2) then sweep active reward slots, taking the protocol
/// performance fee on the LP-fee gross only. Mirrors
/// `execute_action_orca::handle_collect_fees`: the fee base is measured BEFORE
/// rewards so reward proceeds (which may share a pair-mint ATA) are never
/// folded into the taxed gross.
fn collect_fees_and_route_to_treasury<'info>(
    ctx: &mut Context<'info, StartRebalanceOrca<'info>>,
    view: &WhirlpoolView,
) -> Result<()> {
    let pre_a = ctx.accounts.vault_token_a.amount;
    let pre_b = ctx.accounts.vault_token_b.amount;

    let seed_bytes = VaultSignerSeedBytes::new(&ctx.accounts.vault);
    let seeds = seed_bytes.seeds();

    let fees_cpi = CollectFeesV2Cpi {
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
    };
    invoke_collect_fees_v2(&fees_cpi, &seeds)?;

    // Measure the LP-fee delta NOW — before rewards are collected.
    ctx.accounts.vault_token_a.reload()?;
    ctx.accounts.vault_token_b.reload()?;
    let gross_a = ctx
        .accounts
        .vault_token_a
        .amount
        .checked_sub(pre_a)
        .ok_or(error!(AargauError::PostCpiBalanceDecreased))?;
    let gross_b = ctx
        .accounts
        .vault_token_b
        .amount
        .checked_sub(pre_b)
        .ok_or(error!(AargauError::PostCpiBalanceDecreased))?;

    // Sweep active reward slots AFTER the fee base is fixed — untaxed.
    collect_rewards(ctx, view, &seeds)?;

    let fee_rate_bps = ctx.accounts.protocol_config.fee_rate_bps;
    let fee_a = calc_aargau_fee(gross_a, fee_rate_bps)?;
    let fee_b = calc_aargau_fee(gross_b, fee_rate_bps)?;

    let user_net_a = gross_a
        .checked_sub(fee_a)
        .ok_or(error!(AargauError::FeeExceedsGross))?;
    let user_net_b = gross_b
        .checked_sub(fee_b)
        .ok_or(error!(AargauError::FeeExceedsGross))?;

    let signer_arr: &[&[&[u8]]] = &[&seeds];

    transfer_performance_fee(
        ctx.accounts.token_program_a.key(),
        ctx.accounts.vault_token_a.to_account_info(),
        ctx.accounts.mint_a.to_account_info(),
        ctx.accounts.treasury_token_a.to_account_info(),
        ctx.accounts.vault.to_account_info(),
        ctx.accounts.mint_a.decimals,
        fee_a,
        signer_arr,
    )?;
    transfer_performance_fee(
        ctx.accounts.token_program_b.key(),
        ctx.accounts.vault_token_b.to_account_info(),
        ctx.accounts.mint_b.to_account_info(),
        ctx.accounts.treasury_token_b.to_account_info(),
        ctx.accounts.vault.to_account_info(),
        ctx.accounts.mint_b.decimals,
        fee_b,
        signer_arr,
    )?;

    // Refresh balances so the subsequent decrease/close legs see the holdings
    // after the treasury split.
    ctx.accounts.vault_token_a.reload()?;
    ctx.accounts.vault_token_b.reload()?;

    let clock = Clock::get()?;
    emit!(FeesClaimed {
        vault: ctx.accounts.vault.key(),
        gross_a,
        gross_b,
        aargau_fee_a: fee_a,
        aargau_fee_b: fee_b,
        user_net_a,
        user_net_b,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

/// Run `decrease_liquidity_v2` for the full position liquidity, enforcing the
/// caller-supplied slippage floors on the observed delta as defence-in-depth.
fn decrease_full_liquidity<'info>(
    ctx: &mut Context<'info, StartRebalanceOrca<'info>>,
    liquidity_amount: u128,
    token_min_a: u64,
    token_min_b: u64,
) -> Result<()> {
    let pre_a = ctx.accounts.vault_token_a.amount;
    let pre_b = ctx.accounts.vault_token_b.amount;

    let seed_bytes = VaultSignerSeedBytes::new(&ctx.accounts.vault);
    let seeds = seed_bytes.seeds();

    let cpi: DecreaseLiquidityV2Cpi = DecreaseLiquidityV2Cpi {
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
    invoke_decrease_liquidity_v2(&cpi, liquidity_amount, token_min_a, token_min_b, &seeds)?;

    ctx.accounts.vault_token_a.reload()?;
    ctx.accounts.vault_token_b.reload()?;
    let received_a = ctx
        .accounts
        .vault_token_a
        .amount
        .checked_sub(pre_a)
        .ok_or(error!(AargauError::PostCpiBalanceDecreased))?;
    let received_b = ctx
        .accounts
        .vault_token_b
        .amount
        .checked_sub(pre_b)
        .ok_or(error!(AargauError::PostCpiBalanceDecreased))?;

    require!(received_a >= token_min_a, AargauError::SlippageExceeded);
    require!(received_b >= token_min_b, AargauError::SlippageExceeded);

    Ok(())
}

/// Burn the empty position NFT and refund rent to the user. Requires the
/// position fully drained by `decrease_full_liquidity`.
fn close_old_position(ctx: &mut Context<StartRebalanceOrca>) -> Result<()> {
    let expected_mint = ctx
        .accounts
        .vault
        .position_mint
        .ok_or(AargauError::VaultNoActivePosition)?;
    require_keys_eq!(
        ctx.accounts.position_mint.key(),
        expected_mint,
        AargauError::InvalidVaultPda
    );

    let seed_bytes = VaultSignerSeedBytes::new(&ctx.accounts.vault);
    let seeds = seed_bytes.seeds();

    let cpi = ClosePositionWithTokenExtensionsCpi {
        whirlpool_program: ctx.accounts.whirlpool_program.to_account_info(),
        vault: ctx.accounts.vault.to_account_info(),
        receiver: ctx.accounts.user.to_account_info(),
        position: ctx.accounts.position.to_account_info(),
        position_mint: ctx.accounts.position_mint.to_account_info(),
        position_token_account: ctx.accounts.position_token_account.to_account_info(),
        token_2022_program: ctx.accounts.token_2022_program.to_account_info(),
    };
    invoke_close_position_with_token_extensions(&cpi, &seeds)
}

// ---------------------------------------------------------------------------
// Reward collection (mirrors execute_action_orca::collect_rewards)
// ---------------------------------------------------------------------------

/// Run `collect_reward_v2` once per active reward slot. Active slots are the
/// whirlpool reward triplets whose `vault != Pubkey::default()`. Incompatible
/// (TransferHook / NonTransferable) Token-2022 reward mints are skipped, never
/// fatal — the reward stays in the position and a skip event is emitted.
fn collect_rewards<'info>(
    ctx: &Context<'info, StartRebalanceOrca<'info>>,
    view: &WhirlpoolView,
    seeds: &[&[u8]],
) -> Result<()> {
    let active: Vec<usize> = (0..ORCA_REWARD_SLOTS)
        .filter(|&i| view.rewards[i].vault != Pubkey::default())
        .collect();

    if active.is_empty() {
        require!(
            ctx.remaining_accounts.is_empty(),
            AargauError::InvalidActionPayload
        );
        return Ok(());
    }

    let expected = active.len() * ORCA_REWARD_ACCOUNTS_PER_SLOT;
    require!(
        ctx.remaining_accounts.len() == expected,
        AargauError::InvalidActionPayload
    );

    for (slot_pos, &reward_index) in active.iter().enumerate() {
        let base = slot_pos * ORCA_REWARD_ACCOUNTS_PER_SLOT;
        let reward_owner_account = &ctx.remaining_accounts[base];
        let reward_mint = &ctx.remaining_accounts[base + 1];
        let reward_vault = &ctx.remaining_accounts[base + 2];
        let reward_token_program = &ctx.remaining_accounts[base + 3];

        require_keys_eq!(
            reward_vault.key(),
            view.rewards[reward_index].vault,
            AargauError::InvalidPoolReserve
        );
        require_keys_eq!(
            reward_mint.key(),
            view.rewards[reward_index].mint,
            AargauError::InvalidPoolMint
        );
        require_keys_eq!(
            reward_token_program.key(),
            *reward_mint.owner,
            AargauError::InvalidRewardOwner
        );
        require_reward_owner_is_vault_ata(
            &reward_owner_account.key(),
            &ctx.accounts.vault.key(),
            &reward_mint.key(),
            &reward_token_program.key(),
        )?;

        if is_token_2022(reward_mint.owner) {
            let mint_data = reward_mint.try_borrow_data()?;
            if require_v2_transferable_mint(&mint_data).is_err() {
                drop(mint_data);
                emit!(RewardCollectionSkipped {
                    vault: ctx.accounts.vault.key(),
                    reward_index: reward_index as u8,
                    reward_mint: reward_mint.key(),
                    timestamp: Clock::get()?.unix_timestamp,
                });
                continue;
            }
        }

        let cpi = CollectRewardV2Cpi {
            whirlpool_program: ctx.accounts.whirlpool_program.to_account_info(),
            whirlpool: ctx.accounts.whirlpool.to_account_info(),
            vault: ctx.accounts.vault.to_account_info(),
            position: ctx.accounts.position.to_account_info(),
            position_token_account: ctx.accounts.position_token_account.to_account_info(),
            reward_owner_account: reward_owner_account.to_account_info(),
            reward_mint: reward_mint.to_account_info(),
            reward_vault: reward_vault.to_account_info(),
            reward_token_program: reward_token_program.to_account_info(),
            memo_program: ctx.accounts.memo_program.to_account_info(),
        };
        invoke_collect_reward_v2(&cpi, reward_index as u8, seeds)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Parse the on-chain `Whirlpool` view (validates discriminator as
/// defence-in-depth on top of the owner constraint).
fn parse_whirlpool_view(whirlpool: &UncheckedAccount<'_>) -> Result<WhirlpoolView> {
    let data = whirlpool.try_borrow_data()?;
    parse_whirlpool_view_from_bytes(&data)
}

/// The `position` AccountInfo must match `vault.position_address` AND be a real
/// Orca `Position` owned by the Whirlpools program.
fn require_active_position(vault: &VaultAccount, position: &UncheckedAccount<'_>) -> Result<()> {
    let expected = vault
        .position_address
        .ok_or(AargauError::VaultNoActivePosition)?;
    require_keys_eq!(position.key(), expected, AargauError::VaultNoActivePosition);
    require_orca_position(&position.to_account_info())
}

/// Bind both tick-array accounts to the PDAs derived from the active position's
/// stored (OLD) range and the pool's tick spacing.
fn require_tick_arrays_bound(
    ctx: &Context<StartRebalanceOrca>,
    view: &WhirlpoolView,
) -> Result<()> {
    let lower = ctx
        .accounts
        .vault
        .position_range_lower
        .ok_or(AargauError::VaultNoActivePosition)?;
    let upper = ctx
        .accounts
        .vault
        .position_range_upper
        .ok_or(AargauError::VaultNoActivePosition)?;
    require_tick_array(
        &ctx.accounts.whirlpool.key(),
        lower,
        view.tick_spacing,
        &ctx.accounts.tick_array_lower.key(),
    )?;
    require_tick_array(
        &ctx.accounts.whirlpool.key(),
        upper,
        view.tick_spacing,
        &ctx.accounts.tick_array_upper.key(),
    )
}
