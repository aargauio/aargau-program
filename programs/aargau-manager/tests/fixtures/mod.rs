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

// Loader helpers below are consumed by upcoming mollusk-backed harnesses; until
// then `cargo clippy --all-targets -D warnings` would flag them as dead code.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// File name of the pinned Meteora DLMM program binary inside `tests/fixtures/`.
pub const METEORA_DLMM_FIXTURE_FILENAME: &str = "meteora_dlmm.so";

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
