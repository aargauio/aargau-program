//! `execute_action` — user-signed entry point for keeper-style operations on a
//! Meteora DLMM vault.
//!
//! Current scope: only Meteora and only the three position-management ops:
//! `CollectFees`, `IncreaseLiquidity`, `DecreaseLiquidity`. The vault PDA
//! signs the underlying CPIs; performance fees on collected LP fees are routed
//! to the protocol treasury ATAs.
//!
//! `OpenPosition` / `ClosePosition` and the keeper dual-authority signer model
//! ship later. The struct currently requires the vault owner (`user`) to sign —
//! `triggered_by` is therefore always `User` for emitted events.
//!
//! Token-2022 transfer hooks are intentionally unsupported here. SPL classic
//! mints only.

use crate::{
    constants::{
        BPS_DIVISOR_U16, MAX_ACTIVE_BIN_SLIPPAGE_HARD_CAP, MAX_METEORA_BINS, MEMO_PROGRAM_ID,
        METEORA_DLMM_PROGRAM_ID, METEORA_INLINE_BITMAP_BIN_LIMIT, TREASURY_SEED,
    },
    errors::AargauError,
    events::{FeesClaimed, LiquidityChanged, LiquidityOp, PositionClosed, PositionOpened},
    state::{KeeperAction, Protocol, ProtocolConfig, TriggeredBy, VaultAccount},
    utils::{
        fee::calc_aargau_fee,
        meteora::{
            add_liquidity::{
                invoke_add_liquidity_by_strategy2, AddLiquidityByStrategy2Cpi,
                METEORA_STRATEGY_SPOT_IMBALANCED,
            },
            claim_fee::{invoke_claim_fee2, ClaimFee2Cpi},
            close_position::{invoke_close_position_if_empty, ClosePositionIfEmptyCpi},
            lb_pair_view::{
                parse_lb_pair_view_from_bytes, require_lb_pair_bindings, require_spl_classic_mints,
                LbPairView,
            },
            open_position::{invoke_initialize_position, InitializePositionCpi},
            position_view::require_position_v2,
            remove_liquidity::{invoke_remove_liquidity_by_range2, RemoveLiquidityByRange2Cpi},
        },
        signer_seeds::vault_signer_seeds,
    },
};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct ExecuteActionParams {
    pub action: KeeperAction,
}

/// User-signed instruction. The `protocol_config` is loaded purely to enforce
/// the global pause kill-switch and to fetch `fee_rate_bps` for the treasury
/// split on `CollectFees`.
///
/// Account ordering & flags mirror the Meteora CPI helpers in
/// `utils/meteora/`. Treasury ATAs are required even for non-fee ops to keep
/// the IDL stable; they are only written to by `CollectFees`.
#[derive(Accounts)]
pub struct ExecuteAction<'info> {
    /// Vault owner — must equal `vault.user_authority`.
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
    #[account(
        seeds = [TREASURY_SEED, protocol_config.key().as_ref()],
        bump,
    )]
    pub treasury_pda: UncheckedAccount<'info>,

    /// Treasury ATA for token A — owner must equal `treasury_pda`. Only written
    /// to on `CollectFees`; required on every call to keep the IDL stable.
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

    // ── Meteora DLMM accounts ────────────────────────────────────────────────
    /// CHECK: validated against `vault.pool_address` + DLMM program ownership.
    #[account(
        mut,
        constraint = lb_pair.key() == vault.pool_address @ AargauError::InvalidPool,
        constraint = lb_pair.owner == &METEORA_DLMM_PROGRAM_ID @ AargauError::InvalidPool,
    )]
    pub lb_pair: UncheckedAccount<'info>,

    /// CHECK: for `CollectFees` / `IncreaseLiquidity` / `DecreaseLiquidity` /
    /// `ClosePosition` this must equal `vault.position_address` and pass the
    /// `PositionV2` discriminator check — enforced per-arm inside the handler
    /// because `OpenPosition` runs against an uninitialised account that
    /// would otherwise fail an Anchor constraint. For `OpenPosition` this is
    /// the ephemeral `PositionV2` keypair co-signed off-chain by the client
    /// (decision C1); the DLMM program initialises and owns it on success.
    #[account(mut)]
    pub position: UncheckedAccount<'info>,

    /// CHECK: pool reserve for token X — DLMM enforces the binding to lb_pair.
    #[account(mut)]
    pub reserve_x: UncheckedAccount<'info>,

    /// CHECK: pool reserve for token Y.
    #[account(mut)]
    pub reserve_y: UncheckedAccount<'info>,

    /// Token program for token X. Both A and B may use the same program (SPL
    /// classic) or different programs in mixed Token-2022 pools — Token-2022
    /// hooks are not yet wired, but the account is kept separate so the
    /// account list stays stable when hook support is added.
    pub token_program_x: Interface<'info, TokenInterface>,
    pub token_program_y: Interface<'info, TokenInterface>,

    /// Token program used by the Aargau performance-fee transfers. Equals the
    /// program owning `mint_a` / `mint_b`.
    pub token_program: Interface<'info, TokenInterface>,

    /// CHECK: must equal `MEMO_PROGRAM_ID`.
    #[account(constraint = memo_program.key() == MEMO_PROGRAM_ID @ AargauError::InvalidPool)]
    pub memo_program: UncheckedAccount<'info>,

    /// CHECK: PDA owned by the DLMM program (`["__event_authority"]`).
    /// The DLMM program verifies the seeds itself; we only re-pass it.
    pub event_authority: UncheckedAccount<'info>,

    /// CHECK: must equal `METEORA_DLMM_PROGRAM_ID`.
    #[account(constraint = dlmm_program.key() == METEORA_DLMM_PROGRAM_ID @ AargauError::InvalidPool)]
    pub dlmm_program: UncheckedAccount<'info>,

    /// CHECK: BinArray PDA covering the lower half of the position range.
    /// Equality vs the derived PDA is checked inside the CPI helper.
    #[account(mut)]
    pub bin_array_lower: UncheckedAccount<'info>,

    /// CHECK: BinArray PDA = `bin_array_lower_index + 1`.
    #[account(mut)]
    pub bin_array_upper: UncheckedAccount<'info>,

    /// System program — required by `initialize_position` (rent payment for
    /// the new `PositionV2` account). Wired on every call to keep the IDL
    /// stable across action variants.
    pub system_program: Program<'info, System>,

    /// CHECK: Rent sysvar — required by `initialize_position`. The DLMM
    /// program checks the well-known sysvar key itself.
    pub rent_sysvar: Sysvar<'info, Rent>,
}

pub fn handler(mut ctx: Context<ExecuteAction>, params: ExecuteActionParams) -> Result<()> {
    require!(
        ctx.accounts.vault.protocol == Protocol::Meteora,
        AargauError::InvalidPool
    );
    require!(
        ctx.accounts.vault.pending_rebalance.is_none(),
        AargauError::PendingRebalanceExists
    );

    // Parse the LbPair view once, validate discriminator + binding of all
    // dependent accounts (mints, reserves) to the pool. This blocks the
    // attack where a caller substitutes any SPL token account for a reserve.
    let lb_pair_view = parse_lb_pair_view(&ctx.accounts.lb_pair)?;
    require_lb_pair_bindings(
        &lb_pair_view,
        &ctx.accounts.mint_a.key(),
        &ctx.accounts.mint_b.key(),
        &ctx.accounts.reserve_x.key(),
        &ctx.accounts.reserve_y.key(),
    )?;

    // SPL-classic enforcement on the fee transfer path. The treasury split
    // uses `token_program`; if either mint lives under Token-2022 we cannot
    // safely run `transfer_checked` without the transfer-hook accounts,
    // which are not yet wired.
    require_spl_classic_mints(
        ctx.accounts.mint_a.to_account_info().owner,
        ctx.accounts.mint_b.to_account_info().owner,
        &ctx.accounts.token_program.key(),
    )?;

    match params.action {
        KeeperAction::CollectFees => {
            require_active_position_v2(&ctx.accounts.vault, &ctx.accounts.position)?;
            let (range_lower, range_upper) = position_range(&ctx.accounts.vault)?;
            handle_collect_fees(&mut ctx, range_lower, range_upper)
        }
        KeeperAction::IncreaseLiquidity {
            amount_a_max,
            amount_b_max,
            active_id_slippage,
        } => {
            require_active_position_v2(&ctx.accounts.vault, &ctx.accounts.position)?;
            let (range_lower, range_upper) = position_range(&ctx.accounts.vault)?;
            require_bin_count_within_cap(range_lower, range_upper)?;
            handle_increase_liquidity(
                &mut ctx,
                range_lower,
                range_upper,
                amount_a_max,
                amount_b_max,
                lb_pair_view.active_id,
                active_id_slippage,
            )
        }
        KeeperAction::DecreaseLiquidity { bps } => {
            require_active_position_v2(&ctx.accounts.vault, &ctx.accounts.position)?;
            let (range_lower, range_upper) = position_range(&ctx.accounts.vault)?;
            handle_decrease_liquidity(&mut ctx, range_lower, range_upper, bps)
        }
        KeeperAction::OpenPosition {
            lower_bin_id,
            upper_bin_id,
        } => handle_open_position(&mut ctx, lower_bin_id, upper_bin_id),
        KeeperAction::ClosePosition => {
            require_active_position_v2(&ctx.accounts.vault, &ctx.accounts.position)?;
            handle_close_position(&mut ctx)
        }
    }
}

// ---------------------------------------------------------------------------
// Action handlers
// ---------------------------------------------------------------------------

fn handle_collect_fees(
    ctx: &mut Context<ExecuteAction>,
    range_lower: i32,
    range_upper: i32,
) -> Result<()> {
    let pre_a = ctx.accounts.vault_token_a.amount;
    let pre_b = ctx.accounts.vault_token_b.amount;

    let seed_bytes = vault_signer_seed_bytes(&ctx.accounts.vault);
    let seeds = vault_signer_seeds(&seed_bytes.user, &seed_bytes.pool, &seed_bytes.bump);

    let cpi = ClaimFee2Cpi {
        dlmm_program: ctx.accounts.dlmm_program.to_account_info(),
        lb_pair: ctx.accounts.lb_pair.to_account_info(),
        position: ctx.accounts.position.to_account_info(),
        vault: ctx.accounts.vault.to_account_info(),
        reserve_x: ctx.accounts.reserve_x.to_account_info(),
        reserve_y: ctx.accounts.reserve_y.to_account_info(),
        vault_token_x: ctx.accounts.vault_token_a.to_account_info(),
        vault_token_y: ctx.accounts.vault_token_b.to_account_info(),
        token_x_mint: ctx.accounts.mint_a.to_account_info(),
        token_y_mint: ctx.accounts.mint_b.to_account_info(),
        token_program_x: ctx.accounts.token_program_x.to_account_info(),
        token_program_y: ctx.accounts.token_program_y.to_account_info(),
        memo_program: ctx.accounts.memo_program.to_account_info(),
        event_authority: ctx.accounts.event_authority.to_account_info(),
        bin_array_lower: ctx.accounts.bin_array_lower.to_account_info(),
        bin_array_upper: ctx.accounts.bin_array_upper.to_account_info(),
    };
    invoke_claim_fee2(&cpi, range_lower, range_upper, &seeds)?;

    // Refresh balances after the CPI so we observe the deposited fees.
    ctx.accounts.vault_token_a.reload()?;
    ctx.accounts.vault_token_b.reload()?;
    let post_a = ctx.accounts.vault_token_a.amount;
    let post_b = ctx.accounts.vault_token_b.amount;

    let gross_a = post_a
        .checked_sub(pre_a)
        .ok_or(error!(AargauError::PostCpiBalanceDecreased))?;
    let gross_b = post_b
        .checked_sub(pre_b)
        .ok_or(error!(AargauError::PostCpiBalanceDecreased))?;

    let fee_rate_bps = ctx.accounts.protocol_config.fee_rate_bps;
    let fee_a = calc_aargau_fee(gross_a, fee_rate_bps)?;
    let fee_b = calc_aargau_fee(gross_b, fee_rate_bps)?;

    let token_program_key = ctx.accounts.token_program.key();
    let signer_arr: &[&[&[u8]]] = &[&seeds];

    transfer_performance_fee(
        token_program_key,
        ctx.accounts.vault_token_a.to_account_info(),
        ctx.accounts.mint_a.to_account_info(),
        ctx.accounts.treasury_token_a.to_account_info(),
        ctx.accounts.vault.to_account_info(),
        ctx.accounts.mint_a.decimals,
        fee_a,
        signer_arr,
    )?;
    transfer_performance_fee(
        token_program_key,
        ctx.accounts.vault_token_b.to_account_info(),
        ctx.accounts.mint_b.to_account_info(),
        ctx.accounts.treasury_token_b.to_account_info(),
        ctx.accounts.vault.to_account_info(),
        ctx.accounts.mint_b.decimals,
        fee_b,
        signer_arr,
    )?;

    let user_net_a = gross_a
        .checked_sub(fee_a)
        .ok_or(error!(AargauError::FeeExceedsGross))?;
    let user_net_b = gross_b
        .checked_sub(fee_b)
        .ok_or(error!(AargauError::FeeExceedsGross))?;

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

fn handle_increase_liquidity(
    ctx: &mut Context<ExecuteAction>,
    range_lower: i32,
    range_upper: i32,
    amount_a_max: u64,
    amount_b_max: u64,
    active_id: i32,
    active_id_slippage: u16,
) -> Result<()> {
    require!(
        amount_a_max > 0 || amount_b_max > 0,
        AargauError::InvalidActionPayload
    );
    require!(
        amount_a_max <= ctx.accounts.vault_token_a.amount,
        AargauError::InsufficientFunds
    );
    require!(
        amount_b_max <= ctx.accounts.vault_token_b.amount,
        AargauError::InsufficientFunds
    );
    // Cap the caller-supplied bin-slippage budget. Without an upper bound a
    // buggy or hostile caller could pass `u16::MAX` and effectively disable
    // Meteora's own slippage protection.
    require!(
        active_id_slippage <= MAX_ACTIVE_BIN_SLIPPAGE_HARD_CAP,
        AargauError::SlippageOutOfRange
    );

    let pre_a = ctx.accounts.vault_token_a.amount;
    let pre_b = ctx.accounts.vault_token_b.amount;

    let seed_bytes = vault_signer_seed_bytes(&ctx.accounts.vault);
    let seeds = vault_signer_seeds(&seed_bytes.user, &seed_bytes.pool, &seed_bytes.bump);

    // Known limitation: pools with a `bin_array_bitmap_extension` PDA — needed
    // when the active bin range exceeds what the inline LbPair bitmap can
    // address (~±5460 bins) — are not supported here. The DLMM program rejects
    // an `add_liquidity` that crosses bin chunks not covered by the inline
    // bitmap when this slot does not hold the real extension PDA. Tracked for
    // a follow-up that exposes it as an explicit `Option<UncheckedAccount>` on
    // this struct.
    let cpi = AddLiquidityByStrategy2Cpi {
        dlmm_program: ctx.accounts.dlmm_program.to_account_info(),
        position: ctx.accounts.position.to_account_info(),
        lb_pair: ctx.accounts.lb_pair.to_account_info(),
        bin_array_bitmap_extension: ctx.accounts.dlmm_program.to_account_info(),
        vault: ctx.accounts.vault.to_account_info(),
        vault_token_x: ctx.accounts.vault_token_a.to_account_info(),
        vault_token_y: ctx.accounts.vault_token_b.to_account_info(),
        reserve_x: ctx.accounts.reserve_x.to_account_info(),
        reserve_y: ctx.accounts.reserve_y.to_account_info(),
        token_x_mint: ctx.accounts.mint_a.to_account_info(),
        token_y_mint: ctx.accounts.mint_b.to_account_info(),
        token_program_x: ctx.accounts.token_program_x.to_account_info(),
        token_program_y: ctx.accounts.token_program_y.to_account_info(),
        event_authority: ctx.accounts.event_authority.to_account_info(),
        bin_array_lower: ctx.accounts.bin_array_lower.to_account_info(),
        bin_array_upper: ctx.accounts.bin_array_upper.to_account_info(),
    };
    invoke_add_liquidity_by_strategy2(
        &cpi,
        amount_a_max,
        amount_b_max,
        active_id,
        i32::from(active_id_slippage),
        range_lower,
        range_upper,
        METEORA_STRATEGY_SPOT_IMBALANCED,
        &seeds,
    )?;

    ctx.accounts.vault_token_a.reload()?;
    ctx.accounts.vault_token_b.reload()?;
    let consumed_a = pre_a
        .checked_sub(ctx.accounts.vault_token_a.amount)
        .ok_or(error!(AargauError::PostCpiBalanceDecreased))?;
    let consumed_b = pre_b
        .checked_sub(ctx.accounts.vault_token_b.amount)
        .ok_or(error!(AargauError::PostCpiBalanceDecreased))?;

    let clock = Clock::get()?;
    emit!(LiquidityChanged {
        vault: ctx.accounts.vault.key(),
        position_address: ctx.accounts.position.key(),
        op: LiquidityOp::Increase,
        amount_a: consumed_a,
        amount_b: consumed_b,
        timestamp: clock.unix_timestamp,
        triggered_by: TriggeredBy::User,
    });

    Ok(())
}

fn handle_decrease_liquidity(
    ctx: &mut Context<ExecuteAction>,
    range_lower: i32,
    range_upper: i32,
    bps: u16,
) -> Result<()> {
    require!(
        bps > 0 && bps <= BPS_DIVISOR_U16,
        AargauError::InvalidActionPayload
    );

    let pre_a = ctx.accounts.vault_token_a.amount;
    let pre_b = ctx.accounts.vault_token_b.amount;

    let seed_bytes = vault_signer_seed_bytes(&ctx.accounts.vault);
    let seeds = vault_signer_seeds(&seed_bytes.user, &seed_bytes.pool, &seed_bytes.bump);

    // Known limitation: pools with a `bin_array_bitmap_extension` PDA — needed
    // when the active bin range exceeds what the inline LbPair bitmap can
    // address (~±5460 bins) — are not supported here. The DLMM program rejects
    // a `remove_liquidity` that crosses bin chunks not covered by the inline
    // bitmap when this slot does not hold the real extension PDA. Tracked for
    // a follow-up that exposes it as an explicit `Option<UncheckedAccount>` on
    // this struct.
    let cpi = RemoveLiquidityByRange2Cpi {
        dlmm_program: ctx.accounts.dlmm_program.to_account_info(),
        position: ctx.accounts.position.to_account_info(),
        lb_pair: ctx.accounts.lb_pair.to_account_info(),
        bin_array_bitmap_extension: ctx.accounts.dlmm_program.to_account_info(),
        vault: ctx.accounts.vault.to_account_info(),
        vault_token_x: ctx.accounts.vault_token_a.to_account_info(),
        vault_token_y: ctx.accounts.vault_token_b.to_account_info(),
        reserve_x: ctx.accounts.reserve_x.to_account_info(),
        reserve_y: ctx.accounts.reserve_y.to_account_info(),
        token_x_mint: ctx.accounts.mint_a.to_account_info(),
        token_y_mint: ctx.accounts.mint_b.to_account_info(),
        token_program_x: ctx.accounts.token_program_x.to_account_info(),
        token_program_y: ctx.accounts.token_program_y.to_account_info(),
        memo_program: ctx.accounts.memo_program.to_account_info(),
        event_authority: ctx.accounts.event_authority.to_account_info(),
        bin_array_lower: ctx.accounts.bin_array_lower.to_account_info(),
        bin_array_upper: ctx.accounts.bin_array_upper.to_account_info(),
    };
    invoke_remove_liquidity_by_range2(&cpi, range_lower, range_upper, bps, &seeds)?;

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

    let clock = Clock::get()?;
    emit!(LiquidityChanged {
        vault: ctx.accounts.vault.key(),
        position_address: ctx.accounts.position.key(),
        op: LiquidityOp::Decrease,
        amount_a: received_a,
        amount_b: received_b,
        // Fees claimed implicitly by Meteora during remove stay in the vault
        // ATAs and are taken by the next CollectFees call — no performance fee
        // is charged at this stage.
        timestamp: clock.unix_timestamp,
        triggered_by: TriggeredBy::User,
    });

    Ok(())
}

fn handle_open_position(
    ctx: &mut Context<ExecuteAction>,
    lower_bin_id: i32,
    upper_bin_id: i32,
) -> Result<()> {
    // Vault must be idle — `position_address.is_none()` is the canonical
    // marker. Any other state (active position, pending rebalance) is
    // rejected upfront to keep the open/close lifecycle linear and
    // recoverable from off-chain observation alone.
    require!(
        ctx.accounts.vault.position_address.is_none(),
        AargauError::VaultHasActivePosition
    );
    require!(
        upper_bin_id >= lower_bin_id,
        AargauError::InvalidActionPayload
    );
    require_bin_count_within_cap(lower_bin_id, upper_bin_id)?;
    require_range_within_inline_bitmap(lower_bin_id, upper_bin_id)?;

    let seed_bytes = vault_signer_seed_bytes(&ctx.accounts.vault);
    let seeds = vault_signer_seeds(&seed_bytes.user, &seed_bytes.pool, &seed_bytes.bump);

    let cpi = InitializePositionCpi {
        dlmm_program: ctx.accounts.dlmm_program.to_account_info(),
        user_authority: ctx.accounts.user.to_account_info(),
        position: ctx.accounts.position.to_account_info(),
        lb_pair: ctx.accounts.lb_pair.to_account_info(),
        vault: ctx.accounts.vault.to_account_info(),
        system_program: ctx.accounts.system_program.to_account_info(),
        rent_sysvar: ctx.accounts.rent_sysvar.to_account_info(),
        event_authority: ctx.accounts.event_authority.to_account_info(),
    };
    invoke_initialize_position(&cpi, lower_bin_id, upper_bin_id, &seeds)?;

    // Persist the new position on the vault so subsequent arms can rely on
    // `position_address` + range fields without re-reading the DLMM account.
    let vault = &mut ctx.accounts.vault;
    vault.position_address = Some(ctx.accounts.position.key());
    vault.position_range_lower = Some(lower_bin_id);
    vault.position_range_upper = Some(upper_bin_id);

    let clock = Clock::get()?;
    emit!(PositionOpened {
        vault: vault.key(),
        position_address: ctx.accounts.position.key(),
        range_lower: lower_bin_id,
        range_upper: upper_bin_id,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

fn handle_close_position(ctx: &mut Context<ExecuteAction>) -> Result<()> {
    let position_key = ctx.accounts.position.key();

    let seed_bytes = vault_signer_seed_bytes(&ctx.accounts.vault);
    let seeds = vault_signer_seeds(&seed_bytes.user, &seed_bytes.pool, &seed_bytes.bump);

    // `close_position_if_empty` aborts if any per-bin liquidity share is
    // still non-zero, so the caller must have drained the position via
    // `DecreaseLiquidity { bps: 10_000 }` (and ideally a `CollectFees` to
    // sweep pending rewards) before reaching this arm.
    let cpi = ClosePositionIfEmptyCpi {
        dlmm_program: ctx.accounts.dlmm_program.to_account_info(),
        position: ctx.accounts.position.to_account_info(),
        vault: ctx.accounts.vault.to_account_info(),
        rent_receiver: ctx.accounts.user.to_account_info(),
        event_authority: ctx.accounts.event_authority.to_account_info(),
    };
    invoke_close_position_if_empty(&cpi, &seeds)?;

    let vault = &mut ctx.accounts.vault;
    vault.position_address = None;
    vault.position_range_lower = None;
    vault.position_range_upper = None;

    let clock = Clock::get()?;
    emit!(PositionClosed {
        vault: vault.key(),
        position_address: position_key,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Owned byte buffers for the vault PDA signer seeds. The seed slices
/// returned by `vault_signer_seeds` borrow from these buffers, so callers
/// must keep the struct alive for the duration of the CPI.
struct VaultSignerSeedBytes {
    user: [u8; 32],
    pool: [u8; 32],
    bump: [u8; 1],
}

/// Materialise the byte buffers backing the vault signer seeds. Centralised
/// to avoid the three-line incantation duplicated across every CPI handler.
fn vault_signer_seed_bytes(vault: &VaultAccount) -> VaultSignerSeedBytes {
    VaultSignerSeedBytes {
        user: vault.user_authority.to_bytes(),
        pool: vault.pool_address.to_bytes(),
        bump: [vault.bump],
    }
}

/// Transfer `amount` of `mint` from the vault ATA to the treasury ATA, signed
/// by the vault PDA. No-op when `amount == 0` to avoid useless CPIs and the
/// log noise they produce.
#[allow(clippy::too_many_arguments)]
fn transfer_performance_fee<'info>(
    token_program_key: Pubkey,
    from_vault_ata: AccountInfo<'info>,
    mint: AccountInfo<'info>,
    to_treasury_ata: AccountInfo<'info>,
    vault_authority: AccountInfo<'info>,
    decimals: u8,
    amount: u64,
    signer: &[&[&[u8]]],
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    token_interface::transfer_checked(
        CpiContext::new_with_signer(
            token_program_key,
            TransferChecked {
                from: from_vault_ata,
                mint,
                to: to_treasury_ata,
                authority: vault_authority,
            },
            signer,
        ),
        amount,
        decimals,
    )
}

/// Read `(position_range_lower, position_range_upper)` from the vault, failing
/// when no active position exists.
fn position_range(vault: &VaultAccount) -> Result<(i32, i32)> {
    let lower = vault
        .position_range_lower
        .ok_or(AargauError::VaultNoActivePosition)?;
    let upper = vault
        .position_range_upper
        .ok_or(AargauError::VaultNoActivePosition)?;
    Ok((lower, upper))
}

/// Parse the on-chain `LbPair` view, also enforcing the Anchor discriminator
/// as defence-in-depth (the account constraint already pins
/// `lb_pair.owner == DLMM_PROGRAM_ID`, but a wrong account type owned by the
/// same program would otherwise slip through and the CPI would fail with a
/// less actionable error downstream).
///
/// Thin wrapper around `parse_lb_pair_view_from_bytes` — the byte-level
/// parser lives in `utils::meteora::lb_pair_view` so it can be exercised
/// from the integration tests without needing a synthetic `AccountInfo`.
fn parse_lb_pair_view(lb_pair: &UncheckedAccount<'_>) -> Result<LbPairView> {
    let data = lb_pair.try_borrow_data()?;
    parse_lb_pair_view_from_bytes(&data)
}

/// Combined check for action arms that consume the existing position: the
/// `position` AccountInfo must match `vault.position_address` *and* be a
/// real `PositionV2` owned by the DLMM program. The key-equality check
/// alone is necessary but not sufficient — see `require_position_v2`.
fn require_active_position_v2(vault: &VaultAccount, position: &UncheckedAccount<'_>) -> Result<()> {
    let expected = vault
        .position_address
        .ok_or(AargauError::VaultNoActivePosition)?;
    require_keys_eq!(position.key(), expected, AargauError::VaultNoActivePosition);
    require_position_v2(&position.to_account_info())
}

/// Enforce the Meteora bin-count cap on `[lower, upper]` (inclusive).
/// Reuses the spec constant `MAX_METEORA_BINS` so the budget moves in one
/// place if Meteora ever raises the per-position cap.
fn require_bin_count_within_cap(lower_bin_id: i32, upper_bin_id: i32) -> Result<()> {
    let width = upper_bin_id
        .checked_sub(lower_bin_id)
        .and_then(|delta| delta.checked_add(1))
        .ok_or(AargauError::Overflow)?;
    require!(
        width > 0 && width <= MAX_METEORA_BINS,
        AargauError::TooManyBins
    );
    Ok(())
}

/// Reject ranges whose endpoints fall outside the inline `LbPair` bitmap.
/// Crossing the limit forces the DLMM program to consult the
/// `bin_array_bitmap_extension` PDA, which is not yet wired as a real
/// account (the slot is filled with the program-id placeholder).
/// Returning `BitmapExtensionRequired` upfront keeps the failure mode
/// observable instead of bubbling up an opaque DLMM error.
fn require_range_within_inline_bitmap(lower_bin_id: i32, upper_bin_id: i32) -> Result<()> {
    require!(
        lower_bin_id >= -METEORA_INLINE_BITMAP_BIN_LIMIT
            && upper_bin_id <= METEORA_INLINE_BITMAP_BIN_LIMIT,
        AargauError::BitmapExtensionRequired
    );
    Ok(())
}
