//! `retry_pending_rebalance_raydium` — phase two of a two-transaction Raydium
//! CLMM rebalance (OpenNew). User-signed; the vault PDA owns the new position
//! NFT.
//!
//! Completes a rebalance started by `start_rebalance_raydium`: opens an empty
//! position over the range stored in `vault.pending_rebalance` (user pays the
//! rent), funds it with `increase_liquidity_v2` signed by the vault, then
//! clears the pending state. The range comes ONLY from `pending_rebalance`;
//! the caller supplies amounts, so a retry can never retarget the rebalance.
//!
//! The same instruction serves the first attempt and every retry. Open and
//! increase run in one transaction, so a failed attempt persists nothing and
//! a retry is safe. **Each attempt must use a freshly generated
//! `position_nft_mint` keypair** — the position PDA derives from it. After a
//! success `pending_rebalance` is `None`, so running it again fails with
//! `NoPendingRebalance`.
//!
//! No fees are collected here (the close leg already did), so no treasury
//! accounts are taken — parity with the Orca `retry_pending_rebalance`.
//!
//! ## Pause column: gated (entry)
//!
//! `protocol_config.is_paused` blocks this instruction, as it blocks
//! `retry_pending_rebalance`: it moves vault funds into a new position. While
//! paused a pending vault is not stuck — `cancel_pending_rebalance`,
//! `withdraw` and `emergency_withdraw` are never gated.
//!
//! ## ⚠ VERIFICATION GATE ⚠
//!
//! The Raydium CLMM account orders and Borsh payloads were taken from the
//! program source, not from a replay. The mollusk-svm replay of start → retry
//! and start → failed retry → retry against the pinned mainnet
//! `raydium_clmm.so` is the only gate that proves this handler wire-correct.
//! **Do not promote this instruction beyond local testing until that replay
//! exists and passes.**

use crate::{
    constants::{RAYDIUM_CLMM_PROGRAM_ID, SPL_TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID},
    errors::AargauError,
    events::RebalanceExecuted,
    state::{PendingRebalance, Protocol, ProtocolConfig, TriggeredBy, VaultAccount},
    utils::{
        raydium::{
            accounts::{
                derive_personal_position_pda, derive_position_nft_account, require_tick_array,
                require_tick_array_bitmap_extension,
            },
            pool_state_view::{read_pool_state_view, require_pool_state_bindings, PoolStateView},
            position_guards::{
                is_fresh_position_nft_mint, require_no_remaining_accounts,
                require_openable_tick_range,
            },
            vault_position::{
                increase_liquidity_from_vault, open_empty_position, OpenPositionFundingAccounts,
                PairLegAccounts, RaydiumPositionAccounts,
            },
        },
        signer_seeds::VaultSignerSeedBytes,
        token_2022::require_v2_transferable_mint_account,
        vault_ops::record_pair_transfer_fees,
    },
};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount},
};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct RetryPendingRebalanceRaydiumParams {
    /// Liquidity to add to the new position, computed off-chain against the
    /// current pool price. Only the range is fixed by `pending_rebalance`.
    pub liquidity_amount: u128,
    /// Slippage ceiling for token A taken from the vault by the increase.
    pub token_max_a: u64,
    /// Slippage ceiling for token B taken from the vault by the increase.
    pub token_max_b: u64,
}

/// Open-side Raydium account set: the open / increase subset of
/// `ExecuteActionRaydium`, with the tick arrays of the PENDING range and no
/// treasury accounts.
///
/// `remaining_accounts`: must be empty.
///
/// The deserialized accounts are boxed to keep the SBF stack frame small.
/// Boxing changes neither the wire format nor the IDL.
#[derive(Accounts)]
pub struct RetryPendingRebalanceRaydium<'info> {
    /// Vault owner — must equal `vault.user_authority`. Pays the rent of every
    /// account the open creates.
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

    /// CHECK: the new `PersonalPositionState` — must equal the PDA
    /// `[b"position", position_nft_mint]` (handler); Raydium initialises it.
    #[account(mut)]
    pub personal_position: UncheckedAccount<'info>,

    /// Fresh Token-2022 position NFT mint keypair; must also be system-owned
    /// and empty (handler). Each attempt needs a new keypair.
    #[account(mut)]
    pub position_nft_mint: Signer<'info>,

    /// CHECK: must equal the vault PDA's Token-2022 ATA for
    /// `position_nft_mint` (`derive_position_nft_account`, handler); Raydium
    /// creates it.
    #[account(mut)]
    pub position_nft_account: UncheckedAccount<'info>,

    /// CHECK: pool token vault 0 — bound to `pool_state.token_vault_0` in the
    /// handler.
    #[account(mut)]
    pub token_vault_0: UncheckedAccount<'info>,

    /// CHECK: pool token vault 1 — bound to `pool_state.token_vault_1`.
    #[account(mut)]
    pub token_vault_1: UncheckedAccount<'info>,

    /// CHECK: `TickArrayState` covering the PENDING lower tick — bound to the
    /// PDA derived from `pending_rebalance.new_tick_lower` (handler). Raydium
    /// creates it (user pays) when it does not exist yet.
    #[account(mut)]
    pub tick_array_lower: UncheckedAccount<'info>,

    /// CHECK: `TickArrayState` covering the PENDING upper tick — bound like
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

    /// CHECK: must equal `RAYDIUM_CLMM_PROGRAM_ID`; every builder re-checks it.
    #[account(constraint = clmm_program.key() == RAYDIUM_CLMM_PROGRAM_ID @ AargauError::InvalidPool)]
    pub clmm_program: UncheckedAccount<'info>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handler<'info>(
    ctx: Context<'info, RetryPendingRebalanceRaydium<'info>>,
    params: RetryPendingRebalanceRaydiumParams,
) -> Result<()> {
    let accounts = ctx.accounts;
    let pending = require_rebalance_retryable(&accounts.vault)?;
    require_no_remaining_accounts(ctx.remaining_accounts.len())?;

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
    // `start_rebalance_raydium` already checked the range; re-check so the
    // open never depends on how the pending state was written.
    let tick_lower = pending.new_tick_lower;
    let tick_upper = pending.new_tick_upper;
    require_openable_tick_range(tick_lower, tick_upper, pool.tick_spacing)?;

    require_new_position_accounts_bound(accounts)?;
    require_pending_tick_arrays_bound(accounts, &pool, tick_lower, tick_upper)?;

    // A TransferHook or NonTransferable pair mint would let the position be
    // opened but never decreased or closed through the v2 path — funds
    // locked. Gate before any CPI.
    require_v2_transferable_mint_account(&accounts.mint_a.to_account_info())?;
    require_v2_transferable_mint_account(&accounts.mint_b.to_account_info())?;

    let vault_position = position_accounts(accounts);
    let funding = OpenPositionFundingAccounts {
        payer: accounts.user.to_account_info(),
        rent: accounts.rent.to_account_info(),
        system_program: accounts.system_program.to_account_info(),
        associated_token_program: accounts.associated_token_program.to_account_info(),
    };
    open_empty_position(
        &vault_position,
        &funding,
        tick_lower,
        tick_upper,
        pool.tick_spacing,
    )?;

    let seed_bytes = VaultSignerSeedBytes::new(&accounts.vault);
    let seeds = seed_bytes.seeds();
    increase_liquidity_from_vault(
        &vault_position,
        params.liquidity_amount,
        params.token_max_a,
        params.token_max_b,
        &seeds,
    )?;
    accounts.vault_token_a.reload()?;
    accounts.vault_token_b.reload()?;

    record_pair_transfer_fees(
        &mut accounts.vault,
        &accounts.mint_a.to_account_info(),
        &accounts.mint_b.to_account_info(),
    )?;

    let position_address = accounts.personal_position.key();
    let position_mint = accounts.position_nft_mint.key();
    record_rebalanced_position(
        &mut accounts.vault,
        position_address,
        position_mint,
        tick_lower,
        tick_upper,
    );

    // `RebalanceExecuted` has an old and a new range, but the close leg is a
    // separate transaction and cleared the old range; nothing on the vault
    // records it any more. Same as the Orca retry: emit old == new and let
    // consumers take the old range from their index of the start leg.
    emit!(RebalanceExecuted {
        vault: accounts.vault.key(),
        protocol: Protocol::Raydium,
        old_range_lower: tick_lower,
        old_range_upper: tick_upper,
        new_range_lower: tick_lower,
        new_range_upper: tick_upper,
        triggered_by: TriggeredBy::User,
        timestamp: Clock::get()?.unix_timestamp,
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// State machine (pure; unit-tested in the integration crate)
// ---------------------------------------------------------------------------

/// Vault-state preconditions of the open leg: a Raydium vault with a pending
/// rebalance and no active position. Returns the pending state, the only
/// source of the range. Running again after a success fails with
/// `NoPendingRebalance`.
pub fn require_rebalance_retryable(vault: &VaultAccount) -> Result<PendingRebalance> {
    require!(
        vault.protocol == Protocol::Raydium,
        AargauError::InvalidPool
    );
    let pending = vault
        .pending_rebalance
        .ok_or(AargauError::NoPendingRebalance)?;
    require!(
        vault.position_address.is_none(),
        AargauError::VaultHasActivePosition
    );
    Ok(pending)
}

/// Open-leg transition: record the new position over the pending range and
/// clear the pending state.
pub fn record_rebalanced_position(
    vault: &mut VaultAccount,
    position_address: Pubkey,
    position_mint: Pubkey,
    tick_lower: i32,
    tick_upper: i32,
) {
    vault.position_address = Some(position_address);
    vault.position_mint = Some(position_mint);
    vault.position_range_lower = Some(tick_lower);
    vault.position_range_upper = Some(tick_upper);
    vault.pending_rebalance = None;
}

// ---------------------------------------------------------------------------
// Account bindings
// ---------------------------------------------------------------------------

/// The NFT mint must be a fresh keypair, and the position and NFT account
/// must be the addresses Raydium derives from it for the vault.
fn require_new_position_accounts_bound(accounts: &RetryPendingRebalanceRaydium<'_>) -> Result<()> {
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
    Ok(())
}

/// Bind both tick arrays to the PDAs covering the pending (NEW) range.
fn require_pending_tick_arrays_bound(
    accounts: &RetryPendingRebalanceRaydium<'_>,
    pool: &PoolStateView,
    tick_lower: i32,
    tick_upper: i32,
) -> Result<()> {
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
    accounts: &RetryPendingRebalanceRaydium<'info>,
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
