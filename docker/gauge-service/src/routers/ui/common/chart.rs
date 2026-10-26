//! Server-rendered SVG charts: no script, themed by CSS custom properties so they follow light/dark.

use quench_web::prelude::*;

const W: f64 = 300.0;
const H: f64 = 64.0;

/// One series, `None` where the agent reported nothing (a gap). `class` is `gg-s1` or `gg-s2`.
#[derive(Debug)]
pub struct Series<'a> {
    pub class: &'a str,
    pub values: &'a [Option<f64>],
}

fn points(values: &[Option<f64>], max: f64, slots: usize) -> Vec<String> {
    // A line per unbroken run, so a gap is drawn as a gap.
    let step = W / (slots.max(2) - 1) as f64;
    let offset = slots.saturating_sub(values.len());
    let mut runs: Vec<Vec<String>> = vec![Vec::new()];
    for (i, v) in values.iter().enumerate() {
        match v {
            Some(v) => {
                let x = (offset + i) as f64 * step;
                let y = H - (v / max).clamp(0.0, 1.0) * (H - 2.0) - 1.0;
                runs.last_mut()
                    .expect("one run")
                    .push(format!("{x:.1},{y:.1}"));
            }
            None => runs.push(Vec::new()),
        }
    }
    runs.into_iter()
        .filter(|r| r.len() > 1)
        .map(|r| r.join(" "))
        .collect()
}

/// A line chart of `slots` evenly spaced samples (the newest at the right edge), scaled 0..`max`.
#[must_use]
pub fn line(series: &[Series<'_>], max: f64, slots: usize, label: &str) -> Element {
    let max = if max > 0.0 { max } else { 1.0 };
    let mut svg = element("svg")
        .class("gg-chart")
        .attr("viewBox", format!("0 0 {W} {H}"))
        .attr("preserveAspectRatio", "none")
        .attr("role", "img")
        .attr("aria-label", label.to_string());
    for q in [0.25, 0.5, 0.75] {
        let y = H * q;
        svg = svg.child(
            element("line")
                .class("gg-grid")
                .attr("x1", "0")
                .attr("x2", format!("{W}"))
                .attr("y1", format!("{y:.1}"))
                .attr("y2", format!("{y:.1}")),
        );
    }
    for (n, s) in series.iter().enumerate() {
        for run in points(s.values, max, slots) {
            if n == 0 {
                let first = run
                    .split(' ')
                    .next()
                    .unwrap_or("0,0")
                    .split(',')
                    .next()
                    .unwrap_or("0");
                let last = run
                    .rsplit(' ')
                    .next()
                    .unwrap_or("0,0")
                    .split(',')
                    .next()
                    .unwrap_or("0");
                svg = svg.child(
                    element("polygon")
                        .class(format!("gg-area {}", s.class))
                        .attr("points", format!("{first},{H} {run} {last},{H}")),
                );
            }
            svg = svg.child(
                element("polyline")
                    .class(format!("gg-line {}", s.class))
                    .attr("points", run),
            );
        }
    }
    div().class("gg-chart-box").child(svg)
}
