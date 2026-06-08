use crate::{constants::*, errors::AargauError};
use anchor_lang::prelude::*;

/// Validates that `[lower_bin_id, upper_bin_id]` is non-empty and does not
/// exceed the per-position bin cap defined by `MAX_METEORA_BINS`. This guard
/// prevents unbounded position sizes that would cause CPI transaction size
/// issues across Meteora bin arrays.
pub fn require_bin_count_within_cap(lower_bin_id: i32, upper_bin_id: i32) -> Result<()> {
    let width = upper_bin_id
        .checked_sub(lower_bin_id)
        .and_then(|delta| delta.checked_add(1))
        .ok_or(AargauError::Overflow)?;
    require!(
        width > 0 && width <= MAX_METEORA_BINS,
        AargauError::TooManyBins
    );
    Ok(())
}

/// Rejects bin ranges whose endpoints fall outside the inline `LbPair` bitmap
/// (|bin_id| > `METEORA_INLINE_BITMAP_BIN_LIMIT`). Ranges that cross this
/// boundary require the optional `bin_array_bitmap_extension` PDA, which is
/// not yet wired. Returns `BitmapExtensionRequired`.
pub fn require_range_within_inline_bitmap(lower_bin_id: i32, upper_bin_id: i32) -> Result<()> {
    require!(
        lower_bin_id >= -METEORA_INLINE_BITMAP_BIN_LIMIT
            && upper_bin_id <= METEORA_INLINE_BITMAP_BIN_LIMIT,
        AargauError::BitmapExtensionRequired
    );
    Ok(())
}
