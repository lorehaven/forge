//! When a filesystem fills: a straight line through its recent used-bytes samples.
//!
//! Crude on purpose. A disk that grows steadily gives a believable date; one that fills in bursts (builds, model
//! downloads) gives a rough one, and one that shrinks or stays level gives none.

/// Samples needed before a line means anything (hourly samples: a day's worth).
pub const MIN_SAMPLES: usize = 24;
/// The samples must span at least this long, in seconds.
pub const MIN_SPAN_SECS: f64 = 12.0 * 3600.0;
/// A forecast further out than this is not worth a number.
pub const MAX_DAYS: f64 = 3650.0;
/// Growth under this, in bytes a day, is level.
const LEVEL: f64 = 1_048_576.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fit {
    /// Bytes added a day (negative when the disk is emptying).
    pub per_day: f64,
    /// Days until `size` is reached at that rate; `None` when it is not growing or is further off than [`MAX_DAYS`].
    pub days_left: Option<f64>,
}

/// The line through `samples` (unix seconds, used bytes), and where it meets `size`. `None` with too little data.
#[must_use]
pub fn fit(samples: &[(f64, f64)], size: f64) -> Option<Fit> {
    let (first, last) = (samples.first()?, samples.last()?);
    if samples.len() < MIN_SAMPLES || last.0 - first.0 < MIN_SPAN_SECS {
        return None;
    }
    #[allow(clippy::cast_precision_loss)] // a few hundred samples
    let n = samples.len() as f64;
    let (mean_t, mean_u) = samples
        .iter()
        .fold((0.0, 0.0), |(t, u), s| (t + s.0 / n, u + s.1 / n));
    let (mut num, mut den) = (0.0, 0.0);
    for (t, u) in samples {
        num += (t - mean_t) * (u - mean_u);
        den += (t - mean_t) * (t - mean_t);
    }
    if den <= 0.0 {
        return None;
    }
    let per_day = num / den * 86_400.0;
    let used = last.1;
    let days_left = (per_day > LEVEL && used < size)
        .then(|| (size - used) / per_day)
        .filter(|d| *d <= MAX_DAYS);
    Some(Fit { per_day, days_left })
}
