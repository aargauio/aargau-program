[![License: Apache-2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)

# aargau-manager

Custodial Anchor program on Solana for managing concentrated-liquidity LP positions across multiple DEXs through a vault PDA.

The `VaultAccount` PDA is the **direct owner** of every LP position it manages. Direct ownership (rather than SPL `approve` delegation) is required because Orca Whirlpools and Raydium CLMM burn and reissue the position NFT on every rebalance — a delegate would be invalidated after each burn.

## Supported protocols (CPI targets)

| Protocol | Position model | Status |
|---|---|---|
| Meteora DLMM | Bin-based, single-tx atomic rebalance | Position lifecycle + atomic rebalance implemented; replay against the pinned `.so` pending |
| Orca Whirlpools | Tick-based, Token-2022 NFT-owned position, two-phase rebalance | Position lifecycle + two-phase rebalance implemented; replay against the pinned `.so` pending |
| Raydium CLMM | Tick-based, Token-2022 NFT-owned position, two-phase rebalance | Position lifecycle + two-phase rebalance implemented; replay against the pinned `.so` pending |

## Instructions

| Category | Instruction | Status |
|---|---|---|
| Admin | `initialize_protocol` | Implemented |
| Admin | `set_protocol_pause` | Implemented |
| Admin | `update_protocol_config` | Stub |
| Admin | `withdraw_treasury` | Stub |
| Admin | `transfer_admin` | Stub |
| Admin | `admin_emergency_transfer` | Stub |
| Vault | `create_vault` | Implemented (4-layer pool validation) |
| Vault | `deposit` | Implemented |
| Vault | `withdraw` | Implemented (proportional, slippage-protected) |
| Vault | `emergency_withdraw` | Implemented (bypasses `is_paused`) |
| Vault | `close_vault` | Stub (CPI pending) |
| Vault | `claim_rewards` | Stub (CPI pending) |
| Vault | `manual_rebalance` | Implemented (Meteora atomic rebalance; verification gate) |
| Vault | `start_rebalance_orca` | Implemented (Orca rebalance phase one) |
| Vault | `retry_pending_rebalance` | Implemented (Orca rebalance phase two) |
| Vault | `start_rebalance_raydium` | Implemented (Raydium rebalance phase one; verification gate) |
| Vault | `retry_pending_rebalance_raydium` | Implemented (Raydium rebalance phase two; verification gate) |
| Vault | `cancel_pending_rebalance` | Implemented (no pause gate) |
| Keeper | `execute_action` | Implemented (Meteora position lifecycle; user-signed) |
| Keeper | `execute_action_orca` | Implemented (Orca position lifecycle; user-signed) |
| Keeper | `execute_action_raydium` | Implemented (Raydium position lifecycle; user-signed; verification gate) |
| Keeper | `report_rebalance_attempt` | Stub |

## Build and test

Run the Rust unit/integration tests (no Solana CLI required):

```bash
cargo test --manifest-path programs/aargau-manager/Cargo.toml
```

Build the SBF binary (requires the Solana toolchain):

```bash
cargo build-sbf --manifest-path programs/aargau-manager/Cargo.toml
```

Build with Anchor (generates the IDL):

```bash
anchor build
```

## Source layout

```
programs/aargau-manager/src/
├── lib.rs              # declare_id!, #[program] dispatch
├── errors.rs           # AargauError enum (codes start at 1000)
├── constants.rs        # Program IDs, CPI discriminators, byte offsets
├── events.rs           # #[event] structs
├── state/
│   ├── vault_account.rs    # VaultAccount + enums + PendingRebalance + AutoRebalanceStrategy
│   ├── protocol_config.rs  # ProtocolConfig singleton
│   └── user_config.rs      # reserved
├── instructions/
│   ├── admin/          # initialize_protocol, set_protocol_pause, update_protocol_config,
│   │                   # withdraw_treasury, transfer_admin, admin_emergency_transfer
│   ├── vault/          # create_vault, deposit, withdraw, emergency_withdraw, close_vault,
│   │                   # claim_rewards, manual_rebalance, start_rebalance_orca,
│   │                   # retry_pending_rebalance, start_rebalance_raydium,
│   │                   # retry_pending_rebalance_raydium, cancel_pending_rebalance
│   └── keeper/         # execute_action, execute_action_orca, execute_action_raydium,
│                       # report_rebalance_attempt
└── utils/
    ├── fee.rs              # calc_aargau_fee, calc_net_after_transfer_fee
    ├── pool_validation.rs  # 4-layer pool validation (owner, discriminator, mints, extensions)
    ├── signer_seeds.rs     # vault PDA signer-seed helper
    ├── token_2022.rs       # Token-2022 mint inspection (extensions, transfer fees)
    ├── token_account.rs    # bounds-checked SPL / Token-2022 token-account reader
    ├── vault_ops.rs        # performance-fee transfer, reward-owner binding, lamport sweep
    ├── meteora/            # Meteora DLMM account views + raw CPI builders
    ├── orca/               # Orca Whirlpools account views + raw CPI builders
    └── raydium/            # Raydium CLMM account views + raw CPI builders
```

## Account sizes

| Account | LEN | Notes |
|---|---|---|
| `VaultAccount` | 316 | 8 disc + 308 data (max with all `Option<T>` = `Some`) |
| `ProtocolConfig` | 112 | 8 disc + 104 data |
| `PendingRebalance` | 18 | embedded |
| `AutoRebalanceStrategy` | 46 | embedded |

## PDA seeds

```
VaultAccount     [b"vault", user_authority, pool_address]
ProtocolConfig   [b"protocol_config"]
UserConfig       [b"user_config", user_authority]
TreasuryPda      [b"treasury", protocol_config]
```

## Versioning

`program_version` is stored in `ProtocolConfig` on-chain, encoded as `major * 10_000 + minor * 100 + patch` (e.g. `10_203` = `v1.2.3`).

## Security invariants

- `emergency_withdraw` does not check `is_paused` and does not load `protocol_config`.
- `admin_emergency_transfer` always sends funds to `vault.user_authority`.
- All arithmetic uses `checked_*`; no direct operators on `u64`/`u128`.
- No `unwrap()` outside `#[cfg(test)]`.
- All enums are `#[repr(u8)]` to guarantee 1-byte Borsh serialization.
- All external program accounts are validated via the 4-layer check (owner, discriminator, embedded mints, mint extensions).

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
