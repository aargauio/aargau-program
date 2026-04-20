//! Tests for `src/utils/signer_seeds.rs` — vault_signer_seeds layout and content.

#[cfg(test)]
mod signer_seeds_tests {
    use aargau_manager::state::VaultAccount;
    use aargau_manager::utils::signer_seeds::vault_signer_seeds;
    use anchor_lang::prelude::Pubkey;

    #[test]
    fn test_vault_signer_seeds_length_is_4() {
        // PDA seeds for the vault are always exactly 4 slices:
        //   [b"vault", user_authority, pool_address, bump]
        let user = Pubkey::new_unique();
        let pool = Pubkey::new_unique();
        let bump = [42u8];
        let seeds = vault_signer_seeds(user.as_ref(), pool.as_ref(), &bump);
        assert_eq!(seeds.len(), 4);
    }

    #[test]
    fn test_vault_signer_seeds_first_element_is_vault_literal() {
        let user = Pubkey::new_unique();
        let pool = Pubkey::new_unique();
        let bump = [0u8];
        let seeds = vault_signer_seeds(user.as_ref(), pool.as_ref(), &bump);
        assert_eq!(seeds[0], VaultAccount::SEEDS);
        assert_eq!(seeds[0], b"vault");
    }

    #[test]
    fn test_vault_signer_seeds_second_element_is_user_authority() {
        let user = Pubkey::new_unique();
        let pool = Pubkey::new_unique();
        let bump = [1u8];
        let seeds = vault_signer_seeds(user.as_ref(), pool.as_ref(), &bump);
        assert_eq!(seeds[1], user.as_ref());
    }

    #[test]
    fn test_vault_signer_seeds_third_element_is_pool_address() {
        let user = Pubkey::new_unique();
        let pool = Pubkey::new_unique();
        let bump = [2u8];
        let seeds = vault_signer_seeds(user.as_ref(), pool.as_ref(), &bump);
        assert_eq!(seeds[2], pool.as_ref());
    }

    #[test]
    fn test_vault_signer_seeds_fourth_element_is_bump() {
        let user = Pubkey::new_unique();
        let pool = Pubkey::new_unique();
        let bump = [255u8];
        let seeds = vault_signer_seeds(user.as_ref(), pool.as_ref(), &bump);
        assert_eq!(seeds[3], &bump[..]);
    }

    #[test]
    fn test_vault_signer_seeds_distinct_users_produce_different_second_element() {
        let user_a = Pubkey::new_unique();
        let user_b = Pubkey::new_unique();
        let pool = Pubkey::new_unique();
        let bump = [0u8];
        let seeds_a = vault_signer_seeds(user_a.as_ref(), pool.as_ref(), &bump);
        let seeds_b = vault_signer_seeds(user_b.as_ref(), pool.as_ref(), &bump);
        assert_ne!(seeds_a[1], seeds_b[1]);
        // All other elements are the same.
        assert_eq!(seeds_a[0], seeds_b[0]);
        assert_eq!(seeds_a[2], seeds_b[2]);
        assert_eq!(seeds_a[3], seeds_b[3]);
    }

    #[test]
    fn test_vault_signer_seeds_distinct_pools_produce_different_third_element() {
        let user = Pubkey::new_unique();
        let pool_a = Pubkey::new_unique();
        let pool_b = Pubkey::new_unique();
        let bump = [0u8];
        let seeds_a = vault_signer_seeds(user.as_ref(), pool_a.as_ref(), &bump);
        let seeds_b = vault_signer_seeds(user.as_ref(), pool_b.as_ref(), &bump);
        assert_ne!(seeds_a[2], seeds_b[2]);
    }

    #[test]
    fn test_vault_account_seeds_constant_value() {
        assert_eq!(VaultAccount::SEEDS, b"vault");
        assert_eq!(VaultAccount::SEEDS.len(), 5);
    }
}
