//! Number conversions for display and arithmetic that `as` would silently truncate, wrap or round.

/// `n` as an `f64`; exact up to 2^53, the nearest value beyond it. Negative values keep their sign.
#[must_use]
pub fn i64_to_f64(n: i64) -> f64 {
    let magnitude = n.unsigned_abs();
    let high = u32::try_from(magnitude >> 32).unwrap_or(u32::MAX);
    let low = u32::try_from(magnitude & 0xFFFF_FFFF).unwrap_or(u32::MAX);
    let value = f64::from(high) * 4_294_967_296.0 + f64::from(low);
    if n < 0 { -value } else { value }
}

/// `value` rounded to the nearest whole number, as an `i64`; saturates at the ends, and a NaN is `0`.
#[must_use]
pub fn f64_to_i64(value: f64) -> i64 {
    if value.is_nan() {
        return 0;
    }
    // A whole number prints exactly, so parsing it back loses nothing.
    format!("{:.0}", value.round())
        .parse()
        .unwrap_or(if value < 0.0 { i64::MIN } else { i64::MAX })
}
