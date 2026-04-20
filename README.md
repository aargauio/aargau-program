# Aargau Manager Program

Custodial Anchor/Solana smart contract for vault-based LP position management across Solana DEXs.

## Overview

The `VaultAccount` PDA is the **direct owner** of all LP positions. This is required because Orca/Raydium burn and reissue position NFTs on every rebalance — an SPL delegate would become invalid after each burn.

## Current Status

| Instruction | Status | Notes |
|---|---|---|
| `initialize_protocol` | Implemented | Creates ProtocolConfig singleton |
| `create_vault` | Implemented | 4-layer pool validation, TransferFee placeholder |
| `deposit` | Implemented | Idle only (no active CPI) |
| `withdraw` | Implemented | Proportional + full close |
| `emergency_withdraw` | Implemented | Bypasses is_paused — hard requirement |
| `set_protocol_pause` | Implemented | Admin kill switch |
| `close_vault` | Stub | CPI integration pending |
| `claim_rewards` | Stub | CPI integration pending |
| `manual_rebalance` | Stub | CPI integration pending for all protocols |
| `cancel_pending_rebalance` | Stub | CPI integration pending |
| `retry_pending_rebalance` | Stub | CPI integration pending |
| `update_protocol_config` | Stub | |
| `withdraw_treasury` | Stub | |
| `transfer_admin` | Stub | |
| `admin_emergency_transfer` | Stub | Destination hard-coded to vault.user_authority |
| `execute_action` | Stub | Dual-authority keeper; reserved for future use |
| `report_rebalance_attempt` | Stub | Dual-authority keeper; reserved for future use |

## Build

No Solana/Anchor CLI required for Rust unit tests:

```bash
cargo test --manifest-path programs/aargau-manager/Cargo.toml
```

To build the SBF binary (requires Solana toolchain):

```bash
cargo build-sbf --manifest-path programs/aargau-manager/Cargo.toml
```

To build and generate IDL (requires Anchor CLI):

```bash
anchor build
```

## Architecture

```
programs/aargau-manager/src/
├── lib.rs              # declare_id!, #[program] dispatch
├── errors.rs           # AargauError enum (starts at 1000)
├── constants.rs        # Program IDs, discriminators, offsets
├── events.rs           # 26 #[event] structs
├── state/
│   ├── vault_account.rs    # VaultAccount (316 bytes), enums, PendingRebalance
│   ├── protocol_config.rs  # ProtocolConfig (112 bytes) singleton
│   └── user_config.rs      # UserConfig (reserved for future use)
├── instructions/
│   ├── admin/          # initialize_protocol, set_protocol_pause, update_protocol_config,
│   │                   # withdraw_treasury, transfer_admin, admin_emergency_transfer
│   ├── vault/          # create_vault, deposit, withdraw, emergency_withdraw, close_vault,
│   │                   # claim_rewards, manual_rebalance, cancel_pending_rebalance,
│   │                   # retry_pending_rebalance
│   └── keeper/         # execute_action, report_rebalance_attempt
└── utils/
    ├── fee.rs              # calc_aargau_fee, calc_net_after_transfer_fee
    ├── pool_validation.rs  # 4-layer pool validation
    └── signer_seeds.rs     # vault_signer_seeds helper
```

## Account Sizes

| Account | LEN | Data |
|---|---|---|
| `VaultAccount` | 316 | 8 disc + 308 data |
| `ProtocolConfig` | 112 | 8 disc + 104 data |
| `PendingRebalance` | — | 18 bytes (embedded) |
| `AutoRebalanceStrategy` | — | 46 bytes (embedded) |

## Versioning

`program_version` is stored in `ProtocolConfig` on-chain, encoded as `major * 10_000 + minor * 100 + patch`.

| Value | Version |
|---|---|
| `100` | v0.1.0 |
| `10_000` | v1.0.0 |
| `10_203` | v1.2.3 |

Git tag convention:

```
v0.1.0-localnet   — first localnet deploy
v0.1.0-devnet     — devnet deploy
v1.0.0-audit      — version submitted for audit
v1.0.0-mainnet    — mainnet deploy (upgrade authority renounced after this tag)
```

## Security Requirements

- `emergency_withdraw` never checks `is_paused` — unconditional
- `admin_emergency_transfer` destination is always `vault.user_authority`
- All arithmetic uses `checked_*` — no direct operators on u64/u128
- No `unwrap()` outside `#[cfg(test)]`
- All enums have `#[repr(u8)]` to ensure 1-byte Borsh serialization
