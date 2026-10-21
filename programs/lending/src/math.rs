use crate::errors::LendingError;
use anchor_lang::prelude::*;

pub const WAD: u128 = 1_000_000_000_000_000_000;
pub const BPS_SCALE: u128 = 10_000;
pub const MICRO_USD: u128 = 1_000_000;
pub const SLOTS_PER_YEAR: u128 = 78_840_000; // ~400ms slot time

/// Calculates utilization rate in BPS (0..=10_000)
pub fn utilization_rate_bps(available_liquidity: u64, borrowed_liquidity_wad: u128) -> Result<u64> {
    let borrowed = borrowed_liquidity_wad
        .checked_div(WAD)
        .ok_or(LendingError::MathOverflow)?;
    let total = (available_liquidity as u128)
        .checked_add(borrowed)
        .ok_or(LendingError::MathOverflow)?;
    if total == 0 {
        return Ok(0);
    }
    let util = borrowed
        .checked_mul(BPS_SCALE)
        .ok_or(LendingError::MathOverflow)?
        .checked_div(total)
        .ok_or(LendingError::MathOverflow)?;
    Ok(util.min(BPS_SCALE) as u64)
}

/// Computes updated cumulative borrow index in WAD based on elapsed slots
pub fn compound_borrow_index(
    current_index_wad: u128,
    base_rate_bps: u16,
    slope_bps: u16,
    utilization_bps: u64,
    slots_elapsed: u64,
) -> Result<u128> {
    if slots_elapsed == 0 {
        return Ok(current_index_wad);
    }
    // annual_rate_bps = base_rate + (utilization * slope / 10_000)
    let variable_bps = (utilization_bps as u128)
        .checked_mul(slope_bps as u128)
        .ok_or(LendingError::MathOverflow)?
        .checked_div(BPS_SCALE)
        .ok_or(LendingError::MathOverflow)?;
    let annual_rate_bps = (base_rate_bps as u128)
        .checked_add(variable_bps)
        .ok_or(LendingError::MathOverflow)?;

    // rate_per_slot_wad = (annual_rate_bps * WAD) / (BPS_SCALE * SLOTS_PER_YEAR)
    let interest_factor_wad = annual_rate_bps
        .checked_mul(WAD)
        .ok_or(LendingError::MathOverflow)?
        .checked_mul(slots_elapsed as u128)
        .ok_or(LendingError::MathOverflow)?
        .checked_div(BPS_SCALE * SLOTS_PER_YEAR)
        .ok_or(LendingError::MathOverflow)?;

    let delta_index = current_index_wad
        .checked_mul(interest_factor_wad)
        .ok_or(LendingError::MathOverflow)?
        .checked_div(WAD)
        .ok_or(LendingError::MathOverflow)?;

    current_index_wad
        .checked_add(delta_index)
        .ok_or_else(|| error!(LendingError::MathOverflow))
}

/// Computes USD value in micro-dollars (10^-6 USD) given token amount, decimals, and micro-USD price.
/// Rounds DOWN (`round_up = false`) for collateral and UP (`round_up = true`) for debt.
pub fn token_usd_value_micro(
    amount: u128,
    decimals: u8,
    price_usd_micro: u64,
    round_up: bool,
) -> Result<u128> {
    let scale = 10u128
        .checked_pow(decimals as u32)
        .ok_or(LendingError::MathOverflow)?;
    let numerator = amount
        .checked_mul(price_usd_micro as u128)
        .ok_or(LendingError::MathOverflow)?;

    if round_up && numerator % scale != 0 {
        Ok(numerator / scale + 1)
    } else {
        Ok(numerator / scale)
    }
}

/// Health factor in BPS. Above 10_000 (1.0x) is healthy; below 10_000 is liquidatable.
/// If debt is 0, returns u64::MAX.
pub fn health_factor_bps(
    collateral_usd_micro: u128,
    liquidation_threshold_bps: u16,
    debt_usd_micro: u128,
) -> Result<u64> {
    if debt_usd_micro == 0 {
        return Ok(u64::MAX);
    }
    let risk_adjusted_collateral = collateral_usd_micro
        .checked_mul(liquidation_threshold_bps as u128)
        .ok_or(LendingError::MathOverflow)?
        .checked_div(BPS_SCALE)
        .ok_or(LendingError::MathOverflow)?;

    let hf = risk_adjusted_collateral
        .checked_mul(BPS_SCALE)
        .ok_or(LendingError::MathOverflow)?
        .checked_div(debt_usd_micro)
        .ok_or(LendingError::MathOverflow)?;

    Ok(hf.min(u64::MAX as u128) as u64)
}

// #[cfg(test)]
// mod proptests {
//     use super::*;
//     use proptest::prelude::*;

//     proptest! {
//         #[test]
//         fn prop_borrow_index_ strictly_monotonic(
//             avail in 1u64..1_000_000_000_000,
//             borrowed in 1u64..1_000_000_000_000,
//             slots in 1u64..10_000_000,
//         ) {
//             let util = utilization_rate_bps(avail, (borrowed as u128) * WAD).unwrap();
//             prop_assert!(util <= 10_000);

//             let next_idx = compound_borrow_index(WAD, 200, 1500, util, slots).unwrap();
//             prop_assert!(next_idx >= WAD);
//         }

//         #[test]
//         fn prop_debt_rounding_favors_protocol(
//             amount in 1u128..1_000_000_000_000,
//             price in 1u64..100_000_000_000,
//         ) {
//             let down = token_usd_value_micro(amount, 9, price, false).unwrap();
//             let up = token_usd_value_micro(amount, 9, price, true).unwrap();
//             prop_assert!(up >= down);
//             prop_assert!(up - down <= 1);
//         }
//     }
// }