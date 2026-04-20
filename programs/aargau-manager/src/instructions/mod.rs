pub mod admin_emergency_transfer;
pub mod cancel_pending_rebalance;
pub mod claim_rewards;
pub mod close_vault;
pub mod create_vault;
pub mod deposit;
pub mod emergency_withdraw;
pub mod execute_action;
pub mod initialize_protocol;
pub mod manual_rebalance;
pub mod report_rebalance_attempt;
pub mod retry_pending_rebalance;
pub mod set_protocol_pause;
pub mod transfer_admin;
pub mod update_protocol_config;
pub mod withdraw;
pub mod withdraw_treasury;

// Anchor's #[program] macro generates internal types (e.g. __client_accounts_*)
// that are only accessible via glob re-exports. The ambiguous_glob_reexports
// warning is a known Anchor 1.0 limitation — all modules intentionally export
// a private `handler` fn with the same name, which is called via module path
// (e.g. `create_vault::handler(...)`), not via this glob.
#[allow(ambiguous_glob_reexports)]
pub use admin_emergency_transfer::*;
#[allow(ambiguous_glob_reexports)]
pub use cancel_pending_rebalance::*;
#[allow(ambiguous_glob_reexports)]
pub use claim_rewards::*;
#[allow(ambiguous_glob_reexports)]
pub use close_vault::*;
#[allow(ambiguous_glob_reexports)]
pub use create_vault::*;
#[allow(ambiguous_glob_reexports)]
pub use deposit::*;
#[allow(ambiguous_glob_reexports)]
pub use emergency_withdraw::*;
#[allow(ambiguous_glob_reexports)]
pub use execute_action::*;
#[allow(ambiguous_glob_reexports)]
pub use initialize_protocol::*;
#[allow(ambiguous_glob_reexports)]
pub use manual_rebalance::*;
#[allow(ambiguous_glob_reexports)]
pub use report_rebalance_attempt::*;
#[allow(ambiguous_glob_reexports)]
pub use retry_pending_rebalance::*;
#[allow(ambiguous_glob_reexports)]
pub use set_protocol_pause::*;
#[allow(ambiguous_glob_reexports)]
pub use transfer_admin::*;
#[allow(ambiguous_glob_reexports)]
pub use update_protocol_config::*;
#[allow(ambiguous_glob_reexports)]
pub use withdraw::*;
#[allow(ambiguous_glob_reexports)]
pub use withdraw_treasury::*;
