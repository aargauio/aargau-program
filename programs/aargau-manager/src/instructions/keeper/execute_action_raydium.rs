//! `execute_action_raydium` — user-signed entry point for position-lifecycle
//! operations on a Raydium CLMM vault.
//!
//! Scope: the five position-management ops — `OpenPosition`,
//! `IncreaseLiquidity`, `DecreaseLiquidity`, `CollectFees` (fees + rewards),
//! `ClosePosition`. The vault PDA is `nft_owner` for every CPI after open;
//! performance fees on collected LP fees are routed to the protocol treasury
//! ATAs. The struct requires the vault owner (`user`) to sign, so
//! `triggered_by` is always `User`; `allowed_ops` is not consulted.
//!
//! ## Pause column: gated (entry)
//!
//! `protocol_config.is_paused` blocks every action, as in
//! `execute_action_orca`: `OpenPosition` and `IncreaseLiquidity` put vault
//! funds into a position, so the instruction is an entry path. The never-gated
//! exit paths are `withdraw`, `emergency_withdraw` and
//! `cancel_pending_rebalance`.
//!
//! ## How Raydium differs from Orca
//!
//! - **Open is empty.** `OpenPosition` mints the Token-2022 NFT into the vault
//!   PDA's ATA with zero liquidity; the user pays every rent (NFT mint, NFT
//!   account, `PersonalPositionState`, and any tick array Raydium creates).
//!   Funds move only through `IncreaseLiquidity`, signed by the vault.
//! - **Fees travel with every decrease.** Raydium has no separate collect:
//!   `decrease_liquidity_v2` pays all fees and rewards owed. `CollectFees` is a
//!   zero-liquidity decrease, and `DecreaseLiquidity` runs that collect step
//!   (with its fee split) before removing principal, so a decrease can never
//!   move LP fees into the vault without the performance fee.
//! - **Fee base = pool-vault outflow.** LP fees are measured as the outflow of
//!   the pool's `token_vault_0/1`, converted into the amount received using
//!   the mint's live transfer-fee config. Rewards leave from separate reward
//!   vaults, so they are never taxed, even when a reward mint equals a pair
//!   mint.
//! - **Rewards are mandatory, not best-effort.** Raydium requires one
//!   `[reward_vault, recipient, reward_mint]` triple per initialized reward
//!   slot on every decrease; each recipient is bound to the vault's own ATA.
//! - **Close refunds rent to the vault PDA.** The excess lamports are swept to
//!   the user in the same instruction.
//!
//! Token-2022 pair mints are accepted except `TransferHook` /
//! `NonTransferable`, which `OpenPosition` rejects up front.
//!
//! ## ⚠ VERIFICATION GATE ⚠
//!
//! The Raydium CLMM account orders, Borsh payloads and fee/reward payout
//! behaviour were taken from the program source, not from a replay. The
//! mollusk-svm replay of open → increase → collect → decrease → close against
//! the pinned mainnet `raydium_clmm.so` is the only gate that proves this
//! handler and the builders in `utils/raydium/` are wire-correct. **Do not
//! promote this instruction beyond local testing until that replay exists and
//! passes.** If the replay shows a mismatch, revisit this handler and the
//! builders together.

use crate::{
    constants::{
        MEMO_PROGRAM_ID, RAYDIUM_CLMM_PROGRAM_ID, SPL_TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID,
        TREASURY_SEED,
    },
    errors::AargauError,
    events::{FeesClaimed, LiquidityChanged, LiquidityOp, PositionClosed, PositionOpened},
    state::{Protocol, ProtocolConfig, RaydiumKeeperAction, TriggeredBy, VaultAccount},
    utils::{
        fee::calc_aargau_fee,
        raydium::{
            accounts::{
                derive_personal_position_pda, derive_position_nft_account, require_tick_array,
                require_tick_array_bitmap_extension,
            },
            close_position::{invoke_close_position, ClosePositionCpi},
            decrease_liquidity::{invoke_decrease_liquidity_v2, DecreaseLiquidityV2Cpi},
            increase_liquidity::{invoke_increase_liquidity_v2, IncreaseLiquidityV2Cpi},
            lp_fee::{measure_lp_fee_received, LpFeeLegBalances},
            open_position::{invoke_open_position_with_token22_nft, OpenPositionWithToken22NftCpi},
            personal_position_view::{
                read_personal_position_view, require_personal_position_bindings,
                PersonalPositionView,
            },
            pool_state_view::{read_pool_state_view, require_pool_state_bindings, PoolStateView},
            reward_accounts::{
                bind_reward_transfer_accounts, expected_reward_account_count,
                RewardTransferAccounts,
            },
        },
        signer_seeds::VaultSignerSeedBytes,
        token_2022::{
            is_token_2022, read_epoch_transfer_fee, read_transfer_fee_snapshot,
            require_v2_transferable_mint_account, TransferFeeSnapshot,
        },
        token_account::read_token_account_view,
        vault_ops::{sweep_excess_vault_lamports, transfer_performance_fee},
    },
};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount},
};

/// A zero-liquidity `decrease_liquidity_v2` only pays out fees and rewards
/// owed. Raydium skips its slippage check when `liquidity == 0` and no
/// principal moves, so zero minimums disable nothing here.
const COLLECT_ONLY_LIQUIDITY: u128 = 0;
const COLLECT_ONLY_MIN_AMOUNT: u64 = 0;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct ExecuteActionRaydiumParams {
    pub action: RaydiumKeeperAction,
}

/// User-signed instruction. `protocol_config` enforces the global pause
/// kill-switch and provides `fee_rate_bps` for the treasury split. Treasury
/// ATAs are required on every call to keep the IDL stable; only `CollectFees`
/// and `DecreaseLiquidity` write to them.
///
/// Raydium's `_v2` instructions take SPL Token and Token-2022 in fixed slots
/// and pick one per transfer; each Aargau fee leg routes its
/// `transfer_checked` through the program that owns that leg's mint.
///
/// `remaining_accounts`: exactly one `[reward_vault, recipient, reward_mint]`
/// triple per initialized pool reward slot for `CollectFees` and
/// `DecreaseLiquidity`; empty for every other action.
///
/// The deserialized accounts are boxed: with 26 accounts the unboxed struct
/// risks overflowing the SBF stack frame. Boxing changes neither the wire
/// format nor the IDL.
#[derive(Accounts)]
pub struct ExecuteActionRaydium<'info> {
    /// Vault owner — must equal `vault.user_authority`. Pays rent on open and
    /// receives the swept rent on close.
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

    /// CHECK: `PersonalPositionState`. On `OpenPosition` it must equal the PDA
    /// `[b"position", position_nft_mint]` (Raydium initialises it); on every
    /// other action it must equal `vault.position_address`, be owned by the
    /// CLMM program, carry the discriminator and be bound to
    /// `vault.position_mint` + `vault.pool_address` — all checked per action.
    #[account(mut)]
    pub personal_position: UncheckedAccount<'info>,

    /// CHECK: Token-2022 position NFT mint. On `OpenPosition` it must be a
    /// fresh system-owned signer; otherwise it must equal `vault.position_mint`.
    /// Checked per action.
    #[account(mut)]
    pub position_nft_mint: UncheckedAccount<'info>,

    /// CHECK: must equal the vault PDA's Token-2022 ATA for
    /// `position_nft_mint` (`derive_position_nft_account`), checked per action.
    #[account(mut)]
    pub position_nft_account: UncheckedAccount<'info>,

    /// CHECK: pool token vault 0 — bound to `pool_state.token_vault_0` in the
    /// handler; its balance is the LP-fee measurement base.
    #[account(mut)]
    pub token_vault_0: UncheckedAccount<'info>,

    /// CHECK: pool token vault 1 — bound to `pool_state.token_vault_1`.
    #[account(mut)]
    pub token_vault_1: UncheckedAccount<'info>,

    /// CHECK: `TickArrayState` covering the lower tick — bound per action to
    /// the PDA derived from the range (`require_tick_array`). Not forwarded by
    /// `ClosePosition`.
    #[account(mut)]
    pub tick_array_lower: UncheckedAccount<'info>,

    /// CHECK: `TickArrayState` covering the upper tick — bound like
    /// `tick_array_lower`.
    #[account(mut)]
    pub tick_array_upper: UncheckedAccount<'info>,

    /// CHECK: bound to the pool's derived bitmap-extension PDA in the handler
    /// (`require_tick_array_bitmap_extension`). Forwarded to open / increase /
    /// decrease, which ignore it unless a tick array leaves the default bitmap.
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

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handler<'info>(
    ctx: Context<'info, ExecuteActionRaydium<'info>>,
    params: ExecuteActionRaydiumParams,
) -> Result<()> {
    let accounts = ctx.accounts;
    let remaining_accounts = ctx.remaining_accounts;

    require!(
        accounts.vault.protocol == Protocol::Raydium,
        AargauError::InvalidPool
    );
    require!(
        accounts.vault.pending_rebalance.is_none(),
        AargauError::PendingRebalanceExists
    );

    // Bind mints and pool vaults to the pool, so no arbitrary token account
    // can stand in for a pool vault (it is the LP-fee measurement base).
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

    require!(
        remaining_accounts.len() == raydium_remaining_account_count(&params.action, &pool)?,
        AargauError::InvalidActionPayload
    );

    match params.action {
        RaydiumKeeperAction::OpenPosition {
            tick_lower,
            tick_upper,
        } => handle_open_position(accounts, &pool, tick_lower, tick_upper),
        RaydiumKeeperAction::IncreaseLiquidity {
            liquidity_amount,
            token_max_a,
            token_max_b,
        } => {
            require_active_position(accounts)?;
            require_tick_arrays_bound(accounts, &pool)?;
            handle_increase_liquidity(accounts, liquidity_amount, token_max_a, token_max_b)
        }
        RaydiumKeeperAction::DecreaseLiquidity {
            liquidity_amount,
            token_min_a,
            token_min_b,
        } => {
            let position = require_active_position(accounts)?;
            require_tick_arrays_bound(accounts, &pool)?;
            let rewards =
                bind_reward_transfer_accounts(&pool, &accounts.vault.key(), remaining_accounts)?;
            handle_decrease_liquidity(
                accounts,
                &position,
                &rewards,
                liquidity_amount,
                token_min_a,
                token_min_b,
            )
        }
        RaydiumKeeperAction::CollectFees => {
            require_active_position(accounts)?;
            require_tick_arrays_bound(accounts, &pool)?;
            let rewards =
                bind_reward_transfer_accounts(&pool, &accounts.vault.key(), remaining_accounts)?;
            handle_collect_fees(accounts, &rewards)
        }
        RaydiumKeeperAction::ClosePosition => {
            let position = require_active_position(accounts)?;
            handle_close_position(accounts, &position)
        }
    }
}

// ---------------------------------------------------------------------------
// Action handlers
// ---------------------------------------------------------------------------

fn handle_open_position<'info>(
    accounts: &mut ExecuteActionRaydium<'info>,
    pool: &PoolStateView,
    tick_lower: i32,
    tick_upper: i32,
) -> Result<()> {
    require!(
        accounts.vault.position_address.is_none(),
        AargauError::VaultHasActivePosition
    );
    require!(tick_upper > tick_lower, AargauError::InvalidActionPayload);

    let nft_mint = accounts.position_nft_mint.to_account_info();
    require!(
        is_fresh_position_nft_mint(nft_mint.is_signer, nft_mint.owner, nft_mint.data_len()),
        AargauError::InvalidActionPayload
    );
    require_keys_eq!(
        accounts.personal_position.key(),
        derive_personal_position_pda(nft_mint.key),
        AargauError::InvalidVaultPda
    );
    require_keys_eq!(
        accounts.position_nft_account.key(),
        derive_position_nft_account(&accounts.vault.key(), nft_mint.key),
        AargauError::InvalidVaultPda
    );
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
    )?;

    // A TransferHook or NonTransferable pair mint would let the position be
    // opened but never decreased or closed through the v2 path — funds locked.
    // Gate before any state is written.
    require_v2_transferable_mint_account(&accounts.mint_a.to_account_info())?;
    require_v2_transferable_mint_account(&accounts.mint_b.to_account_info())?;

    let cpi = OpenPositionWithToken22NftCpi {
        clmm_program: accounts.clmm_program.to_account_info(),
        payer: accounts.user.to_account_info(),
        vault: accounts.vault.to_account_info(),
        position_nft_mint: nft_mint.clone(),
        position_nft_account: accounts.position_nft_account.to_account_info(),
        pool_state: accounts.pool_state.to_account_info(),
        tick_array_lower: accounts.tick_array_lower.to_account_info(),
        tick_array_upper: accounts.tick_array_upper.to_account_info(),
        personal_position: accounts.personal_position.to_account_info(),
        token_account_0: accounts.vault_token_a.to_account_info(),
        token_account_1: accounts.vault_token_b.to_account_info(),
        token_vault_0: accounts.token_vault_0.to_account_info(),
        token_vault_1: accounts.token_vault_1.to_account_info(),
        rent: accounts.rent.to_account_info(),
        system_program: accounts.system_program.to_account_info(),
        token_program: accounts.token_program.to_account_info(),
        associated_token_program: accounts.associated_token_program.to_account_info(),
        token_program_2022: accounts.token_2022_program.to_account_info(),
        vault_0_mint: accounts.mint_a.to_account_info(),
        vault_1_mint: accounts.mint_b.to_account_info(),
        tick_array_bitmap_extension: accounts.tick_array_bitmap_extension.to_account_info(),
    };
    invoke_open_position_with_token22_nft(&cpi, tick_lower, tick_upper, pool.tick_spacing)?;

    snapshot_pair_transfer_fees(accounts)?;

    let position_address = accounts.personal_position.key();
    let vault = &mut accounts.vault;
    vault.position_address = Some(position_address);
    vault.position_mint = Some(*nft_mint.key);
    vault.position_range_lower = Some(tick_lower);
    vault.position_range_upper = Some(tick_upper);

    emit!(PositionOpened {
        vault: vault.key(),
        position_address,
        range_lower: tick_lower,
        range_upper: tick_upper,
        timestamp: Clock::get()?.unix_timestamp,
    });

    Ok(())
}

fn handle_increase_liquidity(
    accounts: &mut ExecuteActionRaydium<'_>,
    liquidity_amount: u128,
    token_max_a: u64,
    token_max_b: u64,
) -> Result<()> {
    let balance_before_a = accounts.vault_token_a.amount;
    let balance_before_b = accounts.vault_token_b.amount;
    require_raydium_increase_bounds(
        liquidity_amount,
        token_max_a,
        token_max_b,
        balance_before_a,
        balance_before_b,
    )?;

    let seed_bytes = VaultSignerSeedBytes::new(&accounts.vault);
    let seeds = seed_bytes.seeds();
    let cpi = increase_liquidity_cpi(accounts);
    invoke_increase_liquidity_v2(&cpi, liquidity_amount, token_max_a, token_max_b, &seeds)?;

    accounts.vault_token_a.reload()?;
    accounts.vault_token_b.reload()?;
    let consumed_a = observed_debit(balance_before_a, accounts.vault_token_a.amount)?;
    let consumed_b = observed_debit(balance_before_b, accounts.vault_token_b.amount)?;

    // Raydium enforces the maxima against the gross debit; re-check the
    // observed debit so the bound never depends on the CPI alone.
    require_consumed_within_max(consumed_a, token_max_a)?;
    require_consumed_within_max(consumed_b, token_max_b)?;

    emit!(LiquidityChanged {
        vault: accounts.vault.key(),
        position_address: accounts.personal_position.key(),
        op: LiquidityOp::Increase,
        amount_a: consumed_a,
        amount_b: consumed_b,
        timestamp: Clock::get()?.unix_timestamp,
        triggered_by: TriggeredBy::User,
    });

    Ok(())
}

fn handle_decrease_liquidity<'info>(
    accounts: &mut ExecuteActionRaydium<'info>,
    position: &PersonalPositionView,
    rewards: &[RewardTransferAccounts<'info>],
    liquidity_amount: u128,
    token_min_a: u64,
    token_min_b: u64,
) -> Result<()> {
    require_raydium_decrease_bounds(
        liquidity_amount,
        position.liquidity,
        token_min_a,
        token_min_b,
    )?;

    let seed_bytes = VaultSignerSeedBytes::new(&accounts.vault);
    let seeds = seed_bytes.seeds();

    // 1. Collect first: the principal decrease would otherwise pay out the
    //    owed LP fees untaxed. Within this instruction no swap can accrue new
    //    fees, so the second decrease carries principal only.
    collect_lp_fees_and_route_to_treasury(accounts, rewards, &seeds)?;

    // 2. Remove principal. Re-read the ATAs: the fee split just moved tokens.
    accounts.vault_token_a.reload()?;
    accounts.vault_token_b.reload()?;
    let balance_before_a = accounts.vault_token_a.amount;
    let balance_before_b = accounts.vault_token_b.amount;

    let cpi = decrease_liquidity_cpi(accounts, rewards);
    invoke_decrease_liquidity_v2(&cpi, liquidity_amount, token_min_a, token_min_b, &seeds)?;

    accounts.vault_token_a.reload()?;
    accounts.vault_token_b.reload()?;
    let received_a = observed_credit(balance_before_a, accounts.vault_token_a.amount)?;
    let received_b = observed_credit(balance_before_b, accounts.vault_token_b.amount)?;

    // Raydium checks the floors against principal net of transfer fee; the
    // observed ATA credit is the same quantity. Re-check as defence-in-depth.
    require_principal_meets_min(received_a, token_min_a)?;
    require_principal_meets_min(received_b, token_min_b)?;

    emit!(LiquidityChanged {
        vault: accounts.vault.key(),
        position_address: accounts.personal_position.key(),
        op: LiquidityOp::Decrease,
        amount_a: received_a,
        amount_b: received_b,
        timestamp: Clock::get()?.unix_timestamp,
        triggered_by: TriggeredBy::User,
    });

    Ok(())
}

fn handle_collect_fees<'info>(
    accounts: &mut ExecuteActionRaydium<'info>,
    rewards: &[RewardTransferAccounts<'info>],
) -> Result<()> {
    let seed_bytes = VaultSignerSeedBytes::new(&accounts.vault);
    let seeds = seed_bytes.seeds();
    collect_lp_fees_and_route_to_treasury(accounts, rewards, &seeds)
}

fn handle_close_position(
    accounts: &mut ExecuteActionRaydium<'_>,
    position: &PersonalPositionView,
) -> Result<()> {
    require_raydium_position_empty(position)?;

    let seed_bytes = VaultSignerSeedBytes::new(&accounts.vault);
    let seeds = seed_bytes.seeds();
    let cpi = ClosePositionCpi {
        clmm_program: accounts.clmm_program.to_account_info(),
        vault: accounts.vault.to_account_info(),
        position_nft_mint: accounts.position_nft_mint.to_account_info(),
        position_nft_account: accounts.position_nft_account.to_account_info(),
        personal_position: accounts.personal_position.to_account_info(),
        system_program: accounts.system_program.to_account_info(),
        token_program_2022: accounts.token_2022_program.to_account_info(),
        pool_state: accounts.pool_state.to_account_info(),
    };
    invoke_close_position(&cpi, &seeds)?;

    let position_address = accounts.personal_position.key();
    let vault = &mut accounts.vault;
    vault.position_address = None;
    vault.position_mint = None;
    vault.position_range_lower = None;
    vault.position_range_upper = None;

    // Raydium refunded the NFT mint, NFT account and position rent to the
    // vault PDA (`nft_owner`); funds may only leave to the user.
    sweep_excess_vault_lamports(
        &accounts.vault,
        &accounts.user.to_account_info(),
        &accounts.rent,
    )?;

    emit!(PositionClosed {
        vault: accounts.vault.key(),
        position_address,
        timestamp: Clock::get()?.unix_timestamp,
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// Fee collection
// ---------------------------------------------------------------------------

/// Zero-liquidity decrease (pays every fee and reward owed into the vault
/// ATAs), then the performance-fee split on the LP-fee portion only.
fn collect_lp_fees_and_route_to_treasury<'info>(
    accounts: &mut ExecuteActionRaydium<'info>,
    rewards: &[RewardTransferAccounts<'info>],
    vault_signer_seeds: &[&[u8]],
) -> Result<()> {
    let pool_vault_before_0 = read_pool_vault_amount(&accounts.token_vault_0)?;
    let pool_vault_before_1 = read_pool_vault_amount(&accounts.token_vault_1)?;
    let vault_ata_before_a = accounts.vault_token_a.amount;
    let vault_ata_before_b = accounts.vault_token_b.amount;

    let cpi = decrease_liquidity_cpi(accounts, rewards);
    invoke_decrease_liquidity_v2(
        &cpi,
        COLLECT_ONLY_LIQUIDITY,
        COLLECT_ONLY_MIN_AMOUNT,
        COLLECT_ONLY_MIN_AMOUNT,
        vault_signer_seeds,
    )?;

    accounts.vault_token_a.reload()?;
    accounts.vault_token_b.reload()?;
    let epoch = Clock::get()?.epoch;
    let lp_fee_a = measure_lp_fee_received(
        &LpFeeLegBalances {
            pool_vault_before: pool_vault_before_0,
            pool_vault_after: read_pool_vault_amount(&accounts.token_vault_0)?,
            vault_ata_before: vault_ata_before_a,
            vault_ata_after: accounts.vault_token_a.amount,
        },
        read_live_transfer_fee(&accounts.mint_a.to_account_info(), epoch)?,
    )?;
    let lp_fee_b = measure_lp_fee_received(
        &LpFeeLegBalances {
            pool_vault_before: pool_vault_before_1,
            pool_vault_after: read_pool_vault_amount(&accounts.token_vault_1)?,
            vault_ata_before: vault_ata_before_b,
            vault_ata_after: accounts.vault_token_b.amount,
        },
        read_live_transfer_fee(&accounts.mint_b.to_account_info(), epoch)?,
    )?;

    route_performance_fees(accounts, lp_fee_a, lp_fee_b, vault_signer_seeds)
}

/// Transfer the Aargau share of each LP-fee leg to the treasury ATAs and emit
/// `FeesClaimed`. Each leg uses the token program that owns its mint.
fn route_performance_fees(
    accounts: &ExecuteActionRaydium<'_>,
    lp_fee_a: u64,
    lp_fee_b: u64,
    vault_signer_seeds: &[&[u8]],
) -> Result<()> {
    let fee_rate_bps = accounts.protocol_config.fee_rate_bps;
    let split_a = split_lp_fee(lp_fee_a, fee_rate_bps)?;
    let split_b = split_lp_fee(lp_fee_b, fee_rate_bps)?;
    let signer: &[&[&[u8]]] = &[vault_signer_seeds];

    let mint_a = accounts.mint_a.to_account_info();
    transfer_performance_fee(
        *mint_a.owner,
        accounts.vault_token_a.to_account_info(),
        mint_a.clone(),
        accounts.treasury_token_a.to_account_info(),
        accounts.vault.to_account_info(),
        accounts.mint_a.decimals,
        split_a.aargau_fee,
        signer,
    )?;
    let mint_b = accounts.mint_b.to_account_info();
    transfer_performance_fee(
        *mint_b.owner,
        accounts.vault_token_b.to_account_info(),
        mint_b.clone(),
        accounts.treasury_token_b.to_account_info(),
        accounts.vault.to_account_info(),
        accounts.mint_b.decimals,
        split_b.aargau_fee,
        signer,
    )?;

    emit!(FeesClaimed {
        vault: accounts.vault.key(),
        gross_a: split_a.gross,
        gross_b: split_b.gross,
        aargau_fee_a: split_a.aargau_fee,
        aargau_fee_b: split_b.aargau_fee,
        user_net_a: split_a.user_net,
        user_net_b: split_b.user_net,
        timestamp: Clock::get()?.unix_timestamp,
    });

    Ok(())
}

fn read_pool_vault_amount(pool_vault: &UncheckedAccount<'_>) -> Result<u64> {
    Ok(read_token_account_view(&pool_vault.to_account_info())?.amount)
}

fn read_live_transfer_fee(
    mint: &AccountInfo<'_>,
    epoch: u64,
) -> Result<Option<TransferFeeSnapshot>> {
    let data = mint.try_borrow_data()?;
    Ok(read_epoch_transfer_fee(&data, epoch))
}

// ---------------------------------------------------------------------------
// Account bindings
// ---------------------------------------------------------------------------

/// Bind the position accounts to what the vault recorded on open and return
/// the on-chain position. Key equality alone is not enough: the view check
/// proves the account is a live CLMM `PersonalPositionState` for this NFT
/// mint and pool.
fn require_active_position(accounts: &ExecuteActionRaydium<'_>) -> Result<PersonalPositionView> {
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

/// Bind both tick arrays to the PDAs covering the stored position range.
fn require_tick_arrays_bound(
    accounts: &ExecuteActionRaydium<'_>,
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

fn increase_liquidity_cpi<'info>(
    accounts: &ExecuteActionRaydium<'info>,
) -> IncreaseLiquidityV2Cpi<'info> {
    IncreaseLiquidityV2Cpi {
        clmm_program: accounts.clmm_program.to_account_info(),
        vault: accounts.vault.to_account_info(),
        nft_account: accounts.position_nft_account.to_account_info(),
        pool_state: accounts.pool_state.to_account_info(),
        personal_position: accounts.personal_position.to_account_info(),
        tick_array_lower: accounts.tick_array_lower.to_account_info(),
        tick_array_upper: accounts.tick_array_upper.to_account_info(),
        vault_token_0: accounts.vault_token_a.to_account_info(),
        vault_token_1: accounts.vault_token_b.to_account_info(),
        token_vault_0: accounts.token_vault_0.to_account_info(),
        token_vault_1: accounts.token_vault_1.to_account_info(),
        token_program: accounts.token_program.to_account_info(),
        token_program_2022: accounts.token_2022_program.to_account_info(),
        vault_0_mint: accounts.mint_a.to_account_info(),
        vault_1_mint: accounts.mint_b.to_account_info(),
        tick_array_bitmap_extension: accounts.tick_array_bitmap_extension.to_account_info(),
    }
}

fn decrease_liquidity_cpi<'a, 'info>(
    accounts: &ExecuteActionRaydium<'info>,
    rewards: &'a [RewardTransferAccounts<'info>],
) -> DecreaseLiquidityV2Cpi<'a, 'info> {
    DecreaseLiquidityV2Cpi {
        clmm_program: accounts.clmm_program.to_account_info(),
        vault: accounts.vault.to_account_info(),
        nft_account: accounts.position_nft_account.to_account_info(),
        personal_position: accounts.personal_position.to_account_info(),
        pool_state: accounts.pool_state.to_account_info(),
        token_vault_0: accounts.token_vault_0.to_account_info(),
        token_vault_1: accounts.token_vault_1.to_account_info(),
        tick_array_lower: accounts.tick_array_lower.to_account_info(),
        tick_array_upper: accounts.tick_array_upper.to_account_info(),
        vault_token_0: accounts.vault_token_a.to_account_info(),
        vault_token_1: accounts.vault_token_b.to_account_info(),
        token_program: accounts.token_program.to_account_info(),
        token_program_2022: accounts.token_2022_program.to_account_info(),
        memo_program: accounts.memo_program.to_account_info(),
        vault_0_mint: accounts.mint_a.to_account_info(),
        vault_1_mint: accounts.mint_b.to_account_info(),
        tick_array_bitmap_extension: accounts.tick_array_bitmap_extension.to_account_info(),
        reward_accounts: rewards,
    }
}

/// Record the pair mints' Token-2022 transfer-fee config on the vault, as
/// the Orca open does.
fn snapshot_pair_transfer_fees(accounts: &mut ExecuteActionRaydium<'_>) -> Result<()> {
    let mint_a = accounts.mint_a.to_account_info();
    let mint_b = accounts.mint_b.to_account_info();
    let fee_a = read_transfer_fee_snapshot(&mint_a)?;
    let fee_b = read_transfer_fee_snapshot(&mint_b)?;

    let vault = &mut accounts.vault;
    vault.uses_token_2022 = is_token_2022(mint_a.owner) || is_token_2022(mint_b.owner);
    vault.token_a_transfer_fee_bps = fee_a.transfer_fee_bps;
    vault.token_a_maximum_fee = fee_a.maximum_fee;
    vault.token_b_transfer_fee_bps = fee_b.transfer_fee_bps;
    vault.token_b_maximum_fee = fee_b.maximum_fee;
    Ok(())
}

// ---------------------------------------------------------------------------
// Pure guards and arithmetic (unit-tested in the integration crate)
// ---------------------------------------------------------------------------

/// Number of `remaining_accounts` each action must carry: one reward triple
/// per initialized pool reward slot for the two decrease-based actions, none
/// otherwise.
pub fn raydium_remaining_account_count(
    action: &RaydiumKeeperAction,
    pool: &PoolStateView,
) -> Result<usize> {
    match action {
        RaydiumKeeperAction::CollectFees | RaydiumKeeperAction::DecreaseLiquidity { .. } => {
            expected_reward_account_count(pool)
        }
        RaydiumKeeperAction::OpenPosition { .. }
        | RaydiumKeeperAction::IncreaseLiquidity { .. }
        | RaydiumKeeperAction::ClosePosition => Ok(0),
    }
}

/// The open's NFT mint must be a fresh keypair that signed the transaction:
/// Raydium creates the mint account itself.
pub fn is_fresh_position_nft_mint(is_signer: bool, owner: &Pubkey, data_len: usize) -> bool {
    is_signer && *owner == anchor_lang::system_program::ID && data_len == 0
}

/// `IncreaseLiquidity` payload bounds: non-zero liquidity, at least one
/// non-zero maximum, and neither maximum above the vault's balance.
pub fn require_raydium_increase_bounds(
    liquidity_amount: u128,
    token_max_a: u64,
    token_max_b: u64,
    vault_balance_a: u64,
    vault_balance_b: u64,
) -> Result<()> {
    require!(liquidity_amount > 0, AargauError::InvalidActionPayload);
    require!(
        token_max_a > 0 || token_max_b > 0,
        AargauError::InvalidActionPayload
    );
    require!(
        token_max_a <= vault_balance_a,
        AargauError::InsufficientFunds
    );
    require!(
        token_max_b <= vault_balance_b,
        AargauError::InsufficientFunds
    );
    Ok(())
}

/// `DecreaseLiquidity` payload bounds: non-zero liquidity no larger than the
/// on-chain position liquidity, and a slippage floor on at least one side
/// (only `emergency_withdraw` may exit with 0/0).
pub fn require_raydium_decrease_bounds(
    liquidity_amount: u128,
    position_liquidity: u128,
    token_min_a: u64,
    token_min_b: u64,
) -> Result<()> {
    require!(liquidity_amount > 0, AargauError::InvalidActionPayload);
    require!(
        liquidity_amount <= position_liquidity,
        AargauError::InvalidActionPayload
    );
    require!(
        token_min_a > 0 || token_min_b > 0,
        AargauError::SlippageExceeded
    );
    Ok(())
}

/// Raydium only closes an empty position; checking the same fields up front
/// gives a clear error instead of Raydium's `ClosePositionErr`.
pub fn require_raydium_position_empty(position: &PersonalPositionView) -> Result<()> {
    let is_empty = position.liquidity == 0
        && position.token_fees_owed_0 == 0
        && position.token_fees_owed_1 == 0
        && position.reward_amounts_owed.iter().all(|owed| *owed == 0);
    require!(is_empty, AargauError::PositionNotEmpty);
    Ok(())
}

/// Tokens that left a vault ATA across a CPI. A balance that grew during a
/// deposit-style CPI is an accounting inconsistency (`Underflow`).
pub fn observed_debit(balance_before: u64, balance_after: u64) -> Result<u64> {
    balance_before
        .checked_sub(balance_after)
        .ok_or(error!(AargauError::Underflow))
}

/// Tokens that arrived in a vault ATA across a CPI (`PostCpiBalanceDecreased`
/// if it shrank).
pub fn observed_credit(balance_before: u64, balance_after: u64) -> Result<u64> {
    balance_after
        .checked_sub(balance_before)
        .ok_or(error!(AargauError::PostCpiBalanceDecreased))
}

/// Max-in check on the observed debit.
pub fn require_consumed_within_max(consumed: u64, token_max: u64) -> Result<()> {
    require!(consumed <= token_max, AargauError::SlippageExceeded);
    Ok(())
}

/// Min-out check on the observed principal credit.
pub fn require_principal_meets_min(received: u64, token_min: u64) -> Result<()> {
    require!(received >= token_min, AargauError::SlippageExceeded);
    Ok(())
}

/// One LP-fee leg split between the treasury and the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LpFeeSplit {
    pub gross: u64,
    pub aargau_fee: u64,
    pub user_net: u64,
}

/// Split a received LP fee with `calc_aargau_fee` (rounds down, user favored).
pub fn split_lp_fee(gross: u64, fee_rate_bps: u16) -> Result<LpFeeSplit> {
    let aargau_fee = calc_aargau_fee(gross, fee_rate_bps)?;
    let user_net = gross
        .checked_sub(aargau_fee)
        .ok_or(error!(AargauError::FeeExceedsGross))?;
    Ok(LpFeeSplit {
        gross,
        aargau_fee,
        user_net,
    })
}
