//! Line charts as a standalone SVG image.
//!
//! The browser draws the image with `<img src="data:image/svg+xml,...">`, where CSS variables of the page do not reach, so the
//! colours are in the image, with a dark variant that follows the system's colour scheme.

use std::fmt::Write as _;

/// The drawing's size; the image stretches to its box.
pub const W: f64 = 300.0;
pub const H: f64 = 64.0;

/// One series, `None` where the agent reported nothing (a gap).
#[derive(Debug, Clone, Copy)]
pub struct Series<'a> {
    /// `1` to `3`: which colour.
    pub colour: u8,
    pub values: &'a [Option<f64>],
}

/// The runs of a series as `x,y` point lists: a line per unbroken run, so a gap is drawn as a gap.
#[must_use]
pub fn points(values: &[Option<f64>], max: f64, slots: usize) -> Vec<String> {
    let slots = slots.max(2);
    #[allow(clippy::cast_precision_loss)] // a few hundred samples
    let step = W / (slots - 1) as f64;
    let offset = slots.saturating_sub(values.len());
    let mut runs: Vec<Vec<String>> = vec![Vec::new()];
    for (i, v) in values.iter().enumerate() {
        match v {
            Some(v) => {
                #[allow(clippy::cast_precision_loss)]
                let x = (offset + i) as f64 * step;
                let y = H - (v / max).clamp(0.0, 1.0).mul_add(H - 2.0, 0.0) - 1.0;
                if let Some(run) = runs.last_mut() {
                    run.push(format!("{x:.1},{y:.1}"));
                }
            }
            None => runs.push(Vec::new()),
        }
    }
    runs.into_iter()
        .filter(|r| r.len() > 1)
        .map(|r| r.join(" "))
        .collect()
}

const STYLE: &str = "<style>\
.g{stroke:#98a2b3;stroke-opacity:.28;stroke-width:1}\
.l{fill:none;stroke-width:2;stroke-linejoin:round;stroke-linecap:round}\
.l1{stroke:#17b26a}.l2{stroke:#2e90fa}.l3{stroke:#f79009}.a1{fill:#17b26a;fill-opacity:.12}\
</style>";

/// A line chart of `slots` evenly spaced samples (the newest at the right edge), scaled 0..`max`, with a faint area under
/// the first series.
#[must_use]
pub fn line_svg(series: &[Series<'_>], max: f64, slots: usize) -> String {
    let max = if max > 0.0 { max } else { 1.0 };
    let mut svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 {W} {H}' preserveAspectRatio='none'>{STYLE}"
    );
    for q in [0.25, 0.5, 0.75] {
        let y = H * q;
        let _ = write!(
            svg,
            "<line class='g' x1='0' x2='{W}' y1='{y:.1}' y2='{y:.1}' vector-effect='non-scaling-stroke'/>"
        );
    }
    for (n, s) in series.iter().enumerate() {
        for run in points(s.values, max, slots) {
            if n == 0 {
                let edge = |first: bool| {
                    let point = if first {
                        run.split(' ').next()
                    } else {
                        run.rsplit(' ').next()
                    };
                    point
                        .and_then(|p| p.split(',').next())
                        .unwrap_or("0")
                        .to_string()
                };
                let _ = write!(
                    svg,
                    "<polygon class='a1' points='{},{H} {run} {},{H}'/>",
                    edge(true),
                    edge(false)
                );
            }
            let _ = write!(
                svg,
                "<polyline class='l l{}' points='{run}' vector-effect='non-scaling-stroke'/>",
                s.colour.clamp(1, 3)
            );
        }
    }
    svg.push_str("</svg>");
    svg
}

/// The chart as the `src` of an `<img>`.
#[must_use]
pub fn data_uri(svg: &str) -> String {
    let mut url = String::from("data:image/svg+xml,");
    for b in svg.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~/:".contains(&b) {
            url.push(char::from(b));
        } else {
            let _ = write!(url, "%{b:02X}");
        }
    }
    url
}
