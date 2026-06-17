//! Shared glue helpers for the Orca Whirlpools instruction handlers
//! (`execute_action_orca`, `start_rebalance_orca`, `retry_pending_rebalance`).
//!
//! These three handlers all sign CPIs with the vault PDA, route per-mint
//! performance-fee transfers to treasury, and bind tick-array accounts to a
//! range. The logic is identical across the three, so it lives here once
//! instead of being copy-pasted per file.

use crate::{
    errors::AargauError,
    state::VaultAccount,
    utils::{
        orca::accounts::{derive_tick_array_pda, tick_array_start_index},
        signer_seeds::vault_signer_seeds,
    },
};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, TransferChecked};

/// Owned byte buffers backing the vault PDA signer seeds.
///
/// The vault signer seeds borrow these byte buffers, so the buffer must
/// outlive the seeds. Construct it on the stack in the handler, then call
/// [`VaultSignerSeedBytes::seeds`].
pub struct VaultSignerSeedBytes {
    user: [u8; 32],
    pool: [u8; 32],
    bump: [u8; 1],
}

impl VaultSignerSeedBytes {
    pub fn new(vault: &VaultAccount) -> Self {
        Self {
            user: vault.user_authority.to_bytes(),
            pool: vault.pool_address.to_bytes(),
            bump: [vault.bump],
        }
    }

    pub fn seeds(&self) -> [&[u8]; 4] {
        vault_signer_seeds(&self.user, &self.pool, &self.bump)
    }
}

/// Transfer `amount` of `mint` from the vault ATA to the treasury ATA, signed
/// by the vault PDA. No-op when `amount == 0`.
#[allow(clippy::too_many_arguments)]
pub fn transfer_performance_fee<'info>(
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

/// Bind a passed tick-array account to the PDA covering `tick` for the given
/// whirlpool and tick spacing. Rejects a mismatch with `InvalidPool`.
pub fn require_tick_array(
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
