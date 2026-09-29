//! Raydium CLMM replay fixtures: two mainnet pools snapshotted under
//! `tests/fixtures/raydium/<pool>/`, plus the tick ranges the replay opens.
//!
//! Each pool directory holds a `manifest.txt` (`<label> <pubkey>` per line)
//! and one `<label>.json` snapshot per entry, all captured in a single
//! `getMultipleAccounts` call so they share one slot. The tests below re-check
//! every snapshot's owner, discriminator and binding to its pool, so a bad
//! re-capture fails on every `cargo test` instead of inside the replay.
//!
//! Replay accounts that are **not** snapshotted (the replay creates them):
//! the user, the vault PDA and its ATAs, the treasury ATAs, the position NFT
//! mint and account, the `PersonalPositionState` and `protocol_position`.

use std::path::PathBuf;

use anchor_lang::prelude::{pubkey, Pubkey};

use super::account_snapshot::{read_account_snapshot, AccountSnapshot};
use super::fixtures_dir;

/// A `[tick_lower, tick_upper)` position range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickRange {
    pub tick_lower: i32,
    pub tick_upper: i32,
}

/// Emission window of a pool reward slot, in unix seconds. The replay sets
/// `Clock::unix_timestamp` inside it for rewards to accrue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewardWindow {
    pub open_time_secs: u64,
    pub end_time_secs: u64,
}

/// One snapshotted pool and the ranges the replay uses on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RaydiumReplayPool {
    /// Directory under `tests/fixtures/raydium/`.
    pub snapshot_dir_name: &'static str,
    pub pool_state: Pubkey,
    pub tick_spacing: u16,
    /// `tick_current` in the committed `pool_state` snapshot.
    pub snapshot_tick_current: i32,
    /// Range opened by the lifecycle replay (open → increase → collect →
    /// decrease → close). Straddles the snapshot tick, with its bounds in two
    /// different captured tick arrays.
    pub lifecycle_range: TickRange,
    /// Target range of the two-transaction rebalance replay. Also straddles
    /// the snapshot tick, on a different pair of tick arrays.
    pub rebalance_range: TickRange,
    /// Window of reward slot 0, the only initialized slot in both pools.
    pub reward_0_window: RewardWindow,
    /// Start indices of every captured tick array, ascending.
    pub captured_tick_array_starts: &'static [i32],
}

/// WSOL/USDC, tick spacing 1, SPL Token mints. Reward 0 (RAY) has ended but
/// stays initialized, so its triple is still mandatory on decrease.
pub const RAYDIUM_WSOL_USDC_POOL: RaydiumReplayPool = RaydiumReplayPool {
    snapshot_dir_name: "wsol_usdc_ts1",
    pool_state: pubkey!("3ucNos4NbumPLZNWztqGHNFFgkHeRMBQAVemeeomsUxv"),
    tick_spacing: 1,
    snapshot_tick_current: -21_356,
    lifecycle_range: TickRange {
        tick_lower: -21_420,
        tick_upper: -21_300,
    },
    rebalance_range: TickRange {
        tick_lower: -21_540,
        tick_upper: -21_180,
    },
    reward_0_window: RewardWindow {
        open_time_secs: 1_776_754_800,
        end_time_secs: 1_779_778_800,
    },
    captured_tick_array_starts: &[
        -21_600, -21_540, -21_480, -21_420, -21_360, -21_300, -21_240, -21_180, -21_120,
    ],
};

/// USTM/USDT, tick spacing 10, SPL Token mints. Reward 0 is active and paid
/// in USTM, which is also `token_mint_0`: the case where an ATA balance diff
/// would mix LP fees with rewards.
pub const RAYDIUM_USTM_USDT_POOL: RaydiumReplayPool = RaydiumReplayPool {
    snapshot_dir_name: "ustm_usdt_ts10",
    pool_state: pubkey!("5jyqmaMwichK6rcXzM69krSujDvzR8pDGv5i8kBAKEM4"),
    tick_spacing: 10,
    snapshot_tick_current: -38_628,
    lifecycle_range: TickRange {
        tick_lower: -39_100,
        tick_upper: -38_300,
    },
    rebalance_range: TickRange {
        tick_lower: -38_900,
        tick_upper: -38_200,
    },
    reward_0_window: RewardWindow {
        open_time_secs: 1_783_803_600,
        end_time_secs: 1_791_579_600,
    },
    // -40200 and everything above -38400 were not initialized at capture.
    captured_tick_array_starts: &[-40_800, -39_600, -39_000, -38_400],
};

/// Both replay pools.
pub const RAYDIUM_REPLAY_POOLS: [RaydiumReplayPool; 2] =
    [RAYDIUM_WSOL_USDC_POOL, RAYDIUM_USTM_USDT_POOL];

/// A snapshot together with its manifest label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabeledAccountSnapshot {
    pub label: String,
    pub snapshot: AccountSnapshot,
}

/// Manifest label of the tick array starting at `start_tick_index`.
pub fn tick_array_label(start_tick_index: i32) -> String {
    format!("tick_array_{start_tick_index}")
}

/// Absolute path of a pool's snapshot directory.
pub fn raydium_snapshot_dir(pool: &RaydiumReplayPool) -> PathBuf {
    fixtures_dir().join("raydium").join(pool.snapshot_dir_name)
}

/// Load one snapshot of `pool` by manifest label (e.g. `"pool_state"`,
/// `"token_vault_0"`, `tick_array_label(-21_360)`).
pub fn load_raydium_account_snapshot(
    pool: &RaydiumReplayPool,
    label: &str,
) -> Result<AccountSnapshot, String> {
    let path = raydium_snapshot_dir(pool).join(format!("{label}.json"));
    if !path.exists() {
        return Err(format!(
            "Raydium snapshot not found at {}. Snapshots are committed; \
             re-capture them with \
             `sh programs/aargau-manager/tests/fixtures/dump-raydium-fixtures.sh snapshots {}`",
            path.display(),
            raydium_snapshot_dir(pool).display()
        ));
    }
    read_account_snapshot(&path)
}

/// Load every snapshot listed in the pool's manifest, in manifest order, and
/// check each file's pubkey against its manifest entry.
pub fn load_raydium_pool_snapshots(
    pool: &RaydiumReplayPool,
) -> Result<Vec<LabeledAccountSnapshot>, String> {
    read_manifest(pool)?
        .into_iter()
        .map(|(label, pubkey)| {
            let snapshot = load_raydium_account_snapshot(pool, &label)?;
            if snapshot.pubkey != pubkey {
                return Err(format!(
                    "snapshot `{label}` holds {} but the manifest lists {pubkey}",
                    snapshot.pubkey
                ));
            }
            Ok(LabeledAccountSnapshot { label, snapshot })
        })
        .collect()
}

fn read_manifest(pool: &RaydiumReplayPool) -> Result<Vec<(String, Pubkey)>, String> {
    let path = raydium_snapshot_dir(pool).join("manifest.txt");
    let manifest = std::fs::read_to_string(&path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    manifest
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            parse_manifest_line(line).map_err(|error| format!("{}: {error}", path.display()))
        })
        .collect()
}

fn parse_manifest_line(line: &str) -> Result<(String, Pubkey), String> {
    let mut words = line.split_whitespace();
    match (words.next(), words.next(), words.next()) {
        (Some(label), Some(pubkey), None) => {
            let pubkey = pubkey
                .parse::<Pubkey>()
                .map_err(|error| format!("`{line}`: {error}"))?;
            Ok((label.to_string(), pubkey))
        }
        _ => Err(format!("`{line}`: expected `<label> <pubkey>`")),
    }
}

#[cfg(test)]
mod tests {
    use aargau_manager::constants::{
        RAYDIUM_CLMM_PROGRAM_ID, RAYDIUM_POOL_REWARD_INFOS_OFFSET,
        RAYDIUM_POOL_STATE_DISCRIMINATOR, RAYDIUM_TICK_ARRAY_BITMAP_EXTENSION_DISCRIMINATOR,
        RAYDIUM_TICK_ARRAY_DISCRIMINATOR, SPL_TOKEN_PROGRAM_ID,
    };
    use aargau_manager::utils::raydium::accounts::{
        derive_tick_array_bitmap_extension_pda, derive_tick_array_pda, tick_array_start_index,
    };
    use aargau_manager::utils::raydium::pool_state_view::{
        parse_pool_state_view_from_bytes, PoolStateView,
    };
    use sha2::{Digest, Sha256};

    use super::*;

    // `PoolState` offsets the crate does not read (absolute, packed layout).
    const POOL_AMM_CONFIG_OFFSET: usize = 9;
    const POOL_OBSERVATION_OFFSET: usize = 201;
    const POOL_MINT_DECIMALS_0_OFFSET: usize = 233;
    const POOL_MINT_DECIMALS_1_OFFSET: usize = 234;
    const POOL_TICK_CURRENT_OFFSET: usize = 269;
    const POOL_STATE_LEN: usize = 1544;
    // `RewardInfo` offsets relative to the slot start.
    const REWARD_INFO_OPEN_TIME_RELATIVE_OFFSET: usize = 1;
    const REWARD_INFO_END_TIME_RELATIVE_OFFSET: usize = 9;

    const TICK_ARRAY_POOL_ID_OFFSET: usize = 8;
    const TICK_ARRAY_START_INDEX_OFFSET: usize = 40;
    const TICK_ARRAY_STATE_LEN: usize = 10_240;
    const BITMAP_EXTENSION_POOL_ID_OFFSET: usize = 8;

    // SPL Token classic layouts.
    const SPL_MINT_LEN: usize = 82;
    const SPL_MINT_DECIMALS_OFFSET: usize = 44;
    const SPL_MINT_IS_INITIALIZED_OFFSET: usize = 45;
    const SPL_TOKEN_ACCOUNT_LEN: usize = 165;
    const SPL_TOKEN_ACCOUNT_MINT_OFFSET: usize = 0;
    const SPL_TOKEN_ACCOUNT_AUTHORITY_OFFSET: usize = 32;
    const SPL_TOKEN_ACCOUNT_STATE_OFFSET: usize = 108;
    const SPL_TOKEN_ACCOUNT_STATE_INITIALIZED: u8 = 1;

    fn account_discriminator(account_name: &str) -> [u8; 8] {
        let digest = Sha256::digest(format!("account:{account_name}").as_bytes());
        let mut discriminator = [0u8; 8];
        discriminator.copy_from_slice(&digest[..8]);
        discriminator
    }

    fn read_pubkey_at(data: &[u8], offset: usize) -> Pubkey {
        Pubkey::try_from(&data[offset..offset + 32]).unwrap()
    }

    fn read_i32_at(data: &[u8], offset: usize) -> i32 {
        i32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
    }

    fn read_u64_at(data: &[u8], offset: usize) -> u64 {
        u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap())
    }

    fn load(pool: &RaydiumReplayPool, label: &str) -> AccountSnapshot {
        load_raydium_account_snapshot(pool, label).unwrap()
    }

    fn pool_state_data(pool: &RaydiumReplayPool) -> Vec<u8> {
        load(pool, "pool_state").data
    }

    fn pool_view(pool: &RaydiumReplayPool) -> PoolStateView {
        parse_pool_state_view_from_bytes(&pool_state_data(pool)).unwrap()
    }

    /// Reward 0's mint snapshot: its own entry, or `mint_0` when the reward
    /// is paid in the pair's first mint (listed once in the manifest).
    fn reward_0_mint_label(pool: &RaydiumReplayPool) -> &'static str {
        let view = pool_view(pool);
        if view.rewards[0].token_mint == view.token_mint_0 {
            "mint_0"
        } else {
            "reward_0_mint"
        }
    }

    #[test]
    fn every_manifest_entry_loads_and_all_snapshots_share_one_slot() {
        for pool in &RAYDIUM_REPLAY_POOLS {
            let snapshots = load_raydium_pool_snapshots(pool).unwrap();
            let slot = snapshots[0].snapshot.slot;
            for entry in &snapshots {
                assert_eq!(
                    entry.snapshot.slot, slot,
                    "{}: {}",
                    pool.snapshot_dir_name, entry.label
                );
                assert!(!entry.snapshot.is_executable, "{}", entry.label);
                assert!(entry.snapshot.lamports > 0, "{}", entry.label);
            }
        }
    }

    #[test]
    fn pool_state_is_a_clmm_pool_bound_to_the_captured_accounts() {
        for pool in &RAYDIUM_REPLAY_POOLS {
            let snapshot = load(pool, "pool_state");
            assert_eq!(snapshot.pubkey, pool.pool_state);
            assert_eq!(snapshot.owner, RAYDIUM_CLMM_PROGRAM_ID);
            assert_eq!(snapshot.data.len(), POOL_STATE_LEN);
            assert_eq!(snapshot.data[..8], RAYDIUM_POOL_STATE_DISCRIMINATOR);

            let view = parse_pool_state_view_from_bytes(&snapshot.data).unwrap();
            assert_eq!(view.tick_spacing, pool.tick_spacing);
            assert_eq!(view.token_mint_0, load(pool, "mint_0").pubkey);
            assert_eq!(view.token_mint_1, load(pool, "mint_1").pubkey);
            assert_eq!(view.token_vault_0, load(pool, "token_vault_0").pubkey);
            assert_eq!(view.token_vault_1, load(pool, "token_vault_1").pubkey);
            assert_eq!(
                read_pubkey_at(&snapshot.data, POOL_AMM_CONFIG_OFFSET),
                load(pool, "amm_config").pubkey
            );
            assert_eq!(
                read_pubkey_at(&snapshot.data, POOL_OBSERVATION_OFFSET),
                load(pool, "observation_state").pubkey
            );

            let initialized: Vec<_> = view.initialized_rewards().collect();
            assert_eq!(initialized.len(), 1, "{}", pool.snapshot_dir_name);
            assert_eq!(
                view.rewards[0].token_vault,
                load(pool, "reward_0_vault").pubkey
            );
            assert_eq!(
                view.rewards[0].token_mint,
                load(pool, reward_0_mint_label(pool)).pubkey
            );
        }
    }

    #[test]
    fn ustm_usdt_reward_is_paid_in_the_first_pair_mint() {
        let view = pool_view(&RAYDIUM_USTM_USDT_POOL);
        assert_eq!(view.rewards[0].token_mint, view.token_mint_0);
    }

    #[test]
    fn reward_0_window_matches_the_pool_state() {
        for pool in &RAYDIUM_REPLAY_POOLS {
            let data = pool_state_data(pool);
            let slot_start = RAYDIUM_POOL_REWARD_INFOS_OFFSET;
            let window = RewardWindow {
                open_time_secs: read_u64_at(
                    &data,
                    slot_start + REWARD_INFO_OPEN_TIME_RELATIVE_OFFSET,
                ),
                end_time_secs: read_u64_at(
                    &data,
                    slot_start + REWARD_INFO_END_TIME_RELATIVE_OFFSET,
                ),
            };
            assert_eq!(window, pool.reward_0_window, "{}", pool.snapshot_dir_name);
            assert!(window.open_time_secs < window.end_time_secs);
        }
    }

    #[test]
    fn tick_arrays_are_pool_pdas_with_the_labelled_start() {
        for pool in &RAYDIUM_REPLAY_POOLS {
            for &start in pool.captured_tick_array_starts {
                let snapshot = load(pool, &tick_array_label(start));
                assert_eq!(snapshot.owner, RAYDIUM_CLMM_PROGRAM_ID);
                assert_eq!(snapshot.data.len(), TICK_ARRAY_STATE_LEN);
                assert_eq!(snapshot.data[..8], RAYDIUM_TICK_ARRAY_DISCRIMINATOR);
                assert_eq!(snapshot.data[..8], account_discriminator("TickArrayState"));
                assert_eq!(
                    read_pubkey_at(&snapshot.data, TICK_ARRAY_POOL_ID_OFFSET),
                    pool.pool_state
                );
                assert_eq!(
                    read_i32_at(&snapshot.data, TICK_ARRAY_START_INDEX_OFFSET),
                    start
                );
                assert_eq!(
                    snapshot.pubkey,
                    derive_tick_array_pda(&pool.pool_state, start)
                );
                assert_eq!(
                    tick_array_start_index(start, pool.tick_spacing).unwrap(),
                    start
                );
            }
        }
    }

    #[test]
    fn bitmap_extension_is_the_pool_pda() {
        for pool in &RAYDIUM_REPLAY_POOLS {
            let snapshot = load(pool, "tick_array_bitmap_extension");
            assert_eq!(snapshot.owner, RAYDIUM_CLMM_PROGRAM_ID);
            assert_eq!(
                snapshot.data[..8],
                RAYDIUM_TICK_ARRAY_BITMAP_EXTENSION_DISCRIMINATOR
            );
            assert_eq!(
                snapshot.data[..8],
                account_discriminator("TickArrayBitmapExtension")
            );
            assert_eq!(
                read_pubkey_at(&snapshot.data, BITMAP_EXTENSION_POOL_ID_OFFSET),
                pool.pool_state
            );
            assert_eq!(
                snapshot.pubkey,
                derive_tick_array_bitmap_extension_pda(&pool.pool_state)
            );
        }
    }

    #[test]
    fn amm_config_and_observation_carry_clmm_discriminators() {
        for pool in &RAYDIUM_REPLAY_POOLS {
            for (label, account_name) in [
                ("amm_config", "AmmConfig"),
                ("observation_state", "ObservationState"),
            ] {
                let snapshot = load(pool, label);
                assert_eq!(snapshot.owner, RAYDIUM_CLMM_PROGRAM_ID, "{label}");
                assert_eq!(
                    snapshot.data[..8],
                    account_discriminator(account_name),
                    "{label}"
                );
            }
        }
    }

    #[test]
    fn pool_and_reward_vaults_are_initialized_spl_token_accounts_owned_by_the_pool() {
        for pool in &RAYDIUM_REPLAY_POOLS {
            for (vault_label, mint_label) in [
                ("token_vault_0", "mint_0"),
                ("token_vault_1", "mint_1"),
                ("reward_0_vault", reward_0_mint_label(pool)),
            ] {
                let vault = load(pool, vault_label);
                let expected_mint = load(pool, mint_label).pubkey;
                assert_eq!(vault.owner, SPL_TOKEN_PROGRAM_ID, "{vault_label}");
                assert_eq!(vault.data.len(), SPL_TOKEN_ACCOUNT_LEN, "{vault_label}");
                assert_eq!(
                    read_pubkey_at(&vault.data, SPL_TOKEN_ACCOUNT_MINT_OFFSET),
                    expected_mint,
                    "{vault_label}"
                );
                assert_eq!(
                    read_pubkey_at(&vault.data, SPL_TOKEN_ACCOUNT_AUTHORITY_OFFSET),
                    pool.pool_state,
                    "{vault_label}"
                );
                assert_eq!(
                    vault.data[SPL_TOKEN_ACCOUNT_STATE_OFFSET], SPL_TOKEN_ACCOUNT_STATE_INITIALIZED,
                    "{vault_label}"
                );
            }
        }
    }

    #[test]
    fn mints_are_initialized_spl_token_mints_with_the_pool_decimals() {
        for pool in &RAYDIUM_REPLAY_POOLS {
            let data = pool_state_data(pool);
            for (label, pool_decimals) in [
                ("mint_0", Some(data[POOL_MINT_DECIMALS_0_OFFSET])),
                ("mint_1", Some(data[POOL_MINT_DECIMALS_1_OFFSET])),
                (reward_0_mint_label(pool), None),
            ] {
                let mint = load(pool, label);
                assert_eq!(mint.owner, SPL_TOKEN_PROGRAM_ID, "{label}");
                assert_eq!(mint.data.len(), SPL_MINT_LEN, "{label}");
                assert_eq!(mint.data[SPL_MINT_IS_INITIALIZED_OFFSET], 1, "{label}");
                if let Some(decimals) = pool_decimals {
                    assert_eq!(mint.data[SPL_MINT_DECIMALS_OFFSET], decimals, "{label}");
                }
            }
        }
    }

    #[test]
    fn replay_ranges_straddle_the_snapshot_tick_on_captured_tick_arrays() {
        for pool in &RAYDIUM_REPLAY_POOLS {
            let tick_current = read_i32_at(&pool_state_data(pool), POOL_TICK_CURRENT_OFFSET);
            assert_eq!(tick_current, pool.snapshot_tick_current);

            let current_start = tick_array_start_index(tick_current, pool.tick_spacing).unwrap();
            let span = 60 * i32::from(pool.tick_spacing);
            for start in [current_start - span, current_start, current_start + span] {
                assert!(
                    pool.captured_tick_array_starts.contains(&start),
                    "{}: tick array {start} around the current tick is not captured",
                    pool.snapshot_dir_name
                );
            }

            for range in [pool.lifecycle_range, pool.rebalance_range] {
                assert!(range.tick_lower <= tick_current && tick_current < range.tick_upper);
                assert_eq!(range.tick_lower % i32::from(pool.tick_spacing), 0);
                assert_eq!(range.tick_upper % i32::from(pool.tick_spacing), 0);
                let lower_start =
                    tick_array_start_index(range.tick_lower, pool.tick_spacing).unwrap();
                let upper_start =
                    tick_array_start_index(range.tick_upper, pool.tick_spacing).unwrap();
                assert_ne!(
                    lower_start, upper_start,
                    "bounds should use two tick arrays"
                );
                assert!(pool.captured_tick_array_starts.contains(&lower_start));
                assert!(pool.captured_tick_array_starts.contains(&upper_start));
            }
            assert_ne!(pool.lifecycle_range, pool.rebalance_range);
        }
    }

    #[test]
    fn captured_tick_arrays_match_the_manifest() {
        for pool in &RAYDIUM_REPLAY_POOLS {
            let manifest_starts: Vec<i32> = load_raydium_pool_snapshots(pool)
                .unwrap()
                .iter()
                .filter_map(|entry| entry.label.strip_prefix("tick_array_"))
                .filter_map(|suffix| suffix.parse().ok())
                .collect();
            assert_eq!(manifest_starts, pool.captured_tick_array_starts);
        }
    }
}
