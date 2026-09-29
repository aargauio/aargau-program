//! Test fixtures — pinned binaries of external Solana programs used by the
//! integration test binary.
//!
//! Binaries are **not** checked into the public repo; each contributor (or CI)
//! must dump them locally from a trusted source before running fixture-backed
//! tests. The fixtures themselves are loaded lazily so tests that do not
//! touch a given protocol stay green even when the matching `.so` is absent.
//!
//! ## How to produce the Meteora DLMM fixture
//!
//! ```bash
//! solana program dump \
//!     LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo \
//!     programs/aargau-manager/tests/fixtures/meteora_dlmm.so \
//!     --url mainnet-beta
//! ```
//!
//! Pin the resulting file with `sha256sum` in a comment next to any test that
//! depends on a specific ABI version, so an upstream redeploy that breaks the
//! ABI fails loudly instead of silently picking up new behaviour.
//!
//! ## Raydium CLMM replay fixtures
//!
//! The Raydium replay loads the CLMM program plus the SPL programs its CPIs
//! call into (SPL Token, Token-2022, Associated Token Account, Memo). They
//! are dumped over plain JSON-RPC, so no Solana CLI is needed, and each
//! loader checks the file against a pinned hash:
//!
//! ```bash
//! sh programs/aargau-manager/tests/fixtures/dump-raydium-fixtures.sh programs
//! ```
//!
//! What the script does for an upgradeable program (the Raydium example; the
//! ProgramData address comes from the program account, `jsonParsed`):
//!
//! ```bash
//! RPC=https://api.mainnet-beta.solana.com
//! curl -s $RPC -H 'content-type: application/json' \
//!   -d '{"jsonrpc":"2.0","id":1,"method":"getAccountInfo","params":["HzD2cCXXT3UQNjMMY6kDv9w6gZ9qquSdfoGXrLL3LXx",{"encoding":"base64"}]}' \
//!   | jq -r '.result.value.data[0]' | base64 -d > programdata.bin
//! od -An -tu8 -j4 -N8 programdata.bin          # deploy slot (LE u64, bytes 4..12)
//! tail -c +46 programdata.bin > raydium_clmm.so # skip the 45-byte header
//! ```
//!
//! Loader-v2 programs (ATA, Memo) are immutable: their account data is the
//! ELF, with no header. The pin is the sha256 of the ELF with trailing zero
//! bytes stripped, because ProgramData is zero-padded past the ELF and the
//! padding changes with every resize:
//!
//! ```bash
//! n=$(od -An -v -tx1 raydium_clmm.so | tr -s ' ' '\n' | grep -v '^$' \
//!     | grep -n -v '^00$' | tail -n 1 | cut -d: -f1)
//! head -c "$n" raydium_clmm.so | sha256sum   # compare with the pin below
//! ```
//!
//! A mismatch means the program was upgraded upstream: the replay's ABI
//! assumptions must be re-checked before the pin is moved.
//!
//! Captured at mainnet slot ~451690500 (loader v3 = upgradeable, v2 = immutable):
//!
//! | File | Program | Loader | Deploy slot | Raw sha256 (padding included) |
//! |---|---|---|---|---|
//! | `raydium_clmm.so` | `CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK` | v3 | 451479902 | `b1e8b3eb1c0027b3c05b059dec773661cbe93e2b304c1166e575b502ee2ca6d2` |
//! | `spl_token.so` | `TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA` | v3 | 419472000 | `8190d3f7ceb6cb7a7a8d8924bff89f9f611e15ce1f806f2b6237f3311a98f697` |
//! | `spl_token_2022.so` | `TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb` | v3 | 427147035 | `0999dbf708971e723b08d1caafc988826a59c6001ed6dc02260da07defbe1469` |
//! | `spl_associated_token_account.so` | `ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL` | v2 | n/a | `6804554e69fd3a58caa191dc4a58f4c67223d30ca28ab8987f39fc18d2f7374d` |
//! | `spl_memo.so` | `MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr` | v2 | n/a | `f520eaf096361abbb9639ea4dc3e5388a87b9330e121f476607b87c46ef67954` |
//!
//! The mainnet account snapshots are committed; see [`raydium`].

// Loader helpers below are consumed by upcoming mollusk-backed harnesses; until
// then `cargo clippy --all-targets -D warnings` would flag them as dead code.
#![allow(dead_code)]

pub mod account_snapshot;
pub mod raydium;

use std::path::{Path, PathBuf};

use aargau_manager::constants::{
    MEMO_PROGRAM_ID, RAYDIUM_CLMM_PROGRAM_ID, SPL_TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID,
};
use anchor_lang::prelude::Pubkey;
use mollusk_svm::program::loader_keys::{LOADER_V2, LOADER_V3};
use sha2::{Digest, Sha256};

/// File name of the pinned Meteora DLMM program binary inside `tests/fixtures/`.
pub const METEORA_DLMM_FIXTURE_FILENAME: &str = "meteora_dlmm.so";

/// File name of the pinned Orca Whirlpools program binary inside `tests/fixtures/`.
pub const ORCA_WHIRLPOOLS_FIXTURE_FILENAME: &str = "orca_whirlpools.so";

/// Command that re-dumps every hash-pinned program below.
const PINNED_PROGRAMS_DUMP_COMMAND: &str =
    "sh programs/aargau-manager/tests/fixtures/dump-raydium-fixtures.sh programs";

/// A mainnet program ELF dumped into `tests/fixtures/` and pinned by hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinnedProgramFixture {
    pub file_name: &'static str,
    pub program_id: Pubkey,
    /// Loader that owns the program on mainnet; pass it to
    /// `Mollusk::add_program_with_loader_and_elf`.
    pub loader: Pubkey,
    /// Lowercase hex sha256 of the ELF with trailing zero bytes stripped.
    pub stripped_elf_sha256: &'static str,
}

pub const RAYDIUM_CLMM_FIXTURE: PinnedProgramFixture = PinnedProgramFixture {
    file_name: "raydium_clmm.so",
    program_id: RAYDIUM_CLMM_PROGRAM_ID,
    loader: LOADER_V3,
    stripped_elf_sha256: "f78e9dbc080068facc531ab4b679680dd39efb9d07a08716b37d493e99272fc9",
};

pub const SPL_TOKEN_FIXTURE: PinnedProgramFixture = PinnedProgramFixture {
    file_name: "spl_token.so",
    program_id: SPL_TOKEN_PROGRAM_ID,
    loader: LOADER_V3,
    stripped_elf_sha256: "435df1e70f1ca6258eb01a4507701688c7f1a22656e3e4ef361db473bf478075",
};

pub const SPL_TOKEN_2022_FIXTURE: PinnedProgramFixture = PinnedProgramFixture {
    file_name: "spl_token_2022.so",
    program_id: TOKEN_2022_PROGRAM_ID,
    loader: LOADER_V3,
    stripped_elf_sha256: "7ea94a027005b39196fa08e6d0ddcd55ec32eb43266179921a834a1ba2eb1947",
};

pub const ASSOCIATED_TOKEN_ACCOUNT_FIXTURE: PinnedProgramFixture = PinnedProgramFixture {
    file_name: "spl_associated_token_account.so",
    program_id: anchor_spl::associated_token::ID,
    loader: LOADER_V2,
    stripped_elf_sha256: "e93ab1ff110697630b0814bd49980a3b85f8890fa5a574c439eace3120522b5c",
};

pub const SPL_MEMO_FIXTURE: PinnedProgramFixture = PinnedProgramFixture {
    file_name: "spl_memo.so",
    program_id: MEMO_PROGRAM_ID,
    loader: LOADER_V2,
    stripped_elf_sha256: "68d9bbae7023f8f51d32d0f6ff8a7003921c97c99e77921ccd0f51adcf6295db",
};

/// Every program the Raydium replay loads besides `aargau_manager` itself.
pub const RAYDIUM_REPLAY_PROGRAM_FIXTURES: [PinnedProgramFixture; 5] = [
    RAYDIUM_CLMM_FIXTURE,
    SPL_TOKEN_FIXTURE,
    SPL_TOKEN_2022_FIXTURE,
    ASSOCIATED_TOKEN_ACCOUNT_FIXTURE,
    SPL_MEMO_FIXTURE,
];

/// Absolute path to the fixtures directory inside the test binary.
///
/// Resolved via `CARGO_MANIFEST_DIR` so the lookup works regardless of the
/// current working directory the test runner was invoked from.
pub fn fixtures_dir() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest_dir).join("tests").join("fixtures")
}

/// Returns the raw bytes of the pinned Meteora DLMM program.
///
/// Returns `Err` with an instructive message when the file is missing — the
/// caller should `#[ignore]` or `return Ok(())` the test in that case rather
/// than panic, so contributors without the fixture can still run the rest
/// of the suite.
pub fn load_meteora_dlmm_program() -> Result<Vec<u8>, String> {
    let path = fixtures_dir().join(METEORA_DLMM_FIXTURE_FILENAME);
    if !path.exists() {
        return Err(format!(
            "Meteora DLMM fixture not found at {}. \
             Dump it with: \
             `solana program dump LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo {} --url mainnet-beta`",
            path.display(),
            path.display(),
        ));
    }
    std::fs::read(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))
}

/// Returns the raw bytes of the pinned Orca Whirlpools program.
///
/// Like the Meteora loader, returns `Err` with an instructive `solana program
/// dump` command when the file is missing, so contributors without the
/// fixture can still run the rest of the suite.
pub fn load_whirlpools_program() -> Result<Vec<u8>, String> {
    let path = fixtures_dir().join(ORCA_WHIRLPOOLS_FIXTURE_FILENAME);
    if !path.exists() {
        return Err(format!(
            "Orca Whirlpools fixture not found at {}. \
             Dump it with: \
             `solana program dump whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc {} --url mainnet-beta`",
            path.display(),
            path.display(),
        ));
    }
    std::fs::read(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))
}

/// Returns the ELF of the pinned Raydium CLMM program.
pub fn load_raydium_clmm_program() -> Result<Vec<u8>, String> {
    load_pinned_program(&RAYDIUM_CLMM_FIXTURE)
}

/// Returns the ELF of the pinned SPL Token program.
pub fn load_spl_token_program() -> Result<Vec<u8>, String> {
    load_pinned_program(&SPL_TOKEN_FIXTURE)
}

/// Returns the ELF of the pinned Token-2022 program.
pub fn load_spl_token_2022_program() -> Result<Vec<u8>, String> {
    load_pinned_program(&SPL_TOKEN_2022_FIXTURE)
}

/// Returns the ELF of the pinned Associated Token Account program.
pub fn load_associated_token_account_program() -> Result<Vec<u8>, String> {
    load_pinned_program(&ASSOCIATED_TOKEN_ACCOUNT_FIXTURE)
}

/// Returns the ELF of the pinned SPL Memo program.
pub fn load_spl_memo_program() -> Result<Vec<u8>, String> {
    load_pinned_program(&SPL_MEMO_FIXTURE)
}

/// Read a pinned program and check it against its hash.
///
/// Returns `Err` with the dump command when the file is missing, and with
/// both hashes when it no longer matches the pin, so an upstream upgrade
/// fails loudly instead of silently replaying a different ABI.
pub fn load_pinned_program(fixture: &PinnedProgramFixture) -> Result<Vec<u8>, String> {
    let path = fixtures_dir().join(fixture.file_name);
    if !path.exists() {
        return Err(format!(
            "{} fixture not found at {}. Dump it with: `{PINNED_PROGRAMS_DUMP_COMMAND}`",
            fixture.program_id,
            path.display()
        ));
    }
    let elf =
        std::fs::read(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let actual_sha256 = stripped_elf_sha256(&elf);
    if actual_sha256 != fixture.stripped_elf_sha256 {
        return Err(format!(
            "{} does not match its pin: stripped sha256 {actual_sha256}, expected {}. \
             The program was likely upgraded upstream; re-check the replay before moving the pin.",
            path.display(),
            fixture.stripped_elf_sha256
        ));
    }
    Ok(elf)
}

/// Lowercase hex sha256 of `elf` without its trailing zero bytes.
pub fn stripped_elf_sha256(elf: &[u8]) -> String {
    let unpadded_len = elf
        .iter()
        .rposition(|&byte| byte != 0)
        .map_or(0, |last_nonzero| last_nonzero + 1);
    Sha256::digest(&elf[..unpadded_len])
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ELF_MAGIC: &[u8] = b"\x7fELF";

    #[test]
    fn stripped_hash_ignores_trailing_zero_padding_only() {
        let empty_sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert_eq!(stripped_elf_sha256(&[]), empty_sha256);
        assert_eq!(stripped_elf_sha256(&[0, 0]), empty_sha256);
        assert_eq!(
            stripped_elf_sha256(b"abc"),
            stripped_elf_sha256(b"abc\0\0\0")
        );
        assert_ne!(stripped_elf_sha256(b"abc"), stripped_elf_sha256(b"\0abc"));
    }

    #[test]
    fn missing_program_fixture_returns_the_dump_command() {
        let missing = PinnedProgramFixture {
            file_name: "does_not_exist.so",
            ..RAYDIUM_CLMM_FIXTURE
        };
        let error = load_pinned_program(&missing).unwrap_err();
        assert!(error.contains(PINNED_PROGRAMS_DUMP_COMMAND), "{error}");
    }

    #[test]
    #[ignore = "requires the dumped Raydium replay program fixtures"]
    fn raydium_replay_programs_match_their_pins() {
        for fixture in &RAYDIUM_REPLAY_PROGRAM_FIXTURES {
            let elf = load_pinned_program(fixture).unwrap();
            assert!(elf.starts_with(ELF_MAGIC), "{}", fixture.file_name);
        }
    }
}
