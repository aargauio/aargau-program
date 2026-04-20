use crate::{constants::BPS_DIVISOR, errors::AargauError};

/// Calculate the Aargau performance fee on collected LP fees.
///
/// fee = gross × fee_rate_bps / BPS_DIVISOR
///
/// All arithmetic uses checked operations — no overflow possible.
pub fn calc_aargau_fee(gross: u64, fee_rate_bps: u16) -> Result<u64, AargauError> {
    let fee = (gross as u128)
        .checked_mul(fee_rate_bps as u128)
        .ok_or(AargauError::Overflow)?
        .checked_div(BPS_DIVISOR as u128)
        .ok_or(AargauError::DivisionByZero)? as u64;
    Ok(fee)
}

/// Calculate the net amount received after a Token-2022 TransferFee deduction.
///
/// The transfer fee is applied by the token program on transfer, so the treasury
/// receives slightly less than `amount`. Aargau absorbs this difference.
///
/// net = amount − min(ceil(amount × bps / BPS_DIVISOR), max_fee)
pub fn calc_net_after_transfer_fee(amount: u64, bps: u16, max_fee: u64) -> u64 {
    if bps == 0 {
        return amount;
    }
    // Round up to be conservative: add (BPS_DIVISOR - 1) before integer division.
    // Clamp to u64::MAX before narrowing: bps can be up to 65535 (> BPS_DIVISOR),
    // so raw_fee could exceed u64::MAX before the min(max_fee) is applied.
    let raw_fee = ((amount as u128)
        .saturating_mul(bps as u128)
        .saturating_add(BPS_DIVISOR as u128 - 1))
        / BPS_DIVISOR as u128;
    let fee = (raw_fee.min(u64::MAX as u128) as u64).min(max_fee);
    amount.saturating_sub(fee)
}
