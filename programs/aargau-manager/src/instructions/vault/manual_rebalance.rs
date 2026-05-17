//! `manual_rebalance` — user-signed, single-tx Meteora DLMM rebalance.
//!
//! The handler runs two CPIs back-to-back in the same transaction:
//!
//!   1. `claim_fee2` against the **old** range, then transfers the
//!      performance-fee split to the protocol treasury ATAs (mirrors the
//!      pattern in `execute_action::handle_collect_fees`).
//!   2. `rebalance_liquidity` to atomically close the old bin range and
//!      re-open across the new range. `should_claim_fee` is pinned to the
//!      constant `METEORA_REBALANCE_SHOULD_CLAIM_FEE` (= `false`) — passing
//!      `true` here would re-deposit the fees we just extracted, double-
//!      crediting the user with revenue already routed to the treasury.
//!
//! Meteora is the only one of the three integrated DEXes that supports an
//! atomic rebalance: Orca and Raydium force a 2-tx (close+open) flow
//! because the position NFT mint changes. Meteora reuses the same
//! `PositionV2` account, so there is no NFT lifecycle to manage and no
//! `PendingRebalance` state to write. The instruction therefore asserts
//! `vault.pending_rebalance.is_none()` upfront and never sets it.
//!
//! ## Sovereignty
//!
//! `manual_rebalance` is a user-only entrypoint. The `user` signer must
//! equal `vault.user_authority`. The keeper bitmask in `allowed_ops` is
//! **deliberately not consulted** — the vault owner always retains
//! unconditional rebalance authority over their own position.
//!
//! ## ⚠ VERIFICATION GATE ⚠
//!
//! The `RebalanceLiquidity` instruction layout (account ordering + Borsh
//! payload) was assembled from secondary sources — public spec tables and
//! on-chain tx traces (Hawksight automation). Most wallet-mode tooling
//! composes `remove + add` as two separate txs instead, so there is no
//! widely-published byte-for-byte reference. The mollusk-svm replay test
//! against a pinned Meteora `.so` is the only gate that proves the layout
//! is wire-correct. **Do not promote this handler beyond local cluster
//! testing until that replay test exists and passes.** If the replay shows
//! mismatch, both this handler and the `rebalance_liquidity` helper in
//! `utils/meteora/` must be revisited together.

use crate::{
    constants::{
        BPS_DIVISOR_U16, MAX_ACTIVE_BIN_SLIPPAGE_HARD_CAP, MAX_METEORA_BINS, MEMO_PROGRAM_ID,
        METEORA_DLMM_PROGRAM_ID, METEORA_INLINE_BITMAP_BIN_LIMIT,
        METEORA_REBALANCE_SHOULD_CLAIM_FEE, TREASURY_SEED,
    },
    errors::AargauError,
    events::{FeesClaimed, RebalanceExecuted},
    state::{Protocol, ProtocolConfig, TriggeredBy, VaultAccount},
    utils::{
        fee::calc_aargau_fee,
        meteora::{
            add_liquidity::METEORA_STRATEGY_SPOT_IMBALANCED,
            claim_fee::{invoke_claim_fee2, ClaimFee2Cpi},
            lb_pair_view::{
                parse_lb_pair_view_from_bytes, require_lb_pair_bindings, require_spl_classic_mints,
                LbPairView,
            },
            position_view::require_position_v2,
            rebalance_liquidity::{
                invoke_rebalance_liquidity, RebalanceLiquidityArgs, RebalanceLiquidityCpi,
            },
        },
        signer_seeds::vault_signer_seeds,
    },
};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

/// Parameters for a single-tx Meteora DLMM rebalance.
///
/// `amount_a_max` / `amount_b_max` are slippage-tolerant caps on how many
/// tokens the vault is willing to commit to the new range. The DLMM program
/// will deposit at most this amount of each side; remainders stay in the
/// vault ATAs (consumed on a later `IncreaseLiquidity` or refunded by the
/// user via `withdraw`).
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct ManualRebalanceParams {
    /// New lower `bin_id` (inclusive). Must satisfy
    /// `new_upper_bin_id - new_lower_bin_id + 1 <= MAX_METEORA_BINS`.
    pub new_lower_bin_id: i32,
    /// New upper `bin_id` (inclusive).
    pub new_upper_bin_id: i32,
    /// Bins of tolerance around the on-chain `active_id` observed at quote
    /// time. Capped by `MAX_ACTIVE_BIN_SLIPPAGE_HARD_CAP` so a buggy or
    /// hostile caller cannot disable Meteora's own slippage protection.
    pub active_id_slippage: u16,
    /// Max amount of token A the vault is willing to commit to the new
    /// range. Must be `<= vault_token_a.amount` at handler entry.
    pub amount_a_max: u64,
    /// Max amount of token B the vault is willing to commit to the new
    /// range.
    pub amount_b_max: u64,
}

/// Accounts mirror `ExecuteAction` (the Meteora CPI surface is the same).
/// Treasury ATAs are required because step 1 collects fees and routes the
/// protocol split before the rebalance fires.
#[derive(Accounts)]
pub struct ManualRebalance<'info> {
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

    /// CHECK: must equal `vault.position_address` and pass the `PositionV2`
    /// discriminator check (enforced inside the handler — Anchor account
    /// constraints alone are not sufficient, see `require_position_v2`).
    #[account(mut)]
    pub position: UncheckedAccount<'info>,

    /// CHECK: pool reserve for token X — bound to `lb_pair` via the parsed
    /// `LbPairView`.
    #[account(mut)]
    pub reserve_x: UncheckedAccount<'info>,

    /// CHECK: pool reserve for token Y — bound to `lb_pair` via the parsed
    /// `LbPairView`.
    #[account(mut)]
    pub reserve_y: UncheckedAccount<'info>,

    /// Token program for token X. SPL classic only — the
    /// `require_spl_classic_mints` check rejects Token-2022 mints upfront.
    pub token_program_x: Interface<'info, TokenInterface>,
    pub token_program_y: Interface<'info, TokenInterface>,

    /// Token program used for the performance-fee transfers. Equals the
    /// program owning `mint_a` / `mint_b` (both must be SPL classic).
    pub token_program: Interface<'info, TokenInterface>,

    /// CHECK: must equal `MEMO_PROGRAM_ID` — required by `claim_fee2`.
    #[account(constraint = memo_program.key() == MEMO_PROGRAM_ID @ AargauError::InvalidPool)]
    pub memo_program: UncheckedAccount<'info>,

    /// CHECK: DLMM `__event_authority` PDA. Verified by the DLMM program.
    pub event_authority: UncheckedAccount<'info>,

    /// CHECK: must equal `METEORA_DLMM_PROGRAM_ID`.
    #[account(constraint = dlmm_program.key() == METEORA_DLMM_PROGRAM_ID @ AargauError::InvalidPool)]
    pub dlmm_program: UncheckedAccount<'info>,

    /// CHECK: BinArray pair covering the **old** position range. The CPI
    /// helpers verify PDA derivation.
    #[account(mut)]
    pub bin_array_lower_old: UncheckedAccount<'info>,
    /// CHECK: BinArray PDA = `bin_array_lower_old_index + 1`.
    #[account(mut)]
    pub bin_array_upper_old: UncheckedAccount<'info>,

    /// CHECK: BinArray pair covering the **new** position range. May overlap
    /// the old pair when the new range partially covers the old one;
    /// deduplication is Meteora's responsibility inside the DLMM program.
    #[account(mut)]
    pub bin_array_lower_new: UncheckedAccount<'info>,
    /// CHECK: BinArray PDA = `bin_array_lower_new_index + 1`.
    #[account(mut)]
    pub bin_array_upper_new: UncheckedAccount<'info>,
}

pub fn handler(mut ctx: Context<ManualRebalance>, params: ManualRebalanceParams) -> Result<()> {
    // Currently Meteora-only — Orca / Raydium rebalance uses the 2-tx
    // PendingRebalance flow and is handled in separate instructions.
    require!(
        ctx.accounts.vault.protocol == Protocol::Meteora,
        AargauError::InvalidPool
    );
    // Meteora invariant: pending_rebalance is always `None` (no 2-tx flow).
    require!(
        ctx.accounts.vault.pending_rebalance.is_none(),
        AargauError::PendingRebalanceExists
    );

    // Parse the LbPair once; binds mints + reserves to the pool and yields
    // the on-chain `active_id` consumed by the rebalance call.
    let lb_pair_view = parse_lb_pair_view(&ctx.accounts.lb_pair)?;
    require_lb_pair_bindings(
        &lb_pair_view,
        &ctx.accounts.mint_a.key(),
        &ctx.accounts.mint_b.key(),
        &ctx.accounts.reserve_x.key(),
        &ctx.accounts.reserve_y.key(),
    )?;
    require_spl_classic_mints(
        ctx.accounts.mint_a.to_account_info().owner,
        ctx.accounts.mint_b.to_account_info().owner,
        &ctx.accounts.token_program.key(),
    )?;

    // The existing position must be a real `PositionV2` owned by the DLMM
    // program (key-equality alone is necessary but not sufficient).
    require_active_position_v2(&ctx.accounts.vault, &ctx.accounts.position)?;

    let (old_lower_bin_id, old_upper_bin_id) = position_range(&ctx.accounts.vault)?;
    validate_rebalance_params(&params)?;

    // ── Step 1: claim_fee2 + treasury split ────────────────────────────────
    claim_fees_and_route_to_treasury(&mut ctx, old_lower_bin_id, old_upper_bin_id)?;

    // ── Step 2: rebalance_liquidity (should_claim_fee = false) ─────────────
    execute_rebalance_cpi(&ctx, old_lower_bin_id, lb_pair_view.active_id, &params)?;

    // Update vault state for the new range.
    ctx.accounts.vault.position_range_lower = Some(params.new_lower_bin_id);
    ctx.accounts.vault.position_range_upper = Some(params.new_upper_bin_id);
    let clock = Clock::get()?;
    ctx.accounts.vault.last_rebalance_at = clock.unix_timestamp;

    emit!(RebalanceExecuted {
        vault: ctx.accounts.vault.key(),
        protocol: Protocol::Meteora,
        old_range_lower: old_lower_bin_id,
        old_range_upper: old_upper_bin_id,
        new_range_lower: params.new_lower_bin_id,
        new_range_upper: params.new_upper_bin_id,
        triggered_by: TriggeredBy::User,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// Step helpers
// ---------------------------------------------------------------------------

fn validate_rebalance_params(params: &ManualRebalanceParams) -> Result<()> {
    require!(
        params.amount_a_max > 0 || params.amount_b_max > 0,
        AargauError::InvalidActionPayload
    );
    require!(
        params.active_id_slippage <= MAX_ACTIVE_BIN_SLIPPAGE_HARD_CAP,
        AargauError::SlippageOutOfRange
    );
    // The bps_to_remove on the (implicit) old-range close inside
    // `rebalance_liquidity` is the full position — there is no caller knob.
    // The slippage cap above plus the bin-count / bitmap checks below are
    // the only protection on the new range.
    require_bin_count_within_cap(params.new_lower_bin_id, params.new_upper_bin_id)?;
    require_range_within_inline_bitmap(params.new_lower_bin_id, params.new_upper_bin_id)?;
    // `BPS_DIVISOR_U16` is referenced only to prove the import path is wired
    // for parity with `execute_action`; the rebalance itself has no
    // user-facing bps knob today (the close-side bps is fixed at 100%).
    let _ = BPS_DIVISOR_U16;
    Ok(())
}

fn claim_fees_and_route_to_treasury(
    ctx: &mut Context<ManualRebalance>,
    old_lower_bin_id: i32,
    old_upper_bin_id: i32,
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
        bin_array_lower: ctx.accounts.bin_array_lower_old.to_account_info(),
        bin_array_upper: ctx.accounts.bin_array_upper_old.to_account_info(),
    };
    invoke_claim_fee2(&cpi, old_lower_bin_id, old_upper_bin_id, &seeds)?;

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

    // Refresh balances after the fee transfer so the subsequent rebalance
    // CPI sees the correct vault holdings (treasury split already removed).
    ctx.accounts.vault_token_a.reload()?;
    ctx.accounts.vault_token_b.reload()?;

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

fn execute_rebalance_cpi(
    ctx: &Context<ManualRebalance>,
    old_lower_bin_id: i32,
    on_chain_active_id: i32,
    params: &ManualRebalanceParams,
) -> Result<()> {
    let seed_bytes = vault_signer_seed_bytes(&ctx.accounts.vault);
    let seeds = vault_signer_seeds(&seed_bytes.user, &seed_bytes.pool, &seed_bytes.bump);

    // Known limitation: pools requiring a real `bin_array_bitmap_extension`
    // PDA are rejected upfront by `require_range_within_inline_bitmap`. The
    // slot is filled with `dlmm_program` as a placeholder, matching the
    // pattern in `execute_action` and the Meteora CPI helpers.
    let cpi = RebalanceLiquidityCpi {
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
        bin_array_lower_old: ctx.accounts.bin_array_lower_old.to_account_info(),
        bin_array_upper_old: ctx.accounts.bin_array_upper_old.to_account_info(),
        bin_array_lower_new: ctx.accounts.bin_array_lower_new.to_account_info(),
        bin_array_upper_new: ctx.accounts.bin_array_upper_new.to_account_info(),
    };

    let args = RebalanceLiquidityArgs {
        old_lower_bin_id,
        new_lower_bin_id: params.new_lower_bin_id,
        new_upper_bin_id: params.new_upper_bin_id,
        amount_x: params.amount_a_max,
        amount_y: params.amount_b_max,
        active_id: on_chain_active_id,
        max_active_bin_slippage: i32::from(params.active_id_slippage),
        strategy_type: METEORA_STRATEGY_SPOT_IMBALANCED,
        // Pinned by constant — see module doc and constants.rs.
        should_claim_fee: METEORA_REBALANCE_SHOULD_CLAIM_FEE,
    };
    invoke_rebalance_liquidity(&cpi, &args, &seeds)
}

// ---------------------------------------------------------------------------
// Local utilities — mirror the private helpers in execute_action.rs so the
// two instructions can be tested in isolation without leaking handler-only
// helpers into the public surface.
// ---------------------------------------------------------------------------

/// Owned byte buffers backing the vault PDA signer seeds. The slices
/// returned by `vault_signer_seeds` borrow from these buffers; the struct
/// must outlive the CPI call.
struct VaultSignerSeedBytes {
    user: [u8; 32],
    pool: [u8; 32],
    bump: [u8; 1],
}

fn vault_signer_seed_bytes(vault: &VaultAccount) -> VaultSignerSeedBytes {
    VaultSignerSeedBytes {
        user: vault.user_authority.to_bytes(),
        pool: vault.pool_address.to_bytes(),
        bump: [vault.bump],
    }
}

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

fn position_range(vault: &VaultAccount) -> Result<(i32, i32)> {
    let lower = vault
        .position_range_lower
        .ok_or(AargauError::VaultNoActivePosition)?;
    let upper = vault
        .position_range_upper
        .ok_or(AargauError::VaultNoActivePosition)?;
    Ok((lower, upper))
}

fn parse_lb_pair_view(lb_pair: &UncheckedAccount<'_>) -> Result<LbPairView> {
    let data = lb_pair.try_borrow_data()?;
    parse_lb_pair_view_from_bytes(&data)
}

fn require_active_position_v2(vault: &VaultAccount, position: &UncheckedAccount<'_>) -> Result<()> {
    let expected = vault
        .position_address
        .ok_or(AargauError::VaultNoActivePosition)?;
    require_keys_eq!(position.key(), expected, AargauError::VaultNoActivePosition);
    require_position_v2(&position.to_account_info())
}

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

fn require_range_within_inline_bitmap(lower_bin_id: i32, upper_bin_id: i32) -> Result<()> {
    require!(
        lower_bin_id >= -METEORA_INLINE_BITMAP_BIN_LIMIT
            && upper_bin_id <= METEORA_INLINE_BITMAP_BIN_LIMIT,
        AargauError::BitmapExtensionRequired
    );
    Ok(())
}
