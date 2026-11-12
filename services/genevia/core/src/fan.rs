//! Geometry of the fan chart. Angles are degrees clockwise from straight up; positions are
//! percentages of a square box, so the chart scales with whatever it is drawn in.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sector {
    pub ahn: u32,
    pub generation: u32,
    pub start: f64,
    pub end: f64,
    pub inner: f64,
    pub outer: f64,
}

/// Ahnentafel number to generation: 1 is 0, 2 and 3 are 1, 4..=7 are 2.
#[must_use]
pub const fn generation_of(ahn: u32) -> u32 {
    ahn.ilog2()
}

/// The sector of ancestor `ahn` in a fan of `generations` rings around a hole of radius `hole`
/// (percent of the box). The root fills the hole; every other generation spans `span` degrees,
/// centred on straight up, split evenly among that generation's `2^g` ancestors.
#[must_use]
pub fn sector(ahn: u32, generations: u32, span: f64, hole: f64) -> Sector {
    let generation = generation_of(ahn);
    if generation == 0 {
        return Sector {
            ahn,
            generation,
            start: 0.0,
            end: 360.0,
            inner: 0.0,
            outer: hole,
        };
    }
    let count = f64::from(1_u32 << generation);
    let index = f64::from(ahn - (1_u32 << generation));
    let width = span / count;
    let start = f64::mul_add(index, width, -span / 2.0);
    let ring = (50.0 - hole) / f64::from(generations.max(1));
    Sector {
        ahn,
        generation,
        start,
        end: start + width,
        inner: ring.mul_add(f64::from(generation - 1), hole),
        outer: ring.mul_add(f64::from(generation), hole),
    }
}

fn point(angle: f64, radius: f64) -> (f64, f64) {
    let a = angle.to_radians();
    (
        radius.mul_add(a.sin(), 50.0),
        radius.mul_add(-a.cos(), 50.0),
    )
}

/// The outline of a sector as points (percent of the box), going out along the start edge, round the
/// outer arc, back in along the end edge and round the inner arc.
#[must_use]
pub fn outline(s: &Sector, step: f64) -> Vec<(f64, f64)> {
    let steps = (((s.end - s.start) / step).ceil() as usize).max(1);
    let at = |i: usize| s.start + (s.end - s.start) * (i as f64) / (steps as f64);
    let mut pts = Vec::new();
    if s.inner <= 0.0 {
        for i in 0..=steps {
            pts.push(point(at(i), s.outer));
        }
        return pts;
    }
    for i in 0..=steps {
        pts.push(point(at(i), s.outer));
    }
    for i in (0..=steps).rev() {
        pts.push(point(at(i), s.inner));
    }
    pts
}

/// Where a label goes: centre (percent), rotation in degrees, and whether the text runs along the
/// radius (narrow outer sectors) instead of along the ring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Label {
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub radial: bool,
}

#[must_use]
pub fn label(s: &Sector) -> Label {
    let mid = f64::midpoint(s.start, s.end);
    let (x, y) = if s.inner <= 0.0 {
        (50.0, 50.0)
    } else {
        point(mid, f64::midpoint(s.inner, s.outer))
    };
    if s.inner <= 0.0 {
        return Label {
            x,
            y,
            rotation: 0.0,
            radial: false,
        };
    }
    let radial = s.generation >= 4;
    let norm = mid.rem_euclid(360.0);
    let lower = norm > 90.0 && norm < 270.0;
    let rotation = if radial {
        // Along the radius, reading outwards on the right half and inwards on the left.
        if norm > 180.0 {
            mid - 90.0 + 180.0
        } else {
            mid - 90.0
        }
    } else if lower {
        mid + 180.0
    } else {
        mid
    };
    Label {
        x,
        y,
        rotation,
        radial,
    }
}

// ---------------------------------------------------------------------------------------------
// Labels that fit. Sizes are in `cqw`: percent of the chart's width, like everything above.
// ---------------------------------------------------------------------------------------------

/// The smallest text worth drawing, at zoom 1. Below this a sector gets no label (the tooltip and
/// the preview in the middle still name it).
pub const MIN_FONT: f64 = 0.7;

/// Average width of a character, as a fraction of the font size.
const CHAR_W: f64 = 0.56;
const LINE_H: f64 = 1.15;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Given,
    Surname,
    Years,
    Name,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub role: Role,
}

/// What to write in a sector and how big.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub font: f64,
    pub lines: Vec<Line>,
}

fn initial(word: &str) -> String {
    word.chars()
        .next()
        .map(|c| format!("{}.", c.to_uppercase()))
        .unwrap_or_default()
}

/// The given names, longest form first: all of them, the first, its initial.
fn given_forms(given: &str) -> Vec<String> {
    let first = given.split_whitespace().next().unwrap_or("");
    let mut v = vec![given.to_string()];
    if first != given {
        v.push(first.to_string());
    }
    if !first.is_empty() {
        v.push(initial(first));
    }
    v
}

/// One-line spellings of a person, longest first.
#[must_use]
pub fn name_forms(given: &str, surname: &str, surname_first: bool) -> Vec<String> {
    let join = |g: &str, s: &str| {
        let parts = if surname_first { [s, g] } else { [g, s] };
        parts
            .iter()
            .filter(|p| !p.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut out: Vec<String> = given_forms(given)
        .iter()
        .map(|g| join(g, surname))
        .collect();
    if !surname.is_empty() {
        out.push(surname.to_string());
        out.push(format!(
            "{}{}",
            initial(given.split_whitespace().next().unwrap_or("")),
            initial(surname)
        ));
    }
    out.dedup();
    out.retain(|s| !s.is_empty());
    out
}

fn widest(lines: &[Line]) -> usize {
    lines
        .iter()
        .map(|l| l.text.chars().count())
        .max()
        .unwrap_or(1)
        .max(1)
}

fn fit(lines: &[Line], width: f64, height: f64, base: f64) -> f64 {
    let by_width = width / (widest(lines) as f64 * CHAR_W);
    let by_height = height / (lines.len() as f64 * LINE_H);
    base.min(by_width).min(by_height)
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let keep = max.saturating_sub(1);
    format!(
        "{}…",
        text.chars().take(keep).collect::<String>().trim_end()
    )
}

/// Chooses the text of a sector's label: the fullest spelling that fits at a good size, otherwise
/// the one that fits best, otherwise the surname cut short, otherwise nothing. `zoom` is how far
/// the chart is magnified, which lets smaller text through.
#[must_use]
pub fn plan(
    s: &Sector,
    label: &Label,
    given: &str,
    surname: &str,
    years: &str,
    zoom: f64,
) -> Option<Plan> {
    if s.inner <= 0.0 {
        return None;
    }
    let ring = s.outer - s.inner;
    let arc = (s.end - s.start).to_radians() * (s.inner + s.outer) / 2.0;
    let (width, height, base) = if label.radial {
        (ring * 0.92, arc * 0.9, 1.25)
    } else {
        (arc * 0.86, ring * 0.9, (ring * 0.29).min(2.0))
    };
    let min = MIN_FONT / zoom.max(0.1);

    let line = |text: String, role| Line { text, role };
    let mut candidates: Vec<Vec<Line>> = Vec::new();
    if !label.radial {
        for g in given_forms(given) {
            let mut v = vec![
                line(g, Role::Given),
                line(surname.to_string(), Role::Surname),
            ];
            if !years.is_empty() {
                v.push(line(years.to_string(), Role::Years));
            }
            candidates.push(v);
        }
        for g in given_forms(given) {
            candidates.push(vec![
                line(g, Role::Given),
                line(surname.to_string(), Role::Surname),
            ]);
        }
    }
    for n in name_forms(given, surname, label.radial) {
        candidates.push(vec![line(n, Role::Name)]);
    }
    candidates.retain(|c| c.iter().all(|l| !l.text.is_empty()));

    let mut best: Option<(f64, Vec<Line>)> = None;
    for c in candidates {
        let f = fit(&c, width, height, base);
        if f >= base * 0.75 {
            return Some(Plan { font: f, lines: c });
        }
        if best.as_ref().is_none_or(|(bf, _)| f > *bf) {
            best = Some((f, c));
        }
    }
    if let Some((f, c)) = best
        && f >= min
    {
        return Some(Plan { font: f, lines: c });
    }
    // Nothing fits whole: the surname, cut to the room there is, at the smallest size allowed.
    let room = (width / (min * CHAR_W)) as usize;
    let one_line_ok = height >= min * LINE_H;
    if one_line_ok && room >= 4 && !surname.is_empty() {
        return Some(Plan {
            font: min,
            lines: vec![line(truncate(surname, room), Role::Name)],
        });
    }
    None
}
