use rust_decimal::Decimal;
use rust_decimal::prelude::*;

pub const SATS_PER_BTC: u128 = 100_000_000;
pub const MICRO_USDC: u128 = 1_000_000;

pub fn btc_to_sats_str(s: &str) -> Result<u64, String> {
    let dec = Decimal::from_str(s).map_err(|e| e.to_string())?;
    let sats = dec
                .checked_mul(Decimal::from_u128(SATS_PER_BTC).unwrap())
                .ok_or("overflow converting BTC to sats")?;

    if !sats.fract().is_zero() {
        return Err("BTC amount has more than 8 decimals".into());
    }

    Ok(sats.to_u64().ok_or("overflow converting sats to u64")?)
}

pub fn usdc_to_micro_usdc(s: &str) -> Result<u64, String> {
    let dec = Decimal::from_str(s).map_err(|e| e.to_string())?;
    let micro = dec
                 .checked_mul(Decimal::from_u128(MICRO_USDC).unwrap())
                 .ok_or("overflow converting to micro usdc")?;

    if !micro.fract().is_zero() {
        return Err("USDC amount has more than  6 decimals".into());
    }

    Ok(micro.to_u64().ok_or("overflow converting micro to u64")?)
}

pub fn price_to_micro_str(s: &str) -> Result<u64, String> {
    let dec = Decimal::from_str(s).map_err(|e| e.to_string())?;

    let micro = dec
        .checked_mul(Decimal::from(MICRO_USDC))
        .ok_or("overflow converting price to micro")?;

    if !micro.fract().is_zero() {
        return Err("price has more than 6 decimals".into());
    }

    Ok(micro.to_u64().ok_or("overflow converting price to u64")?)
}
