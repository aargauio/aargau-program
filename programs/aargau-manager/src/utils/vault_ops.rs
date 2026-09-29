use crate::{constants::*, errors::AargauError};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, TransferChecked};

/// Validates that `[lower_bin_id, upper_bin_id]` is non-empty and does not
/// exceed the per-position bin cap defined by `MAX_METEORA_BINS`. This guard
/// prevents unbounded position sizes that would cause CPI transaction size
/// issues across Meteora bin arrays.
pub fn require_bin_count_within_cap(lower_bin_id: i32, upper_bin_id: i32) -> Result<()> {
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

/// Rejects bin ranges whose endpoints fall outside the inline `LbPair` bitmap
/// (|bin_id| > `METEORA_INLINE_BITMAP_BIN_LIMIT`). Ranges that cross this
/// boundary require the optional `bin_array_bitmap_extension` PDA, which is
/// not yet wired. Returns `BitmapExtensionRequired`.
pub fn require_range_within_inline_bitmap(lower_bin_id: i32, upper_bin_id: i32) -> Result<()> {
    require!(
        lower_bin_id >= -METEORA_INLINE_BITMAP_BIN_LIMIT
            && upper_bin_id <= METEORA_INLINE_BITMAP_BIN_LIMIT,
        AargauError::BitmapExtensionRequired
    );
    Ok(())
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

/// Custody bind for reward collection: the destination that a protocol's
/// reward-collection CPI (e.g. Orca `collect_reward_v2`) pays into must be
/// the vault PDA's own associated token account for the reward mint. The
/// destination is supplied via `remaining_accounts` (which Anchor does not
/// validate), so without this check a caller could route rewards into an
/// arbitrary account it controls.
///
/// Uses the program-id-aware ATA derivation so an SPL-classic and a Token-2022
/// reward mint each resolve to their correct ATA. Returns `InvalidRewardOwner`
/// on mismatch.
pub fn require_reward_owner_is_vault_ata(
    reward_owner_account: &Pubkey,
    vault: &Pubkey,
    reward_mint: &Pubkey,
    reward_token_program: &Pubkey,
) -> Result<()> {
    let expected = anchor_spl::associated_token::get_associated_token_address_with_program_id(
        vault,
        reward_mint,
        reward_token_program,
    );
    require_keys_eq!(
        *reward_owner_account,
        expected,
        AargauError::InvalidRewardOwner
    );
    Ok(())
}
