//! Reward-account triples appended to Raydium CLMM `decrease_liquidity_v2`.
//!
//! Every decrease — including a zero-liquidity fee collection — pays out all
//! rewards owed, and Raydium requires exactly one
//! `[reward_vault, recipient, reward_mint]` triple for **every initialized**
//! reward slot (ended rewards included), in slot order. Raydium binds each
//! reward vault to its slot but only checks that the recipient has the same
//! mint, so the recipient is bound here to the vault PDA's own ATA for the
//! reward mint.
//!
//! The triples arrive through `remaining_accounts`, which Anchor does not
//! validate: the exact count is checked first, then each account is bound to
//! the pool's on-chain reward slot.

use anchor_lang::prelude::*;

use crate::constants::RAYDIUM_REWARD_ACCOUNTS_PER_SLOT;
use crate::errors::AargauError;
use crate::utils::token_account::is_token_program;
use crate::utils::vault_ops::require_reward_owner_is_vault_ata;

use super::pool_state_view::{PoolStateView, RaydiumRewardSlot};

/// One validated reward triple, ready to forward to the CPI.
#[derive(Clone)]
pub struct RewardTransferAccounts<'info> {
    pub reward_vault: AccountInfo<'info>,
    /// Vault PDA's ATA for `reward_mint`.
    pub recipient: AccountInfo<'info>,
    pub reward_mint: AccountInfo<'info>,
}

/// Keys of one reward triple, in wire order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewardTransferKeys {
    pub reward_vault: Pubkey,
    pub recipient: Pubkey,
    pub reward_mint: Pubkey,
}

impl RewardTransferAccounts<'_> {
    pub fn keys(&self) -> RewardTransferKeys {
        RewardTransferKeys {
            reward_vault: self.reward_vault.key(),
            recipient: self.recipient.key(),
            reward_mint: self.reward_mint.key(),
        }
    }
}

/// Number of accounts `decrease_liquidity_v2` expects after the bitmap
/// extension: three per initialized reward slot.
pub fn expected_reward_account_count(view: &PoolStateView) -> Result<usize> {
    view.initialized_rewards()
        .count()
        .checked_mul(RAYDIUM_REWARD_ACCOUNTS_PER_SLOT)
        .ok_or(error!(AargauError::Overflow))
}

/// Pure binding of one triple to its pool reward slot.
///
/// - `reward_vault` must be the slot's vault (`InvalidPoolReserve`);
/// - `reward_mint` must be the slot's mint (`InvalidPoolMint`);
/// - `reward_token_program` (the mint's owner) must be SPL Token or
///   Token-2022, and `recipient` must be the vault's ATA for the mint under
///   that program (`InvalidRewardOwner`).
pub fn require_reward_transfer_bound(
    slot: &RaydiumRewardSlot,
    keys: &RewardTransferKeys,
    vault: &Pubkey,
    reward_token_program: &Pubkey,
) -> Result<()> {
    require_keys_eq!(
        keys.reward_vault,
        slot.token_vault,
        AargauError::InvalidPoolReserve
    );
    require_keys_eq!(
        keys.reward_mint,
        slot.token_mint,
        AargauError::InvalidPoolMint
    );
    require!(
        is_token_program(reward_token_program),
        AargauError::InvalidRewardOwner
    );
    require_reward_owner_is_vault_ata(
        &keys.recipient,
        vault,
        &keys.reward_mint,
        reward_token_program,
    )
}

/// Split `accounts` into reward triples and bind each one to the pool's
/// initialized reward slots, in slot order. `accounts` must hold exactly the
/// reward triples — nothing before or after them.
pub fn bind_reward_transfer_accounts<'info>(
    view: &PoolStateView,
    vault: &Pubkey,
    accounts: &[AccountInfo<'info>],
) -> Result<Vec<RewardTransferAccounts<'info>>> {
    require!(
        accounts.len() == expected_reward_account_count(view)?,
        AargauError::InvalidActionPayload
    );

    view.initialized_rewards()
        .enumerate()
        .map(|(position, slot)| {
            bind_reward_triple(slot, vault, reward_triple_at(accounts, position)?)
        })
        .collect()
}

fn reward_triple_at<'a, 'info>(
    accounts: &'a [AccountInfo<'info>],
    position: usize,
) -> Result<&'a [AccountInfo<'info>; RAYDIUM_REWARD_ACCOUNTS_PER_SLOT]> {
    let start = position
        .checked_mul(RAYDIUM_REWARD_ACCOUNTS_PER_SLOT)
        .ok_or(AargauError::Overflow)?;
    let end = start
        .checked_add(RAYDIUM_REWARD_ACCOUNTS_PER_SLOT)
        .ok_or(AargauError::Overflow)?;
    accounts
        .get(start..end)
        .and_then(|triple| triple.try_into().ok())
        .ok_or(error!(AargauError::InvalidActionPayload))
}

fn bind_reward_triple<'info>(
    slot: &RaydiumRewardSlot,
    vault: &Pubkey,
    triple: &[AccountInfo<'info>; RAYDIUM_REWARD_ACCOUNTS_PER_SLOT],
) -> Result<RewardTransferAccounts<'info>> {
    let [reward_vault, recipient, reward_mint] = triple;
    let accounts = RewardTransferAccounts {
        reward_vault: reward_vault.clone(),
        recipient: recipient.clone(),
        reward_mint: reward_mint.clone(),
    };
    require_reward_transfer_bound(slot, &accounts.keys(), vault, reward_mint.owner)?;
    Ok(accounts)
}
