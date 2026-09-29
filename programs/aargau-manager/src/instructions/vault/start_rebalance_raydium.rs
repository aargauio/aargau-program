//! `start_rebalance_raydium` — phase one of a two-transaction Raydium CLMM
//! rebalance (CloseOld). User-signed; the vault PDA owns the position NFT.
//!
//! A Raydium rebalance burns the old position NFT and mints a new one, which
//! does not fit in one transaction. This instruction runs the close leg and
//! stores the target range in `vault.pending_rebalance`;
//! `retry_pending_rebalance_raydium` runs the open leg. While
//! `pending_rebalance.is_some()` the drained tokens sit in the vault ATAs and
//! stay withdrawable, and `execute_action_raydium` is blocked.
//!
//! Sequence (one transaction):
//!   1. zero-liquidity `decrease_liquidity_v2`: collects every LP fee and
//!      reward owed; the performance fee on the LP-fee part goes to the
//!      treasury (`FeesClaimed`). Same fee base as `execute_action_raydium`:
//!      the pool-vault outflow, so rewards are never taxed.
//!   2. `decrease_liquidity_v2` for the position's **full on-chain
//!      liquidity**, read from `PersonalPositionState` (no caller-supplied
//!      amount), with min-out floors checked on the observed credit. Skipped
//!      when the position is already empty.
//!   3. `close_position` (burns the NFT); Raydium refunds the rent to the vault
//!      PDA and it is swept to the user.
//!   4. clear `position_*`, write `pending_rebalance`, emit `RebalancePending`.
//!
//! The target range is checked up front against the pool tick spacing and
//! Raydium's tick domain, so a pending range is always one the open leg can
//! open. The tick arrays passed here cover the OLD (stored) range.
//!
//! ## Pause column: gated (entry)
//!
//! `protocol_config.is_paused` blocks this instruction, as it blocks
//! `start_rebalance_orca`: it starts a transition whose only purpose is to
//! reopen a position, and the paired open leg is gated too. Starting one
//! during a pause would leave the vault pending with no way forward but
//! cancel. The exit paths stay open: `withdraw`, `emergency_withdraw` and
//! `cancel_pending_rebalance` are never gated.
//!
//! ## ⚠ VERIFICATION GATE ⚠
//!
//! The Raydium CLMM account orders, Borsh payloads and fee/reward payout
//! behaviour were taken from the program source, not from a replay. The
//! mollusk-svm replay of start → retry, start → failed retry → retry and
//! start → cancel → withdraw against the pinned mainnet `raydium_clmm.so` is
//! the only gate that proves this handler wire-correct and within the compute
//! budget. **Do not promote this instruction beyond local testing until that
//! replay exists and passes.**

use crate::{
    constants::{
        MEMO_PROGRAM_ID, RAYDIUM_CLMM_PROGRAM_ID, SPL_TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID,
        TREASURY_SEED,
    },
    errors::AargauError,
    events::RebalancePending,
    state::{PendingRebalance, Protocol, ProtocolConfig, TriggeredBy, VaultAccount},
    utils::{
        raydium::{
            accounts::{
                derive_position_nft_account, require_tick_array,
                require_tick_array_bitmap_extension,
            },
            personal_position_view::{
                read_personal_position_view, require_personal_position_bindings,
                PersonalPositionView,
            },
            pool_state_view::{read_pool_state_view, require_pool_state_bindings, PoolStateView},
            position_guards::{
                require_full_drain_floor, require_openable_tick_range,
                require_raydium_position_empty,
            },
            reward_accounts::{bind_reward_transfer_accounts, RewardTransferAccounts},
            vault_position::{
                close_empty_position, collect_lp_fees_to_treasury, decrease_principal_to_vault,
                PairLegAccounts, PositionPayoutAccounts, RaydiumPositionAccounts,
                TreasuryFeeAccounts,
            },
        },
        signer_seeds::VaultSignerSeedBytes,
        vault_ops::sweep_excess_vault_lamports,
    },
};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct StartRebalanceRaydiumParams {
    /// Target lower tick of the new range, persisted into `pending_rebalance`
    /// for the open leg. Must be a multiple of the pool tick spacing.
    pub new_tick_lower: i32,
    /// Target upper tick of the new range.
    pub new_tick_upper: i32,
    /// Slippage floor for token A principal returned by the drain.
    pub token_min_a: u64,
    /// Slippage floor for token B principal returned by the drain.
    pub token_min_b: u64,
}

/// Close-side Raydium account set: the collect / decrease / close subset of
/// `ExecuteActionRaydium`, with the tick arrays of the OLD range. Treasury
/// ATAs receive the performance fee.
///
/// `remaining_accounts`: exactly one `[reward_vault, recipient, reward_mint]`
/// triple per initialized pool reward slot, in slot order; each recipient must
/// be the vault's ATA for the reward mint.
///
/// The deserialized accounts are boxed to keep the SBF stack frame small.
/// Boxing changes neither the wire format nor the IDL.
#[derive(Accounts)]
pub struct StartRebalanceRaydium<'info> {
    /// Vault owner — must equal `vault.user_authority`. Receives the swept
    /// position rent.
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        seeds = [ProtocolConfig::SEEDS],
        bump = protocol_config.bump,
        constraint = !protocol_config.is_paused @ AargauError::ProgramPaused,
    )]
    pub protocol_config: Box<Account<'info, ProtocolConfig>>,

    #[account(
        mut,
        seeds = [VaultAccount::SEEDS, vault.user_authority.as_ref(), vault.pool_address.as_ref()],
        bump = vault.bump,
        constraint = vault.user_authority == user.key() @ AargauError::UnauthorizedUser,
    )]
    pub vault: Box<Account<'info, VaultAccount>>,

    /// Pool mint 0 — bound to `pool_state.token_mint_0` in the handler.
    pub mint_a: Box<InterfaceAccount<'info, Mint>>,
    /// Pool mint 1 — bound to `pool_state.token_mint_1` in the handler.
    pub mint_b: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        mut,
        constraint = vault_token_a.owner == vault.key() @ AargauError::InvalidVaultPda,
        constraint = vault_token_a.mint == mint_a.key() @ AargauError::InvalidPoolMint,
    )]
    pub vault_token_a: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = vault_token_b.owner == vault.key() @ AargauError::InvalidVaultPda,
        constraint = vault_token_b.mint == mint_b.key() @ AargauError::InvalidPoolMint,
    )]
    pub vault_token_b: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: treasury PDA — validated by seeds + bump.
    #[account(seeds = [TREASURY_SEED, protocol_config.key().as_ref()], bump)]
    pub treasury_pda: UncheckedAccount<'info>,

    #[account(
        mut,
        constraint = treasury_token_a.owner == treasury_pda.key() @ AargauError::InvalidTreasury,
        constraint = treasury_token_a.mint == mint_a.key() @ AargauError::InvalidPoolMint,
    )]
    pub treasury_token_a: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = treasury_token_b.owner == treasury_pda.key() @ AargauError::InvalidTreasury,
        constraint = treasury_token_b.mint == mint_b.key() @ AargauError::InvalidPoolMint,
    )]
    pub treasury_token_b: Box<InterfaceAccount<'info, TokenAccount>>,

    // ── Raydium CLMM accounts ────────────────────────────────────────────────
    /// CHECK: key equals `vault.pool_address` and owner is the CLMM program
    /// (constraints below); the handler re-checks the owner, the `PoolState`
    /// discriminator, and binds mints and pool vaults to it.
    #[account(
        mut,
        constraint = pool_state.key() == vault.pool_address @ AargauError::InvalidPool,
        constraint = pool_state.owner == &RAYDIUM_CLMM_PROGRAM_ID @ AargauError::InvalidPool,
    )]
    pub pool_state: UncheckedAccount<'info>,

    /// CHECK: `PersonalPositionState` — must equal `vault.position_address`,
    /// be owned by the CLMM program, carry the discriminator and be bound to
    /// `vault.position_mint` + `vault.pool_address` (handler). Closed by the
    /// CPI.
    #[account(mut)]
    pub personal_position: UncheckedAccount<'info>,

    /// CHECK: Token-2022 position NFT mint — must equal `vault.position_mint`
    /// (handler). Closed by the CPI.
    #[account(mut)]
    pub position_nft_mint: UncheckedAccount<'info>,

    /// CHECK: must equal the vault PDA's Token-2022 ATA for the NFT mint
    /// (`derive_position_nft_account`, handler). Closed by the CPI.
    #[account(mut)]
    pub position_nft_account: UncheckedAccount<'info>,

    /// CHECK: pool token vault 0 — bound to `pool_state.token_vault_0` in the
    /// handler; its outflow is the LP-fee measurement base.
    #[account(mut)]
    pub token_vault_0: UncheckedAccount<'info>,

    /// CHECK: pool token vault 1 — bound to `pool_state.token_vault_1`.
    #[account(mut)]
    pub token_vault_1: UncheckedAccount<'info>,

    /// CHECK: `TickArrayState` covering the OLD lower tick — bound to the PDA
    /// derived from `vault.position_range_lower` (handler).
    #[account(mut)]
    pub tick_array_lower: UncheckedAccount<'info>,

    /// CHECK: `TickArrayState` covering the OLD upper tick — bound like
    /// `tick_array_lower`.
    #[account(mut)]
    pub tick_array_upper: UncheckedAccount<'info>,

    /// CHECK: bound to the pool's derived bitmap-extension PDA in the handler
    /// (`require_tick_array_bitmap_extension`).
    #[account(mut)]
    pub tick_array_bitmap_extension: UncheckedAccount<'info>,

    /// CHECK: must equal the SPL Token program (fixed Raydium slot).
    #[account(constraint = token_program.key() == SPL_TOKEN_PROGRAM_ID @ AargauError::InvalidPool)]
    pub token_program: UncheckedAccount<'info>,

    /// CHECK: must equal the Token-2022 program (fixed Raydium slot; owns the
    /// position NFT mint + account).
    #[account(constraint = token_2022_program.key() == TOKEN_2022_PROGRAM_ID @ AargauError::InvalidPool)]
    pub token_2022_program: UncheckedAccount<'info>,

    /// CHECK: must equal `MEMO_PROGRAM_ID` (fixed `decrease_liquidity_v2` slot).
    #[account(constraint = memo_program.key() == MEMO_PROGRAM_ID @ AargauError::InvalidPool)]
    pub memo_program: UncheckedAccount<'info>,

    /// CHECK: must equal `RAYDIUM_CLMM_PROGRAM_ID`; every builder re-checks it.
    #[account(constraint = clmm_program.key() == RAYDIUM_CLMM_PROGRAM_ID @ AargauError::InvalidPool)]
    pub clmm_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler<'info>(
    ctx: Context<'info, StartRebalanceRaydium<'info>>,
    params: StartRebalanceRaydiumParams,
) -> Result<()> {
    let accounts = ctx.accounts;
    require_rebalance_startable(&accounts.vault)?;

    let pool = read_pool_state_view(&accounts.pool_state.to_account_info())?;
    require_pool_state_bindings(
        &pool,
        &accounts.mint_a.key(),
        &accounts.mint_b.key(),
        &accounts.token_vault_0.key(),
        &accounts.token_vault_1.key(),
    )?;
    require_tick_array_bitmap_extension(
        &accounts.pool_state.key(),
        &accounts.tick_array_bitmap_extension.key(),
    )?;
    require_openable_tick_range(
        params.new_tick_lower,
        params.new_tick_upper,
        pool.tick_spacing,
    )?;

    let position = require_active_position(accounts)?;
    require_full_drain_floor(position.liquidity, params.token_min_a, params.token_min_b)?;
    require_old_tick_arrays_bound(accounts, &pool)?;
    let rewards =
        bind_reward_transfer_accounts(&pool, &accounts.vault.key(), ctx.remaining_accounts)?;

    drain_and_close_position(
        accounts,
        &rewards,
        position.liquidity,
        params.token_min_a,
        params.token_min_b,
    )?;

    let initiated_at = Clock::get()?.unix_timestamp;
    record_pending_rebalance(
        &mut accounts.vault,
        params.new_tick_lower,
        params.new_tick_upper,
        initiated_at,
    );

    emit!(RebalancePending {
        vault: accounts.vault.key(),
        new_range_lower: params.new_tick_lower,
        new_range_upper: params.new_tick_upper,
        initiated_at,
        triggered_by: TriggeredBy::User,
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// State machine (pure; unit-tested in the integration crate)
// ---------------------------------------------------------------------------

/// Vault-state preconditions of the close leg: a Raydium vault with no
/// rebalance in flight. A second start while one is pending fails with
/// `PendingRebalanceExists`; the active position itself is bound later.
pub fn require_rebalance_startable(vault: &VaultAccount) -> Result<()> {
    require!(
        vault.protocol == Protocol::Raydium,
        AargauError::InvalidPool
    );
    require!(
        vault.pending_rebalance.is_none(),
        AargauError::PendingRebalanceExists
    );
    Ok(())
}

/// Close-leg transition: forget the closed position and persist the target
/// range, with a fresh retry counter.
pub fn record_pending_rebalance(
    vault: &mut VaultAccount,
    new_tick_lower: i32,
    new_tick_upper: i32,
    initiated_at: i64,
) {
    vault.position_address = None;
    vault.position_mint = None;
    vault.position_range_lower = None;
    vault.position_range_upper = None;
    vault.pending_rebalance = Some(PendingRebalance {
        new_tick_lower,
        new_tick_upper,
        initiated_at,
        retry_count: 0,
        last_failure_reason: 0,
    });
}

// ---------------------------------------------------------------------------
// Close leg
// ---------------------------------------------------------------------------

/// Collect with the fee split, drain the full liquidity, close the position
/// and sweep the refunded rent to the user.
fn drain_and_close_position<'info>(
    accounts: &mut StartRebalanceRaydium<'info>,
    rewards: &[RewardTransferAccounts<'info>],
    position_liquidity: u128,
    token_min_a: u64,
    token_min_b: u64,
) -> Result<()> {
    let seed_bytes = VaultSignerSeedBytes::new(&accounts.vault);
    let seeds = seed_bytes.seeds();
    let vault_position = position_accounts(accounts);
    let payout = PositionPayoutAccounts {
        memo_program: accounts.memo_program.to_account_info(),
        rewards,
    };

    // 1. Collect first: the drain would otherwise pay out the owed LP fees
    //    untaxed.
    collect_lp_fees_to_treasury(
        &vault_position,
        &payout,
        &treasury_accounts(accounts),
        &seeds,
    )?;

    // 2. Drain the whole position. An empty position has no principal to
    //    move and Raydium would only repeat the collect.
    if position_liquidity > 0 {
        decrease_principal_to_vault(
            &vault_position,
            &payout,
            position_liquidity,
            token_min_a,
            token_min_b,
            &seeds,
        )?;
    }

    // 3. Raydium closes only an empty position. A reward vault too poor to
    //    pay what is owed leaves `reward_amount_owed` behind; fail with a
    //    typed error instead of Raydium's `ClosePositionErr`.
    let drained = read_personal_position_view(&accounts.personal_position.to_account_info())?;
    require_raydium_position_empty(&drained)?;
    close_empty_position(
        &vault_position,
        &accounts.system_program.to_account_info(),
        &seeds,
    )?;

    // Raydium refunded the NFT mint, NFT account and position rent to the
    // vault PDA (`nft_owner`); funds may only leave to the user.
    sweep_excess_vault_lamports(
        &accounts.vault,
        &accounts.user.to_account_info(),
        &Rent::get()?,
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Account bindings
// ---------------------------------------------------------------------------

/// Bind the position accounts to what the vault recorded on open and return
/// the on-chain position, whose `liquidity` is the amount to drain.
fn require_active_position(accounts: &StartRebalanceRaydium<'_>) -> Result<PersonalPositionView> {
    let vault = &accounts.vault;
    let position_address = vault
        .position_address
        .ok_or(AargauError::VaultNoActivePosition)?;
    let nft_mint = vault
        .position_mint
        .ok_or(AargauError::VaultNoActivePosition)?;

    require_keys_eq!(
        accounts.personal_position.key(),
        position_address,
        AargauError::VaultNoActivePosition
    );
    require_keys_eq!(
        accounts.position_nft_mint.key(),
        nft_mint,
        AargauError::InvalidVaultPda
    );
    require_keys_eq!(
        accounts.position_nft_account.key(),
        derive_position_nft_account(&vault.key(), &nft_mint),
        AargauError::InvalidVaultPda
    );

    let position = read_personal_position_view(&accounts.personal_position.to_account_info())?;
    require_personal_position_bindings(&position, &nft_mint, &vault.pool_address)?;
    Ok(position)
}

/// Bind both tick arrays to the PDAs covering the stored (OLD) range.
fn require_old_tick_arrays_bound(
    accounts: &StartRebalanceRaydium<'_>,
    pool: &PoolStateView,
) -> Result<()> {
    let tick_lower = accounts
        .vault
        .position_range_lower
        .ok_or(AargauError::VaultNoActivePosition)?;
    let tick_upper = accounts
        .vault
        .position_range_upper
        .ok_or(AargauError::VaultNoActivePosition)?;
    let pool_key = accounts.pool_state.key();
    require_tick_array(
        &pool_key,
        tick_lower,
        pool.tick_spacing,
        &accounts.tick_array_lower.key(),
    )?;
    require_tick_array(
        &pool_key,
        tick_upper,
        pool.tick_spacing,
        &accounts.tick_array_upper.key(),
    )
}

// ---------------------------------------------------------------------------
// CPI assembly
// ---------------------------------------------------------------------------

/// The handler's accounts as the shared position bundle (leg 0 = token A).
fn position_accounts<'info>(
    accounts: &StartRebalanceRaydium<'info>,
) -> RaydiumPositionAccounts<'info> {
    RaydiumPositionAccounts {
        clmm_program: accounts.clmm_program.to_account_info(),
        vault: accounts.vault.to_account_info(),
        pool_state: accounts.pool_state.to_account_info(),
        personal_position: accounts.personal_position.to_account_info(),
        position_nft_mint: accounts.position_nft_mint.to_account_info(),
        position_nft_account: accounts.position_nft_account.to_account_info(),
        tick_array_lower: accounts.tick_array_lower.to_account_info(),
        tick_array_upper: accounts.tick_array_upper.to_account_info(),
        tick_array_bitmap_extension: accounts.tick_array_bitmap_extension.to_account_info(),
        token_program: accounts.token_program.to_account_info(),
        token_program_2022: accounts.token_2022_program.to_account_info(),
        leg_0: PairLegAccounts {
            mint: accounts.mint_a.to_account_info(),
            decimals: accounts.mint_a.decimals,
            vault_ata: accounts.vault_token_a.to_account_info(),
            pool_vault: accounts.token_vault_0.to_account_info(),
        },
        leg_1: PairLegAccounts {
            mint: accounts.mint_b.to_account_info(),
            decimals: accounts.mint_b.decimals,
            vault_ata: accounts.vault_token_b.to_account_info(),
            pool_vault: accounts.token_vault_1.to_account_info(),
        },
    }
}

fn treasury_accounts<'info>(accounts: &StartRebalanceRaydium<'info>) -> TreasuryFeeAccounts<'info> {
    TreasuryFeeAccounts {
        treasury_ata_0: accounts.treasury_token_a.to_account_info(),
        treasury_ata_1: accounts.treasury_token_b.to_account_info(),
        fee_rate_bps: accounts.protocol_config.fee_rate_bps,
    }
}
