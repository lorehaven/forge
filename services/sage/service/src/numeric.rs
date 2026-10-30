//! Number conversions that `as` would silently truncate, wrap or round.

/// `n` as an `f64`; exact up to 2^53, the nearest value beyond it.
#[must_use]
pub fn u64_to_f64(n: u64) -> f64 {
    let high = u32::try_from(n >> 32).unwrap_or(u32::MAX);
    let low = u32::try_from(n & 0xFFFF_FFFF).unwrap_or(u32::MAX);
    f64::from(high) * 4_294_967_296.0 + f64::from(low)
}

/// `n` as an `f64`; a count too big for a `u64` saturates there.
#[must_use]
pub fn u128_to_f64(n: u128) -> f64 {
    u64_to_f64(u64::try_from(n).unwrap_or(u64::MAX))
}

/// `n` as an `f64`; exact up to 2^53.
#[must_use]
pub fn usize_to_f64(n: usize) -> f64 {
    u64_to_f64(u64::try_from(n).unwrap_or(u64::MAX))
}

/// The nearest `f32` to `value`, by way of its shortest decimal form (so 0.35 stays 0.35).
#[must_use]
pub fn f64_to_f32(value: f64) -> f32 {
    value.to_string().parse().unwrap_or(0.0)
}
