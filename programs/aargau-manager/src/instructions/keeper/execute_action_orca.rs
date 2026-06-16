//! `execute_action_orca` — user-signed entry point for position-lifecycle
//! operations on an Orca Whirlpools vault.
//!
//! Scope: the five position-management ops — `OpenPosition`,
//! `IncreaseLiquidity`, `DecreaseLiquidity`, `CollectFees` (fees + rewards),
//! `ClosePosition`. The vault PDA is the `position_authority` for every CPI;
//! performance fees on collected LP fees are routed to the protocol treasury
//! ATAs.
//!
//! Custody invariant: the position NFT (Token-2022, supply 1) lives in the
//! vault PDA's ATA and never touches the user wallet. On open `owner` = vault
//! PDA; on close the NFT is burned and rent returns to the user.
//!
//! Two-phase `PendingRebalance` rebalance + recovery is out of scope here.
//! The struct requires the vault owner (`user`) to sign — `triggered_by` is
//! always `User` for emitted events.
//!
//! Token-2022 pair mints are accepted (per-mint detection picks the token
//! program). Transfer hooks are not wired: the v2 CPIs serialise
//! `remaining_accounts_info` as `None`, which is correct for plain Token-2022
//! (transfer-fee-only) mints.

use crate::{
    constants::{
        MEMO_PROGRAM_ID, ORCA_METADATA_UPDATE_AUTH, ORCA_REWARD_SLOTS, ORCA_WHIRLPOOL_PROGRAM_ID,
        TOKEN_2022_PROGRAM_ID, TREASURY_SEED,
    },
    errors::AargauError,
    events::{FeesClaimed, LiquidityChanged, LiquidityOp, PositionClosed, PositionOpened},
    state::{OrcaKeeperAction, Protocol, ProtocolConfig, TriggeredBy, VaultAccount},
    utils::{
        fee::calc_aargau_fee,
        orca::{
            accounts::{
                derive_position_pda, derive_tick_array_pda, is_token_2022,
                read_transfer_fee_config, require_orca_position, require_reward_owner_is_vault_ata,
                require_v2_transferable_mint, tick_array_start_index,
            },
            close_position::{
                invoke_close_position_with_token_extensions, ClosePositionWithTokenExtensionsCpi,
            },
            collect_fees::{
                invoke_collect_fees_v2, invoke_collect_reward_v2, CollectFeesV2Cpi,
                CollectRewardV2Cpi,
            },
            decrease_liquidity::{invoke_decrease_liquidity_v2, DecreaseLiquidityV2Cpi},
            increase_liquidity::{invoke_increase_liquidity_v2, IncreaseLiquidityV2Cpi},
            open_position::{
                invoke_open_position_with_token_extensions, OpenPositionWithTokenExtensionsCpi,
            },
            whirlpool_view::{
                parse_whirlpool_view_from_bytes, require_whirlpool_bindings, WhirlpoolView,
            },
        },
        signer_seeds::vault_signer_seeds,
    },
};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked},
};

/// Number of accounts the caller appends to `remaining_accounts` per active
/// reward slot for `CollectFees`: reward_owner ATA, reward_mint, reward_vault,
/// reward_token_program (in that order).
const ORCA_REWARD_ACCOUNTS_PER_SLOT: usize = 4;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct ExecuteActionOrcaParams {
    pub action: OrcaKeeperAction,
}

/// User-signed instruction. `protocol_config` enforces the global pause
/// kill-switch and provides `fee_rate_bps` for the treasury split on
/// `CollectFees`. Treasury ATAs are required on every call to keep the IDL
/// stable; only `CollectFees` writes to them.
///
/// Two token programs (`token_program_a`/`b`) carry the per-mint SPL-or-
/// Token-2022 program; each Aargau fee leg routes its `transfer_checked`
/// through the program that owns that leg's mint. The standalone
/// `token_program` account is reserved/unused (kept to preserve the wire
/// shape for off-chain builders). The position NFT mint + its ATA are always
/// under Token-2022.
#[derive(Accounts)]
pub struct ExecuteActionOrca<'info> {
    /// Vault owner — must equal `vault.user_authority`. Funds the position on
    /// open and receives rent on close.
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

    /// CHECK: the `Position` account. For `OpenPosition` this is the (uninit)
    /// PDA `[b"position", position_mint]` initialised by the CPI; for every
    /// other arm it must equal `vault.position_address` and pass the `Position`
    /// discriminator check — enforced per-arm inside the handler.
    #[account(mut)]
    pub position: UncheckedAccount<'info>,

    /// CHECK: the Token-2022 position NFT mint. On `OpenPosition` this is the
    /// ephemeral mint keypair co-signed off-chain; otherwise it must equal
    /// `vault.position_mint`. The PDA/derivation binding is checked per-arm.
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
    /// Reserved — currently unused. Performance-fee transfers now route each
    /// mint leg through its per-mint program (`token_program_a` / `_b`), since a
    /// mixed Whirlpools pool can have the two mints under different token
    /// programs. Kept in the account list to keep the instruction wire shape
    /// stable for off-chain instruction builders.
    pub token_program: Interface<'info, TokenInterface>,

    /// CHECK: must equal the Token-2022 program (position NFT mint + ATA).
    #[account(constraint = token_2022_program.key() == TOKEN_2022_PROGRAM_ID @ AargauError::InvalidPool)]
    pub token_2022_program: UncheckedAccount<'info>,

    /// CHECK: must equal `MEMO_PROGRAM_ID`.
    #[account(constraint = memo_program.key() == MEMO_PROGRAM_ID @ AargauError::InvalidPool)]
    pub memo_program: UncheckedAccount<'info>,

    /// CHECK: must equal `ORCA_WHIRLPOOL_PROGRAM_ID`.
    #[account(constraint = whirlpool_program.key() == ORCA_WHIRLPOOL_PROGRAM_ID @ AargauError::InvalidPool)]
    pub whirlpool_program: UncheckedAccount<'info>,

    /// CHECK: position-NFT metadata update authority — account #9 of
    /// `open_position_with_token_extensions`. Pinned to the Whirlpools const.
    /// Only read by `OpenPosition`; required on every call to keep the IDL
    /// stable.
    #[account(constraint = metadata_update_auth.key() == ORCA_METADATA_UPDATE_AUTH @ AargauError::InvalidPool)]
    pub metadata_update_auth: UncheckedAccount<'info>,

    /// CHECK: TickArray covering the lower tick — pre-initialised off-chain.
    /// The PDA binding is validated per-arm against the position range.
    #[account(mut)]
    pub tick_array_lower: UncheckedAccount<'info>,

    /// CHECK: TickArray covering the upper tick — pre-initialised off-chain.
    #[account(mut)]
    pub tick_array_upper: UncheckedAccount<'info>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    // For `CollectFees`, per active reward slot the caller appends 4 accounts
    // to `remaining_accounts`: reward_owner ATA, reward_mint, reward_vault,
    // reward_token_program.
}

pub fn handler<'info>(
    mut ctx: Context<'info, ExecuteActionOrca<'info>>,
    params: ExecuteActionOrcaParams,
) -> Result<()> {
    require!(
        ctx.accounts.vault.protocol == Protocol::Orca,
        AargauError::InvalidPool
    );
    require!(
        ctx.accounts.vault.pending_rebalance.is_none(),
        AargauError::PendingRebalanceExists
    );

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

    match params.action {
        OrcaKeeperAction::OpenPosition {
            tick_lower,
            tick_upper,
        } => handle_open_position(&mut ctx, &view, tick_lower, tick_upper),
        OrcaKeeperAction::IncreaseLiquidity {
            liquidity_amount,
            token_max_a,
            token_max_b,
        } => {
            require_active_position(&ctx.accounts.vault, &ctx.accounts.position)?;
            require_tick_arrays_bound(&ctx, &view)?;
            handle_increase_liquidity(&mut ctx, liquidity_amount, token_max_a, token_max_b)
        }
        OrcaKeeperAction::DecreaseLiquidity {
            liquidity_amount,
            token_min_a,
            token_min_b,
        } => {
            require_active_position(&ctx.accounts.vault, &ctx.accounts.position)?;
            require_tick_arrays_bound(&ctx, &view)?;
            handle_decrease_liquidity(&mut ctx, liquidity_amount, token_min_a, token_min_b)
        }
        OrcaKeeperAction::CollectFees => {
            require_active_position(&ctx.accounts.vault, &ctx.accounts.position)?;
            handle_collect_fees(&mut ctx, &view)
        }
        OrcaKeeperAction::ClosePosition => {
            require_active_position(&ctx.accounts.vault, &ctx.accounts.position)?;
            handle_close_position(&mut ctx)
        }
    }
}

// ---------------------------------------------------------------------------
// Action handlers
// ---------------------------------------------------------------------------

fn handle_open_position(
    ctx: &mut Context<ExecuteActionOrca>,
    view: &WhirlpoolView,
    tick_lower: i32,
    tick_upper: i32,
) -> Result<()> {
    require!(
        ctx.accounts.vault.position_address.is_none(),
        AargauError::VaultHasActivePosition
    );
    require!(tick_upper > tick_lower, AargauError::InvalidActionPayload);

    // Bind the position PDA to the NFT mint and the tick arrays to the range.
    let expected_position = derive_position_pda(&ctx.accounts.position_mint.key());
    require_keys_eq!(
        ctx.accounts.position.key(),
        expected_position,
        AargauError::InvalidVaultPda
    );
    require_tick_array(
        &ctx.accounts.whirlpool.key(),
        tick_lower,
        view.tick_spacing,
        &ctx.accounts.tick_array_lower.key(),
    )?;
    require_tick_array(
        &ctx.accounts.whirlpool.key(),
        tick_upper,
        view.tick_spacing,
        &ctx.accounts.tick_array_upper.key(),
    )?;

    // Reject pair mints carrying a TransferHook or NonTransferable extension.
    // The vault's v2 CPIs serialise `remaining_accounts_info = None`, so such a
    // position could be opened but never decreased / fee-collected / closed —
    // funds would be locked. Gate at open before any state is written.
    require_pair_mints_v2_transferable(ctx)?;

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

    // Snapshot per-mint transfer-fee config so the fee path can account for
    // Token-2022 deductions on later collect transfers.
    snapshot_transfer_fees(ctx)?;

    let vault = &mut ctx.accounts.vault;
    vault.position_address = Some(ctx.accounts.position.key());
    vault.position_mint = Some(ctx.accounts.position_mint.key());
    vault.position_range_lower = Some(tick_lower);
    vault.position_range_upper = Some(tick_upper);

    let clock = Clock::get()?;
    emit!(PositionOpened {
        vault: vault.key(),
        position_address: ctx.accounts.position.key(),
        range_lower: tick_lower,
        range_upper: tick_upper,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

fn handle_increase_liquidity<'info>(
    ctx: &mut Context<'info, ExecuteActionOrca<'info>>,
    liquidity_amount: u128,
    token_max_a: u64,
    token_max_b: u64,
) -> Result<()> {
    require!(liquidity_amount > 0, AargauError::InvalidActionPayload);
    require!(
        token_max_a > 0 || token_max_b > 0,
        AargauError::InvalidActionPayload
    );
    require!(
        token_max_a <= ctx.accounts.vault_token_a.amount,
        AargauError::InsufficientFunds
    );
    require!(
        token_max_b <= ctx.accounts.vault_token_b.amount,
        AargauError::InsufficientFunds
    );

    let pre_a = ctx.accounts.vault_token_a.amount;
    let pre_b = ctx.accounts.vault_token_b.amount;

    let seed_bytes = VaultSignerSeedBytes::new(&ctx.accounts.vault);
    let seeds = seed_bytes.seeds();

    let cpi = modify_liquidity_cpi(ctx);
    invoke_increase_liquidity_v2(&cpi, liquidity_amount, token_max_a, token_max_b, &seeds)?;

    ctx.accounts.vault_token_a.reload()?;
    ctx.accounts.vault_token_b.reload()?;
    let consumed_a = pre_a.saturating_sub(ctx.accounts.vault_token_a.amount);
    let consumed_b = pre_b.saturating_sub(ctx.accounts.vault_token_b.amount);

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

fn handle_decrease_liquidity<'info>(
    ctx: &mut Context<'info, ExecuteActionOrca<'info>>,
    liquidity_amount: u128,
    token_min_a: u64,
    token_min_b: u64,
) -> Result<()> {
    require!(liquidity_amount > 0, AargauError::InvalidActionPayload);

    let pre_a = ctx.accounts.vault_token_a.amount;
    let pre_b = ctx.accounts.vault_token_b.amount;

    let seed_bytes = VaultSignerSeedBytes::new(&ctx.accounts.vault);
    let seeds = seed_bytes.seeds();

    let cpi: DecreaseLiquidityV2Cpi = modify_liquidity_cpi(ctx);
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

    // Caller-supplied slippage floors. The v2 CPI also enforces them, but we
    // re-check on the observed delta as defence-in-depth.
    require!(received_a >= token_min_a, AargauError::SlippageExceeded);
    require!(received_b >= token_min_b, AargauError::SlippageExceeded);

    let clock = Clock::get()?;
    emit!(LiquidityChanged {
        vault: ctx.accounts.vault.key(),
        position_address: ctx.accounts.position.key(),
        op: LiquidityOp::Decrease,
        amount_a: received_a,
        amount_b: received_b,
        timestamp: clock.unix_timestamp,
        triggered_by: TriggeredBy::User,
    });

    Ok(())
}

fn handle_collect_fees<'info>(
    ctx: &mut Context<'info, ExecuteActionOrca<'info>>,
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

    // Measure the LP-fee delta NOW — before rewards are collected. When a
    // reward mint equals a pair mint, its `collect_reward_v2` destination ATA
    // is `vault_token_a`/`vault_token_b`, so collecting rewards first would
    // fold reward proceeds into the gross figures and tax them at the LP
    // performance-fee rate (over-charging the user) and mix fees + rewards in
    // the emitted `FeesClaimed.gross_*`. Snapshot the delta against the
    // pre-collect baseline so the fee base reflects ONLY true LP fees.
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

    // Sweep active reward slots AFTER the fee base is fixed. The caller appends
    // 4 accounts per active slot to `remaining_accounts`; we match them
    // positionally against the whirlpool's reward triplets (active slots =
    // vault != default). Reward proceeds are not subject to the LP fee.
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

    // Each mint leg uses its own token program: `mint_a` and `mint_b` may live
    // under different token programs (one SPL-classic, one Token-2022) in a
    // mixed Whirlpools pool. The transferring program must own the mint, so the
    // fee CPI for each leg is routed through that leg's per-mint program.
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

fn handle_close_position(ctx: &mut Context<ExecuteActionOrca>) -> Result<()> {
    let position_key = ctx.accounts.position.key();

    // The NFT mint must match what the vault recorded on open.
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
    invoke_close_position_with_token_extensions(&cpi, &seeds)?;

    let vault = &mut ctx.accounts.vault;
    vault.position_address = None;
    vault.position_mint = None;
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
// Reward collection
// ---------------------------------------------------------------------------

/// Run `collect_reward_v2` once per active reward slot. Active slots are the
/// whirlpool reward triplets whose `vault != Pubkey::default()`. The caller
/// appends, per active slot in slot order, 4 accounts to `remaining_accounts`:
/// `[reward_owner_account, reward_mint, reward_vault, reward_token_program]`.
/// The reward_vault account is bound to the whirlpool's on-chain reward vault.
fn collect_rewards<'info>(
    ctx: &Context<'info, ExecuteActionOrca<'info>>,
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

        // Bind the passed reward vault + mint to the whirlpool's on-chain
        // reward triplet for this index — without this a caller could route
        // a reward collect into an arbitrary vault.
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

        // Custody bind: rewards must land in the vault's own ATA for the reward
        // mint, never an arbitrary account. The destination is supplied via
        // `remaining_accounts` (unchecked by Anchor).
        require_reward_owner_is_vault_ata(
            &reward_owner_account.key(),
            &ctx.accounts.vault.key(),
            &reward_mint.key(),
            &reward_token_program.key(),
        )?;

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

/// Owned byte buffers backing the vault PDA signer seeds. The seed slices
/// borrow from these buffers, so the struct must outlive the CPI.
struct VaultSignerSeedBytes {
    user: [u8; 32],
    pool: [u8; 32],
    bump: [u8; 1],
}

impl VaultSignerSeedBytes {
    fn new(vault: &VaultAccount) -> Self {
        Self {
            user: vault.user_authority.to_bytes(),
            pool: vault.pool_address.to_bytes(),
            bump: [vault.bump],
        }
    }

    fn seeds(&self) -> [&[u8]; 4] {
        vault_signer_seeds(&self.user, &self.pool, &self.bump)
    }
}

/// Assemble the shared ModifyLiquidityV2 CPI struct (increase + decrease).
fn modify_liquidity_cpi<'info>(
    ctx: &Context<'info, ExecuteActionOrca<'info>>,
) -> IncreaseLiquidityV2Cpi<'info> {
    IncreaseLiquidityV2Cpi {
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
    }
}

/// Transfer `amount` of `mint` from the vault ATA to the treasury ATA, signed
/// by the vault PDA. No-op when `amount == 0`.
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

/// Snapshot the Token-2022 transfer-fee config for both pair mints onto the
/// vault. `uses_token_2022` is set when either mint is Token-2022. SPL classic
/// mints (or Token-2022 without the extension) leave the fields at zero.
/// Reject either pair mint if it is a Token-2022 mint carrying a TransferHook
/// or NonTransferable extension (see `require_v2_transferable_mint`). SPL
/// classic mints are skipped — they have no extension TLV.
fn require_pair_mints_v2_transferable(ctx: &Context<ExecuteActionOrca>) -> Result<()> {
    for mint in [&ctx.accounts.mint_a, &ctx.accounts.mint_b] {
        let info = mint.to_account_info();
        if is_token_2022(info.owner) {
            let data = info.try_borrow_data()?;
            require_v2_transferable_mint(&data)?;
        }
    }
    Ok(())
}

fn snapshot_transfer_fees(ctx: &mut Context<ExecuteActionOrca>) -> Result<()> {
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

/// Validate that both tick-array accounts match the PDAs derived from the
/// active position's stored range and the pool's tick spacing.
fn require_tick_arrays_bound(ctx: &Context<ExecuteActionOrca>, view: &WhirlpoolView) -> Result<()> {
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

/// Bind a passed tick-array account to the PDA covering `tick`.
fn require_tick_array(
    whirlpool: &Pubkey,
    tick: i32,
    tick_spacing: u16,
    passed: &Pubkey,
) -> Result<()> {
    let start = tick_array_start_index(tick, tick_spacing)?;
    let expected = derive_tick_array_pda(whirlpool, start);
    require_keys_eq!(*passed, expected, AargauError::InvalidPool);
    Ok(())
}
