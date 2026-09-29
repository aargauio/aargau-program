//! Raw CPI builder for Raydium CLMM `decrease_liquidity_v2`.
//!
//! Removes `liquidity` from the position into the vault ATAs and, in the same
//! instruction, pays out **all** LP fees and rewards owed — Raydium has no
//! separate collect instruction. A call with `liquidity = 0` therefore
//! collects fees and rewards only (Raydium skips the slippage check when
//! `liquidity == 0`). The vault PDA is `nft_owner` and signs via
//! `invoke_signed`.
//!
//! `amount_0_min` / `amount_1_min` are compared against the principal net of
//! any Token-2022 transfer fee; fees owed are excluded from that comparison.
//!
//! Account order mirrors the on-chain `#[derive(Accounts)]` `DecreaseLiquidityV2`:
//!
//! | idx | account                   | flags             | binding                                  |
//! |----:|---------------------------|-------------------|------------------------------------------|
//! |  0  | nft_owner                 | readonly + signer | **vault PDA** (signs)                    |
//! |  1  | nft_account               | readonly          | vault NFT ATA (amount == 1)              |
//! |  2  | personal_position         | writable          | PDA [b"position", nft_mint]              |
//! |  3  | pool_state                | writable          | vault pool                               |
//! |  4  | protocol_position         | readonly          | deprecated; `RAYDIUM_PROTOCOL_POSITION_PLACEHOLDER` |
//! |  5  | token_vault_0             | writable          | == pool.token_vault_0                    |
//! |  6  | token_vault_1             | writable          | == pool.token_vault_1                    |
//! |  7  | tick_array_lower          | writable          | PDA(pool, start(tick_lower)) BE seed     |
//! |  8  | tick_array_upper          | writable          | PDA(pool, start(tick_upper)) BE seed     |
//! |  9  | recipient_token_account_0 | writable          | vault ATA, mint 0                        |
//! | 10  | recipient_token_account_1 | writable          | vault ATA, mint 1                        |
//! | 11  | token_program             | readonly          | == SPL_TOKEN_PROGRAM_ID                  |
//! | 12  | token_program_2022        | readonly          | == TOKEN_2022_PROGRAM_ID                 |
//! | 13  | memo_program              | readonly          | == MEMO_PROGRAM_ID                       |
//! | 14  | vault_0_mint              | readonly          | == pool.token_mint_0                     |
//! | 15  | vault_1_mint              | readonly          | == pool.token_mint_1                     |
//! | 16  | tick_array_bitmap_ext     | writable          | remaining[0], PDA(pool)                  |
//! | 17+ | per initialized reward slot, in slot order:                                      |
//! |     | reward_vault              | writable          | == pool.reward_infos[i].token_vault      |
//! |     | recipient                 | writable          | vault ATA for the reward mint            |
//! |     | reward_mint               | readonly          | == pool.reward_infos[i].token_mint       |
//!
//! Raydium matches the bitmap extension by key and treats every other
//! remaining account as a reward triple, so the extension may sit first.
//!
//! Args (Borsh):
//! ```text
//! discriminator: [u8; 8]
//! liquidity: u128
//! amount_0_min: u64
//! amount_1_min: u64
//! ```

use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};

use crate::constants::{
    MEMO_PROGRAM_ID, RAYDIUM_CLMM_PROGRAM_ID, RAYDIUM_DECREASE_LIQUIDITY_V2_DISCRIMINATOR,
    RAYDIUM_PROTOCOL_POSITION_PLACEHOLDER, RAYDIUM_REWARD_ACCOUNTS_PER_SLOT, RAYDIUM_REWARD_SLOTS,
    SPL_TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID,
};

use super::reward_accounts::{RewardTransferAccounts, RewardTransferKeys};

/// 16 fixed accounts + the bitmap extension + three reward triples at most.
const MAX_DECREASE_ACCOUNTS: usize = 17 + RAYDIUM_REWARD_SLOTS * RAYDIUM_REWARD_ACCOUNTS_PER_SLOT;

pub struct DecreaseLiquidityV2Cpi<'a, 'info> {
    pub clmm_program: AccountInfo<'info>,
    /// Vault PDA — `nft_owner` signer.
    pub vault: AccountInfo<'info>,
    pub nft_account: AccountInfo<'info>,
    pub personal_position: AccountInfo<'info>,
    pub pool_state: AccountInfo<'info>,
    pub token_vault_0: AccountInfo<'info>,
    pub token_vault_1: AccountInfo<'info>,
    pub tick_array_lower: AccountInfo<'info>,
    pub tick_array_upper: AccountInfo<'info>,
    pub vault_token_0: AccountInfo<'info>,
    pub vault_token_1: AccountInfo<'info>,
    pub token_program: AccountInfo<'info>,
    pub token_program_2022: AccountInfo<'info>,
    pub memo_program: AccountInfo<'info>,
    pub vault_0_mint: AccountInfo<'info>,
    pub vault_1_mint: AccountInfo<'info>,
    pub tick_array_bitmap_extension: AccountInfo<'info>,
    /// One triple per initialized reward slot, already bound by
    /// `reward_accounts::bind_reward_transfer_accounts`.
    pub reward_accounts: &'a [RewardTransferAccounts<'info>],
}

/// Account keys for [`build_decrease_liquidity_v2_instruction_data`].
pub struct DecreaseLiquidityV2Keys {
    pub nft_owner: Pubkey,
    pub nft_account: Pubkey,
    pub personal_position: Pubkey,
    pub pool_state: Pubkey,
    pub protocol_position: Pubkey,
    pub token_vault_0: Pubkey,
    pub token_vault_1: Pubkey,
    pub tick_array_lower: Pubkey,
    pub tick_array_upper: Pubkey,
    pub recipient_token_account_0: Pubkey,
    pub recipient_token_account_1: Pubkey,
    pub token_program: Pubkey,
    pub token_program_2022: Pubkey,
    pub memo_program: Pubkey,
    pub vault_0_mint: Pubkey,
    pub vault_1_mint: Pubkey,
    pub tick_array_bitmap_extension: Pubkey,
    pub reward_accounts: Vec<RewardTransferKeys>,
}

/// Pure constructor for the `decrease_liquidity_v2` instruction bytes +
/// account metas.
pub fn build_decrease_liquidity_v2_instruction_data(
    keys: &DecreaseLiquidityV2Keys,
    liquidity: u128,
    amount_0_min: u64,
    amount_1_min: u64,
) -> (Vec<u8>, Vec<AccountMeta>) {
    let mut data = Vec::with_capacity(8 + 16 + 8 + 8);
    data.extend_from_slice(&RAYDIUM_DECREASE_LIQUIDITY_V2_DISCRIMINATOR);
    data.extend_from_slice(&liquidity.to_le_bytes());
    data.extend_from_slice(&amount_0_min.to_le_bytes());
    data.extend_from_slice(&amount_1_min.to_le_bytes());

    let mut metas = Vec::with_capacity(MAX_DECREASE_ACCOUNTS);
    metas.extend([
        AccountMeta::new_readonly(keys.nft_owner, true),
        AccountMeta::new_readonly(keys.nft_account, false),
        AccountMeta::new(keys.personal_position, false),
        AccountMeta::new(keys.pool_state, false),
        AccountMeta::new_readonly(keys.protocol_position, false),
        AccountMeta::new(keys.token_vault_0, false),
        AccountMeta::new(keys.token_vault_1, false),
        AccountMeta::new(keys.tick_array_lower, false),
        AccountMeta::new(keys.tick_array_upper, false),
        AccountMeta::new(keys.recipient_token_account_0, false),
        AccountMeta::new(keys.recipient_token_account_1, false),
        AccountMeta::new_readonly(keys.token_program, false),
        AccountMeta::new_readonly(keys.token_program_2022, false),
        AccountMeta::new_readonly(keys.memo_program, false),
        AccountMeta::new_readonly(keys.vault_0_mint, false),
        AccountMeta::new_readonly(keys.vault_1_mint, false),
        AccountMeta::new(keys.tick_array_bitmap_extension, false),
    ]);
    for reward in &keys.reward_accounts {
        metas.extend([
            AccountMeta::new(reward.reward_vault, false),
            AccountMeta::new(reward.recipient, false),
            AccountMeta::new_readonly(reward.reward_mint, false),
        ]);
    }

    (data, metas)
}

pub fn invoke_decrease_liquidity_v2(
    cpi: &DecreaseLiquidityV2Cpi<'_, '_>,
    liquidity: u128,
    amount_0_min: u64,
    amount_1_min: u64,
    vault_signer_seeds: &[&[u8]],
) -> Result<()> {
    require_keys_eq!(cpi.clmm_program.key(), RAYDIUM_CLMM_PROGRAM_ID);
    require_keys_eq!(cpi.token_program.key(), SPL_TOKEN_PROGRAM_ID);
    require_keys_eq!(cpi.token_program_2022.key(), TOKEN_2022_PROGRAM_ID);
    require_keys_eq!(cpi.memo_program.key(), MEMO_PROGRAM_ID);

    let keys = DecreaseLiquidityV2Keys {
        nft_owner: cpi.vault.key(),
        nft_account: cpi.nft_account.key(),
        personal_position: cpi.personal_position.key(),
        pool_state: cpi.pool_state.key(),
        protocol_position: RAYDIUM_PROTOCOL_POSITION_PLACEHOLDER,
        token_vault_0: cpi.token_vault_0.key(),
        token_vault_1: cpi.token_vault_1.key(),
        tick_array_lower: cpi.tick_array_lower.key(),
        tick_array_upper: cpi.tick_array_upper.key(),
        recipient_token_account_0: cpi.vault_token_0.key(),
        recipient_token_account_1: cpi.vault_token_1.key(),
        token_program: cpi.token_program.key(),
        token_program_2022: cpi.token_program_2022.key(),
        memo_program: cpi.memo_program.key(),
        vault_0_mint: cpi.vault_0_mint.key(),
        vault_1_mint: cpi.vault_1_mint.key(),
        tick_array_bitmap_extension: cpi.tick_array_bitmap_extension.key(),
        reward_accounts: cpi
            .reward_accounts
            .iter()
            .map(RewardTransferAccounts::keys)
            .collect(),
    };
    let (data, metas) =
        build_decrease_liquidity_v2_instruction_data(&keys, liquidity, amount_0_min, amount_1_min);

    let ix = Instruction {
        program_id: RAYDIUM_CLMM_PROGRAM_ID,
        accounts: metas,
        data,
    };

    // `clmm_program` doubles as the `protocol_position` placeholder.
    let mut infos = Vec::with_capacity(MAX_DECREASE_ACCOUNTS);
    infos.extend([
        cpi.vault.clone(),
        cpi.nft_account.clone(),
        cpi.personal_position.clone(),
        cpi.pool_state.clone(),
        cpi.clmm_program.clone(),
        cpi.token_vault_0.clone(),
        cpi.token_vault_1.clone(),
        cpi.tick_array_lower.clone(),
        cpi.tick_array_upper.clone(),
        cpi.vault_token_0.clone(),
        cpi.vault_token_1.clone(),
        cpi.token_program.clone(),
        cpi.token_program_2022.clone(),
        cpi.memo_program.clone(),
        cpi.vault_0_mint.clone(),
        cpi.vault_1_mint.clone(),
        cpi.tick_array_bitmap_extension.clone(),
    ]);
    for reward in cpi.reward_accounts {
        infos.extend([
            reward.reward_vault.clone(),
            reward.recipient.clone(),
            reward.reward_mint.clone(),
        ]);
    }

    invoke_signed(&ix, &infos, &[vault_signer_seeds]).map_err(Into::into)
}
