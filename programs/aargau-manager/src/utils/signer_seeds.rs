use crate::state::VaultAccount;

/// Build the signer seeds array for the vault PDA CPI signing.
///
/// Seeds: [b"vault", user_authority, pool_address, bump]
///
/// Usage (in instruction handler):
/// ```text
/// let bump_bytes = [vault.bump];
/// let seeds = vault_signer_seeds(
///     vault.user_authority.as_ref(),
///     vault.pool_address.as_ref(),
///     &bump_bytes,
/// );
/// let signer: &[&[&[u8]]] = &[&seeds];
/// ```
#[inline]
pub fn vault_signer_seeds<'a>(
    user_authority: &'a [u8],
    pool_address: &'a [u8],
    bump: &'a [u8],
) -> [&'a [u8]; 4] {
    [VaultAccount::SEEDS, user_authority, pool_address, bump]
}
