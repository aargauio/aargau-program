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
