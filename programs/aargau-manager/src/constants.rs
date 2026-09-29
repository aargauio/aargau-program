use anchor_lang::prelude::*;

// ---------------------------------------------------------------------------
// Protocol program IDs
// ---------------------------------------------------------------------------

/// Orca Whirlpools CLMM program (mainnet)
pub const ORCA_WHIRLPOOL_PROGRAM_ID: Pubkey =
    pubkey!("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");

/// Raydium CLMM program (mainnet)
pub const RAYDIUM_CLMM_PROGRAM_ID: Pubkey = pubkey!("CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK");

/// Meteora DLMM program (mainnet)
pub const METEORA_DLMM_PROGRAM_ID: Pubkey = pubkey!("LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo");

// ---------------------------------------------------------------------------
// Account discriminators
// Anchor 8-byte discriminators: sha256("account:<AccountName>")[0..8]
// These are used in create_vault pool validation (layer 2).
// ---------------------------------------------------------------------------

/// Orca Whirlpool account discriminator
pub const ORCA_WHIRLPOOL_DISCRIMINATOR: [u8; 8] = [63, 149, 209, 12, 225, 128, 99, 9];

/// Orca `Position` account discriminator: `sha256("account:Position")[..8]`.
/// Used to certify the account type of the position before any liquidity CPI.
pub const ORCA_POSITION_DISCRIMINATOR: [u8; 8] = [170, 188, 143, 228, 122, 64, 247, 208];

/// Raydium PoolState account discriminator
pub const RAYDIUM_POOL_STATE_DISCRIMINATOR: [u8; 8] = [247, 237, 227, 245, 215, 195, 222, 70];

/// Meteora LbPair account discriminator
pub const METEORA_LB_PAIR_DISCRIMINATOR: [u8; 8] = [33, 11, 49, 98, 181, 101, 177, 13];

// ---------------------------------------------------------------------------
// Pool layout byte offsets for mint validation in create_vault (layer 3).
//
// Every offset is ABSOLUTE into the raw account data buffer: the 8-byte
// Anchor discriminator counts as bytes 0..8. They match the protocol parsers
// at `utils/orca/whirlpool_view` (`WHIRLPOOL_TOKEN_MINT_A_OFFSET = 101`,
// `WHIRLPOOL_TOKEN_MINT_B_OFFSET = 181`) and `utils/meteora/lb_pair_view`
// (`LB_PAIR_TOKEN_X_MINT_OFFSET = 88`, `LB_PAIR_TOKEN_Y_MINT_OFFSET = 120`).
// ---------------------------------------------------------------------------

/// Absolute byte offset of token_mint_a inside Orca Whirlpool data:
/// discriminator 8 + whirlpools_config 32 + whirlpool_bump 1 + tick_spacing 2
/// + fee_tier_index_seed 2 + fee_rate 2 + protocol_fee_rate 2 + liquidity 16
/// + sqrt_price 16 + tick_current_index 4 + protocol_fee_owed_a/b 8 + 8.
pub const ORCA_MINT_A_OFFSET: usize = 101;
/// Absolute byte offset of token_mint_b inside Orca Whirlpool data:
/// token_mint_a 101 + 32 + token_vault_a 32 + fee_growth_global_a 16.
pub const ORCA_MINT_B_OFFSET: usize = 181;

/// Absolute byte offset of token_mint_0 inside Raydium CLMM PoolState data:
/// discriminator 8 + bump 1 + amm_config 32 + owner 32.
pub const RAYDIUM_MINT_A_OFFSET: usize = 73;
/// Absolute byte offset of token_mint_1 inside Raydium CLMM PoolState data:
/// token_mint_0 73 + 32.
pub const RAYDIUM_MINT_B_OFFSET: usize = 105;

/// Absolute byte offset of token_x_mint inside Meteora LbPair data
/// (the 8-byte Anchor discriminator is included in the count).
pub const METEORA_MINT_A_OFFSET: usize = 88;
/// Absolute byte offset of token_y_mint inside Meteora LbPair data.
pub const METEORA_MINT_B_OFFSET: usize = 120;

// ---------------------------------------------------------------------------
// Meteora DLMM instruction discriminators (Anchor 8-byte: sha256("global:<name>")[0..8])
// ---------------------------------------------------------------------------

/// `claim_fee2(min_bin_id: i32, max_bin_id: i32, remaining_accounts_info)`
pub const METEORA_CLAIM_FEE2_DISCRIMINATOR: [u8; 8] = [112, 191, 101, 171, 28, 144, 127, 187];

/// `add_liquidity_by_strategy2(amount_x, amount_y, active_id, max_active_bin_slippage,
///                             strategy_parameters, remaining_accounts_info)`
pub const METEORA_ADD_LIQUIDITY_BY_STRATEGY2_DISCRIMINATOR: [u8; 8] =
    [3, 221, 149, 218, 111, 141, 118, 213];

/// `remove_liquidity_by_range2(from_bin_id, to_bin_id, bps_to_remove, remaining_accounts_info)`
pub const METEORA_REMOVE_LIQUIDITY_BY_RANGE2_DISCRIMINATOR: [u8; 8] =
    [204, 2, 195, 145, 53, 145, 145, 205];

/// `initialize_position(lower_bin_id: i32, width: i32)`
/// Derived: `sha256("global:initialize_position")[..8]`.
pub const METEORA_INITIALIZE_POSITION_DISCRIMINATOR: [u8; 8] =
    [219, 192, 234, 71, 190, 191, 102, 80];

/// `close_position_if_empty()` (no args beyond discriminator).
pub const METEORA_CLOSE_POSITION_IF_EMPTY_DISCRIMINATOR: [u8; 8] =
    [59, 124, 212, 118, 91, 152, 110, 157];

/// `rebalance_liquidity(...)` — atomic close-old-bins + open-new-bins in a
/// single CPI. Derived: `sha256("global:rebalance_liquidity")[..8]`.
pub const METEORA_REBALANCE_LIQUIDITY_DISCRIMINATOR: [u8; 8] = [92, 4, 176, 193, 119, 185, 83, 9];

/// Anchor account discriminator for Meteora's `PositionV2` zero-copy account.
/// Derived: `sha256("account:PositionV2")[..8]`. Used by callers to
/// pre-validate `vault.position_address` before any CPI fires.
pub const METEORA_POSITION_V2_DISCRIMINATOR: [u8; 8] = [117, 176, 212, 199, 245, 180, 133, 182];

// ---------------------------------------------------------------------------
// Orca Whirlpools instruction discriminators (Anchor 8-byte: sha256("global:<name>")[0..8])
//
// All bytes below were re-derived via `sha256("global:<name>")[..8]` and the
// v1 / token-extension variants cross-checked byte-for-byte against the
// on-chain-verified values in the backend Orca tx builders. The `_v2`
// variants (collect_fees_v2 / collect_reward_v2) are derived here.
// ---------------------------------------------------------------------------

/// `open_position_with_token_extensions(tick_lower_index: i32, tick_upper_index: i32,
///                                      with_token_metadata_extension: bool)`.
/// Mints a Token-2022 position NFT (decimals 0, supply 1) into an ATA owned
/// by the vault PDA; no Metaplex metadata account is created.
pub const ORCA_OPEN_POSITION_WITH_TOKEN_EXTENSIONS_DISCRIMINATOR: [u8; 8] =
    [212, 47, 95, 92, 114, 102, 131, 250];

/// `increase_liquidity_v2(liquidity_amount: u128, token_max_a: u64, token_max_b: u64,
///                        remaining_accounts_info: Option<RemainingAccountsInfo>)`.
pub const ORCA_INCREASE_LIQUIDITY_V2_DISCRIMINATOR: [u8; 8] = [133, 29, 89, 223, 69, 238, 176, 10];

/// `decrease_liquidity_v2(liquidity_amount: u128, token_min_a: u64, token_min_b: u64,
///                        remaining_accounts_info: Option<RemainingAccountsInfo>)`.
pub const ORCA_DECREASE_LIQUIDITY_V2_DISCRIMINATOR: [u8; 8] = [58, 127, 188, 62, 79, 82, 196, 96];

/// `collect_fees_v2(remaining_accounts_info: Option<RemainingAccountsInfo>)`.
pub const ORCA_COLLECT_FEES_V2_DISCRIMINATOR: [u8; 8] = [207, 117, 95, 191, 229, 180, 226, 15];

/// `collect_reward_v2(reward_index: u8, remaining_accounts_info: Option<RemainingAccountsInfo>)`.
pub const ORCA_COLLECT_REWARD_V2_DISCRIMINATOR: [u8; 8] = [177, 107, 37, 180, 160, 19, 49, 209];

/// `close_position_with_token_extensions()` — burns the Token-2022 NFT,
/// closes the mint, refunds rent. Requires the position to be fully empty.
pub const ORCA_CLOSE_POSITION_WITH_TOKEN_EXTENSIONS_DISCRIMINATOR: [u8; 8] =
    [1, 182, 135, 59, 155, 25, 99, 223];

// ---------------------------------------------------------------------------
// Orca Whirlpools layout / CLMM constants
// ---------------------------------------------------------------------------

/// Number of ticks packed inside a single Orca `TickArray` account.
pub const TICK_ARRAY_SIZE: i32 = 88;

/// Max reward slots an Orca position can carry (`collect_reward_v2` runs once
/// per active index). Inactive reward vaults equal `Pubkey::default()`.
pub const ORCA_REWARD_SLOTS: usize = 3;

/// Token-2022 program — owns the position NFT mint + its ATA, and may own
/// either pair mint in a mixed pool. Per-mint detection picks the right
/// token program for each `_v2` CPI.
pub const TOKEN_2022_PROGRAM_ID: Pubkey = pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");

/// Orca position-NFT metadata update authority — account #9 of
/// `open_position_with_token_extensions`. The Whirlpools program pins this
/// key itself; we forward it.
pub const ORCA_METADATA_UPDATE_AUTH: Pubkey =
    pubkey!("3axbTs2z5GBy6usVbNVoqEgZMng3vZvMnAoX29BFfwhr");

/// PDA seed prefix for an Orca `Position` account (`[b"position", position_mint]`).
pub const ORCA_POSITION_SEED: &[u8] = b"position";

/// PDA seed prefix for an Orca `TickArray` account
/// (`[b"tick_array", whirlpool, start_tick_index.to_string()]` — third seed is
/// the ASCII decimal string of the start index, not its LE bytes).
pub const ORCA_TICK_ARRAY_SEED: &[u8] = b"tick_array";

/// SPL Token (classic) program. Raydium CLMM's `_v2` liquidity instructions
/// carry it in a fixed slot next to Token-2022 and pick per transfer by the
/// source token account's owner.
pub const SPL_TOKEN_PROGRAM_ID: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

// ---------------------------------------------------------------------------
// Raydium CLMM instruction discriminators (Anchor 8-byte: sha256("global:<name>")[0..8])
//
// Re-derived in `tests/constants.rs`. Account orders and arg layouts are
// documented in the `//!` header of each builder under `utils/raydium/`.
// ---------------------------------------------------------------------------

/// `open_position_with_token22_nft(tick_lower_index: i32, tick_upper_index: i32,
///     tick_array_lower_start_index: i32, tick_array_upper_start_index: i32,
///     liquidity: u128, amount_0_max: u64, amount_1_max: u64,
///     with_metadata: bool, base_flag: Option<bool>)`.
pub const RAYDIUM_OPEN_POSITION_WITH_TOKEN22_NFT_DISCRIMINATOR: [u8; 8] =
    [77, 255, 174, 82, 125, 29, 201, 46];

/// `increase_liquidity_v2(liquidity: u128, amount_0_max: u64, amount_1_max: u64,
///     base_flag: Option<bool>)`.
pub const RAYDIUM_INCREASE_LIQUIDITY_V2_DISCRIMINATOR: [u8; 8] =
    [133, 29, 89, 223, 69, 238, 176, 10];

/// `decrease_liquidity_v2(liquidity: u128, amount_0_min: u64, amount_1_min: u64)`.
/// Also pays out every fee and reward owed to the position.
pub const RAYDIUM_DECREASE_LIQUIDITY_V2_DISCRIMINATOR: [u8; 8] =
    [58, 127, 188, 62, 79, 82, 196, 96];

/// `close_position()` — burns the position NFT and closes the NFT account,
/// the NFT mint and `PersonalPositionState`; all rent goes to `nft_owner`.
pub const RAYDIUM_CLOSE_POSITION_DISCRIMINATOR: [u8; 8] = [123, 134, 81, 0, 49, 68, 98, 98];

// ---------------------------------------------------------------------------
// Raydium CLMM account discriminators (sha256("account:<Name>")[0..8])
// ---------------------------------------------------------------------------

/// `PersonalPositionState` — the per-NFT position account.
pub const RAYDIUM_PERSONAL_POSITION_DISCRIMINATOR: [u8; 8] = [70, 111, 150, 126, 230, 15, 25, 117];

/// `TickArrayState`.
pub const RAYDIUM_TICK_ARRAY_DISCRIMINATOR: [u8; 8] = [192, 155, 85, 205, 49, 249, 129, 42];

/// `TickArrayBitmapExtension`.
pub const RAYDIUM_TICK_ARRAY_BITMAP_EXTENSION_DISCRIMINATOR: [u8; 8] =
    [60, 150, 36, 219, 97, 128, 139, 153];

// ---------------------------------------------------------------------------
// Raydium CLMM PDA seeds
// ---------------------------------------------------------------------------

/// `PersonalPositionState` PDA: `[b"position", nft_mint]`.
pub const RAYDIUM_POSITION_SEED: &[u8] = b"position";

/// `TickArrayState` PDA: `[b"tick_array", pool_state, start_tick_index.to_be_bytes()]`.
/// The third seed is the **big-endian** i32 — not Orca's ASCII string.
pub const RAYDIUM_TICK_ARRAY_SEED: &[u8] = b"tick_array";

/// `TickArrayBitmapExtension` PDA: `[b"pool_tick_array_bitmap_extension", pool_state]`.
/// Created together with every pool.
pub const RAYDIUM_TICK_ARRAY_BITMAP_EXTENSION_SEED: &[u8] = b"pool_tick_array_bitmap_extension";

// ---------------------------------------------------------------------------
// Raydium CLMM layout constants
// ---------------------------------------------------------------------------

/// Number of ticks packed inside a single Raydium `TickArrayState`.
pub const RAYDIUM_TICK_ARRAY_SIZE: i32 = 60;

/// Reward slots in a Raydium pool / position.
pub const RAYDIUM_REWARD_SLOTS: usize = 3;

/// Accounts appended to `decrease_liquidity_v2` per initialized reward slot:
/// `[reward_vault, recipient, reward_mint]`.
pub const RAYDIUM_REWARD_ACCOUNTS_PER_SLOT: usize = 3;

/// `protocol_position` slot value forwarded to open / increase / decrease.
///
/// The account is deprecated on the deployed program: it is an
/// `UncheckedAccount` with no seeds, no `mut` and no reads, so any key is
/// accepted. We forward the CLMM program id because it is already part of
/// every Raydium call (no extra transaction account), it is read-only and it
/// can never alias one of the writable accounts in the same CPI.
pub const RAYDIUM_PROTOCOL_POSITION_PLACEHOLDER: Pubkey = RAYDIUM_CLMM_PROGRAM_ID;

// `PoolState` absolute offsets (the 8-byte discriminator is bytes 0..8).
// The account is `#[repr(C, packed)]`, so there is no alignment padding.

/// `PoolState.token_mint_0` (absolute; same value as `RAYDIUM_MINT_A_OFFSET`).
pub const RAYDIUM_POOL_TOKEN_MINT_0_OFFSET: usize = 73;
/// `PoolState.token_mint_1` (absolute; same value as `RAYDIUM_MINT_B_OFFSET`).
pub const RAYDIUM_POOL_TOKEN_MINT_1_OFFSET: usize = 105;
/// `PoolState.token_vault_0`.
pub const RAYDIUM_POOL_TOKEN_VAULT_0_OFFSET: usize = 137;
/// `PoolState.token_vault_1`.
pub const RAYDIUM_POOL_TOKEN_VAULT_1_OFFSET: usize = 169;
/// `PoolState.tick_spacing` (u16 LE).
pub const RAYDIUM_POOL_TICK_SPACING_OFFSET: usize = 235;
/// Start of `PoolState.reward_infos: [RewardInfo; 3]`.
pub const RAYDIUM_POOL_REWARD_INFOS_OFFSET: usize = 397;
/// Size of one packed `RewardInfo`.
pub const RAYDIUM_POOL_REWARD_INFO_STRIDE: usize = 169;
/// `RewardInfo.token_mint`, relative to the start of the slot. A slot is
/// initialized iff this key is not `Pubkey::default()`.
pub const RAYDIUM_REWARD_INFO_TOKEN_MINT_RELATIVE_OFFSET: usize = 57;
/// `RewardInfo.token_vault`, relative to the start of the slot.
pub const RAYDIUM_REWARD_INFO_TOKEN_VAULT_RELATIVE_OFFSET: usize = 89;

// `PersonalPositionState` absolute offsets (Borsh, fixed size).

/// Full `PersonalPositionState` account size.
pub const RAYDIUM_PERSONAL_POSITION_LEN: usize = 281;
/// `PersonalPositionState.nft_mint`.
pub const RAYDIUM_PERSONAL_POSITION_NFT_MINT_OFFSET: usize = 9;
/// `PersonalPositionState.pool_id`.
pub const RAYDIUM_PERSONAL_POSITION_POOL_ID_OFFSET: usize = 41;
/// `PersonalPositionState.tick_lower_index` (i32 LE).
pub const RAYDIUM_PERSONAL_POSITION_TICK_LOWER_OFFSET: usize = 73;
/// `PersonalPositionState.tick_upper_index` (i32 LE).
pub const RAYDIUM_PERSONAL_POSITION_TICK_UPPER_OFFSET: usize = 77;
/// `PersonalPositionState.liquidity` (u128 LE).
pub const RAYDIUM_PERSONAL_POSITION_LIQUIDITY_OFFSET: usize = 81;
/// `PersonalPositionState.token_fees_owed_0` (u64 LE).
pub const RAYDIUM_PERSONAL_POSITION_FEES_OWED_0_OFFSET: usize = 129;
/// `PersonalPositionState.token_fees_owed_1` (u64 LE).
pub const RAYDIUM_PERSONAL_POSITION_FEES_OWED_1_OFFSET: usize = 137;
/// `PersonalPositionState.reward_infos[i].reward_amount_owed` (u64 LE), per slot.
pub const RAYDIUM_PERSONAL_POSITION_REWARD_OWED_OFFSETS: [usize; RAYDIUM_REWARD_SLOTS] =
    [161, 185, 209];

// ---------------------------------------------------------------------------
// Meteora bin layout / DLMM constants
// ---------------------------------------------------------------------------

/// Number of bins packed inside a single Meteora `BinArray` PDA.
pub const METEORA_BINS_PER_ARRAY: i64 = 70;

/// Hard upper bound on the per-call `active_id_slippage` accepted by
/// `IncreaseLiquidity` (and `manual_rebalance`). Meteora's SDK default is 3
/// bins; the cap stays generous enough to absorb high-volatility rebalances
/// while preventing a buggy / hostile caller from disabling slippage
/// protection entirely (e.g. by passing `i32::MAX`).
pub const MAX_ACTIVE_BIN_SLIPPAGE_HARD_CAP: u16 = 50;

/// Largest absolute `bin_id` reachable by Meteora's inline `LbPair` bitmap
/// (~78 BinArray indices × `METEORA_BINS_PER_ARRAY`). Anything outside
/// `[-LIMIT, LIMIT]` requires the separate `bin_array_bitmap_extension`
/// PDA, which is not wired in this program. Callers reject such ranges
/// upfront with `BitmapExtensionRequired` to fail fast instead of letting
/// the CPI return a less actionable error.
pub const METEORA_INLINE_BITMAP_BIN_LIMIT: i32 = 5460;

// ---------------------------------------------------------------------------
// External program IDs used as account placeholders for Meteora CPIs
// ---------------------------------------------------------------------------

/// SPL Memo program — passed as `memo_program` account in `claim_fee2` and
/// `remove_liquidity_by_range2`. Meteora forwards a memo when fees are paid.
pub const MEMO_PROGRAM_ID: Pubkey = pubkey!("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");

// ---------------------------------------------------------------------------
// PDA seeds
// ---------------------------------------------------------------------------

/// Seed of the protocol treasury PDA (`[b"treasury", protocol_config]`).
/// Centralised so every consumer derives the same address; never inline the
/// literal in account constraints.
pub const TREASURY_SEED: &[u8] = b"treasury";

// ---------------------------------------------------------------------------
// Meteora `rebalance_liquidity` invariants
// ---------------------------------------------------------------------------

/// The Aargau vault collects fees explicitly via `claim_fee2` and routes the
/// performance split to the treasury BEFORE calling `rebalance_liquidity`.
/// Passing `should_claim_fee = true` to Meteora would double-credit the user
/// with the fees we already extracted, so this constant pins the flag to
/// `false` at every CPI call site.
pub const METEORA_REBALANCE_SHOULD_CLAIM_FEE: bool = false;

// ---------------------------------------------------------------------------
// Operational limits
// ---------------------------------------------------------------------------

/// Maximum bins per Meteora position — validated upfront, never silent fail
pub const MAX_METEORA_BINS: i32 = 69;

/// Pending rebalance timeout in seconds (4 minutes)
pub const PENDING_REBALANCE_TIMEOUT_SECS: i64 = 240;

/// Basis points denominator
pub const BPS_DIVISOR: u64 = 10_000;

/// Basis points denominator as u16 — used for pct_bps boundary checks
pub const BPS_DIVISOR_U16: u16 = 10_000;

// ---------------------------------------------------------------------------
// Program version
// ---------------------------------------------------------------------------

/// On-chain program version stored in ProtocolConfig.
/// Encoded as major * 10_000 + minor * 100 + patch.
/// Examples: v0.1.0 → 100, v1.0.0 → 10_000, v1.2.3 → 10_203.
pub const PROGRAM_VERSION: u32 = 100; // v0.1.0
