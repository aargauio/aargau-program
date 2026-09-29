use crate::{
    constants::*,
    errors::AargauError,
    state::VaultAccount,
    utils::token_2022::{is_token_2022, read_transfer_fee_snapshot},
};
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

/// Lamports held above the rent-exempt minimum. Returns 0 when the balance is
/// at or below the minimum (nothing to sweep, and never an underflow).
pub fn excess_lamports_above_rent(current_lamports: u64, rent_exempt_minimum: u64) -> u64 {
    current_lamports.saturating_sub(rent_exempt_minimum)
}

/// Record the pair mints' Token-2022 transfer-fee config on the vault when a
/// position is opened. A mint can change its config between epochs, so every
/// open re-reads the live mints instead of keeping an older snapshot. SPL
/// classic mints and Token-2022 mints without the extension record zero.
pub fn record_pair_transfer_fees(
    vault: &mut VaultAccount,
    mint_a: &AccountInfo<'_>,
    mint_b: &AccountInfo<'_>,
) -> Result<()> {
    let fee_a = read_transfer_fee_snapshot(mint_a)?;
    let fee_b = read_transfer_fee_snapshot(mint_b)?;

    vault.uses_token_2022 = is_token_2022(mint_a.owner) || is_token_2022(mint_b.owner);
    vault.token_a_transfer_fee_bps = fee_a.transfer_fee_bps;
    vault.token_a_maximum_fee = fee_a.maximum_fee;
    vault.token_b_transfer_fee_bps = fee_b.transfer_fee_bps;
    vault.token_b_maximum_fee = fee_b.maximum_fee;
    Ok(())
}

/// Move every lamport the vault PDA holds above its rent-exempt minimum to
/// `user`, which must be `vault.user_authority`. Returns the amount moved.
///
/// Needed after a protocol CPI that refunds rent to the vault PDA itself
/// (Raydium `close_position` pays all reclaimed rent to `nft_owner`). Without
/// the sweep those lamports would sit in the `VaultAccount`, and funds may
/// only leave to the user. The program owns the vault account, so it debits
/// the lamports directly; the vault stays rent-exempt for its data length.
/// No-op when there is no excess.
pub fn sweep_excess_vault_lamports<'info>(
    vault: &Account<'info, VaultAccount>,
    user: &AccountInfo<'info>,
    rent: &Rent,
) -> Result<u64> {
    require_keys_eq!(
        user.key(),
        vault.user_authority,
        AargauError::UnauthorizedUser
    );

    let vault_info = vault.to_account_info();
    let rent_exempt_minimum = rent.minimum_balance(vault_info.data_len());
    let excess = excess_lamports_above_rent(vault_info.lamports(), rent_exempt_minimum);
    if excess == 0 {
        return Ok(0);
    }

    let vault_after = vault_info
        .lamports()
        .checked_sub(excess)
        .ok_or(AargauError::Underflow)?;
    let user_after = user
        .lamports()
        .checked_add(excess)
        .ok_or(AargauError::Overflow)?;
    **vault_info.try_borrow_mut_lamports()? = vault_after;
    **user.try_borrow_mut_lamports()? = user_after;
    Ok(excess)
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
