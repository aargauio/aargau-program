//! Shared helpers for asserting Anchor error codes from test modules.
//!
//! Lifted out of individual test files so the conversion between
//! `anchor_lang::error::Error` and the underlying numeric code lives in a
//! single place.

use aargau_manager::errors::AargauError;

/// Extract the numeric error code from an `anchor_lang::error::Error`,
/// panicking when the variant is a raw `ProgramError` (which would mean the
/// production code returned something other than an `AargauError` and the
/// test should fail loudly).
pub fn err_code(err: &anchor_lang::error::Error) -> u32 {
    match err {
        anchor_lang::error::Error::AnchorError(e) => e.error_code_number,
        anchor_lang::error::Error::ProgramError(_) => panic!("expected AnchorError"),
    }
}

/// Numeric code for an `AargauError` variant — convenience wrapper around the
/// `Into<u32>` impl that the `#[error_code]` macro emits.
pub fn aargau_err_code(variant: AargauError) -> u32 {
    let code: u32 = variant.into();
    code
}
