//! # Aargau Manager Program
//!
//! Custodial vault program for LP position management on Orca Whirlpools,
//! Raydium CLMM, and Meteora DLMM.
//!
//! The `VaultAccount` PDA is the direct owner of all LP positions —
//! compatible with burn-and-reissue NFT mechanics (Orca/Raydium) and
//! owner-field positions (Meteora).
//!
//! ## Implemented instructions (with real logic):
//! - `initialize_protocol` — create ProtocolConfig singleton
//! - `create_vault` — create VaultAccount PDA + ATAs (4-layer pool validation)
//! - `deposit` — transfer tokens into vault ATAs (idle only)
//! - `withdraw` — proportional token withdrawal from idle vault
//! - `emergency_withdraw` — unconditional full exit, bypasses is_paused
//! - `set_protocol_pause` — admin kill switch
//!
//! ## Stubs (correct signatures, reserved for future implementation):
//! - `close_vault`, `claim_rewards`, `manual_rebalance`,
//!   `cancel_pending_rebalance`, `retry_pending_rebalance`,
//!   `update_protocol_config`, `withdraw_treasury`, `transfer_admin`,
//!   `admin_emergency_transfer`, `execute_action`, `report_rebalance_attempt`

use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod state;
pub mod utils;

use instructions::*;

declare_id!("AargMgr1111111111111111111111111111111111111");

#[program]
pub mod aargau_manager {
    use super::*;

    // -------------------------------------------------------------------------
    // Setup
    // -------------------------------------------------------------------------

    pub fn initialize_protocol(
        ctx: Context<InitializeProtocol>,
        params: InitializeProtocolParams,
    ) -> Result<()> {
        initialize_protocol::handler(ctx, params)
    }

    // -------------------------------------------------------------------------
    // Vault lifecycle
    // -------------------------------------------------------------------------

    pub fn create_vault(ctx: Context<CreateVault>, params: CreateVaultParams) -> Result<()> {
        create_vault::handler(ctx, params)
    }

    pub fn close_vault(ctx: Context<CloseVault>, params: CloseVaultParams) -> Result<()> {
        close_vault::handler(ctx, params)
    }

    // -------------------------------------------------------------------------
    // Deposit / Withdraw
    // -------------------------------------------------------------------------

    pub fn deposit(ctx: Context<Deposit>, params: DepositParams) -> Result<()> {
        deposit::handler(ctx, params)
    }

    pub fn withdraw(ctx: Context<Withdraw>, params: WithdrawParams) -> Result<()> {
        withdraw::handler(ctx, params)
    }

    pub fn emergency_withdraw(ctx: Context<EmergencyWithdraw>) -> Result<()> {
        emergency_withdraw::handler(ctx)
    }

    // -------------------------------------------------------------------------
    // Rewards
    // -------------------------------------------------------------------------

    pub fn claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
        claim_rewards::handler(ctx)
    }

    // -------------------------------------------------------------------------
    // Rebalance
    // -------------------------------------------------------------------------

    pub fn manual_rebalance(
        ctx: Context<ManualRebalance>,
        params: ManualRebalanceParams,
    ) -> Result<()> {
        manual_rebalance::handler(ctx, params)
    }

    pub fn cancel_pending_rebalance(ctx: Context<CancelPendingRebalance>) -> Result<()> {
        cancel_pending_rebalance::handler(ctx)
    }

    pub fn retry_pending_rebalance(
        ctx: Context<RetryPendingRebalance>,
        params: RetryPendingRebalanceParams,
    ) -> Result<()> {
        retry_pending_rebalance::handler(ctx, params)
    }

    // -------------------------------------------------------------------------
    // Admin
    // -------------------------------------------------------------------------

    pub fn set_protocol_pause(
        ctx: Context<SetProtocolPause>,
        params: SetProtocolPauseParams,
    ) -> Result<()> {
        set_protocol_pause::handler(ctx, params)
    }

    pub fn update_protocol_config(
        ctx: Context<UpdateProtocolConfig>,
        params: UpdateProtocolConfigParams,
    ) -> Result<()> {
        update_protocol_config::handler(ctx, params)
    }

    pub fn withdraw_treasury(
        ctx: Context<WithdrawTreasury>,
        params: WithdrawTreasuryParams,
    ) -> Result<()> {
        withdraw_treasury::handler(ctx, params)
    }

    pub fn transfer_admin(ctx: Context<TransferAdmin>, params: TransferAdminParams) -> Result<()> {
        transfer_admin::handler(ctx, params)
    }

    pub fn admin_emergency_transfer(ctx: Context<AdminEmergencyTransfer>) -> Result<()> {
        admin_emergency_transfer::handler(ctx)
    }

    // -------------------------------------------------------------------------
    // Keeper co-signed instructions (stubs — reserved for future use)
    // -------------------------------------------------------------------------

    pub fn execute_action(ctx: Context<ExecuteAction>, params: ExecuteActionParams) -> Result<()> {
        execute_action::handler(ctx, params)
    }

    pub fn report_rebalance_attempt(
        ctx: Context<ReportRebalanceAttempt>,
        params: ReportRebalanceAttemptParams,
    ) -> Result<()> {
        report_rebalance_attempt::handler(ctx, params)
    }
}
