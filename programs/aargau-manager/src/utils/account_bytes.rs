//! Bounds-checked little-endian readers over raw account data.
//!
//! Every external account the program parses by hand (protocol pool and
//! position accounts, token accounts) goes through these helpers so an
//! offset past the end of the buffer becomes the caller's error instead of a
//! panic. The caller picks the error so each parser keeps its own specific
//! variant.

use anchor_lang::prelude::*;

use crate::errors::AargauError;

/// Copy `N` bytes starting at `offset`. Fails with `error` when the range
/// overflows `usize` or runs past the end of `data`.
pub fn read_bytes<const N: usize>(
    data: &[u8],
    offset: usize,
    error: AargauError,
) -> Result<[u8; N]> {
    let end = offset.checked_add(N).ok_or(error)?;
    let slice = data.get(offset..end).ok_or(error)?;
    slice.try_into().map_err(|_| error!(error))
}

pub fn read_pubkey(data: &[u8], offset: usize, error: AargauError) -> Result<Pubkey> {
    read_bytes::<32>(data, offset, error).map(Pubkey::from)
}

pub fn read_u16_le(data: &[u8], offset: usize, error: AargauError) -> Result<u16> {
    read_bytes::<2>(data, offset, error).map(u16::from_le_bytes)
}

pub fn read_i32_le(data: &[u8], offset: usize, error: AargauError) -> Result<i32> {
    read_bytes::<4>(data, offset, error).map(i32::from_le_bytes)
}

pub fn read_u64_le(data: &[u8], offset: usize, error: AargauError) -> Result<u64> {
    read_bytes::<8>(data, offset, error).map(u64::from_le_bytes)
}

pub fn read_u128_le(data: &[u8], offset: usize, error: AargauError) -> Result<u128> {
    read_bytes::<16>(data, offset, error).map(u128::from_le_bytes)
}
