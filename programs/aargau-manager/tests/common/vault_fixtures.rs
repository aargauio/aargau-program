//! In-memory `VaultAccount` values for tests of pure vault-state transitions.

use aargau_manager::state::{AutoRebalanceStrategy, Protocol, VaultAccount};
use anchor_lang::prelude::Pubkey;

/// An idle vault (no position, nothing pending) for `protocol`.
pub fn idle_vault(protocol: Protocol) -> VaultAccount {
    VaultAccount {
        user_authority: Pubkey::new_from_array([0x0B; 32]),
        pool_address: Pubkey::new_from_array([0x0C; 32]),
        protocol,
        position_address: None,
        position_mint: None,
        position_range_lower: None,
        position_range_upper: None,
        uses_token_2022: false,
        token_a_transfer_fee_bps: 0,
        token_a_maximum_fee: 0,
        token_b_transfer_fee_bps: 0,
        token_b_maximum_fee: 0,
        reward_token_0_transfer_fee_bps: 0,
        reward_token_0_maximum_fee: 0,
        reward_token_1_transfer_fee_bps: 0,
        reward_token_1_maximum_fee: 0,
        reward_token_2_transfer_fee_bps: 0,
        reward_token_2_maximum_fee: 0,
        allowed_ops: 0,
        strategy: AutoRebalanceStrategy::default(),
        last_rebalance_at: 0,
        rebalances_today: 0,
        gas_spent_today_usd_cents: 0,
        last_day_reset: 0,
        entry_value_usd: 0,
        pending_rebalance: None,
        created_at: 0,
        bump: 255,
        _padding: [0u8; 8],
    }
}

/// A vault holding an open position over `[tick_lower, tick_upper]`.
pub fn vault_with_position(protocol: Protocol, tick_lower: i32, tick_upper: i32) -> VaultAccount {
    let mut vault = idle_vault(protocol);
    vault.position_address = Some(Pubkey::new_from_array([0xA0; 32]));
    vault.position_mint = Some(Pubkey::new_from_array([0xA1; 32]));
    vault.position_range_lower = Some(tick_lower);
    vault.position_range_upper = Some(tick_upper);
    vault
}
