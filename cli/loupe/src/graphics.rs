//! Typeset formulas and the page's images drawn over the text layout, as kitty or sixel images.
//!
//! Each formula on screen is typeset with quench-tex (Noto Sans Math, bundled, or the chosen math font), sized
//! to the terminal's cells and coloured with its foreground; each image is scaled to the rows the layout keeps
//! for it. They are sent with the kitty graphics protocol (PNG, kept by id, placed and removed as lines scroll,
//! cropped by a source rectangle when partly on screen) or as sixel (RGBA, cropped before encoding). Everything
//! here builds strings; nothing is written to a terminal.

use crate::pager::{Candidate, Content, Item};
use quench_cli::graphics::detect::{self, Capabilities, Protocol};
use quench_cli::graphics::{Rgb, kitty, sixel};
use quench_image::Image;
use quench_tex::typeset::{self, Color, Fonts, Raster, RenderOptions};
use quench_tex::{Formula, Macros, parse_with};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

/// What the terminal can draw, and with what.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Graphics {
    pub protocol: Protocol,
    /// One cell in pixels (width, height).
    pub cell: (u32, u32),
    pub foreground: Rgb,
    pub background: Rgb,
}

/// The cell size assumed when the terminal did not say (a protocol forced inside a multiplexer).
pub const DEFAULT_CELL: (u32, u32) = (10, 20);

impl Graphics {
    /// What `capabilities` allow: `None` when the terminal shows neither kitty nor sixel images.
    #[must_use]
    pub fn from_capabilities(capabilities: &Capabilities) -> Option<Self> {
        let protocol = capabilities.protocol()?;
        let background = capabilities.background.unwrap_or(Rgb::BLACK);
        let foreground = capabilities.foreground.unwrap_or_else(|| {
            if capabilities.background.is_some_and(|b| luminance(b) > 128) {
                Rgb::BLACK
            } else {
                Rgb::new(0xdd, 0xdd, 0xdd)
            }
        });
        Some(Self {
            protocol,
            cell: capabilities
                .cell_size_px
                .filter(|&(w, h)| w > 0 && h > 0)
                .unwrap_or(DEFAULT_CELL),
            foreground,
            background,
        })
    }
}

fn luminance(c: Rgb) -> u32 {
    (u32::from(c.r) * 299 + u32::from(c.g) * 587 + u32::from(c.b) * 114) / 1000
}

/// Turns the formulas of a page into pixels.
pub trait Typesetter {
    /// A new page with these formulas (their TeX), in page order: macros they define apply to those after.
    fn page(&mut self, formulas: &[String]);
    /// Formula `index` of the page, typeset in display or text style and drawn; `None` when it cannot be.
    fn raster(&mut self, index: usize, display: bool, options: &RenderOptions) -> Option<Raster>;
}

/// The typesetter of quench-tex, with the bundled Noto Sans Math or another math font.
#[derive(Debug)]
pub struct TexTypesetter {
    fonts: Fonts,
    formulas: Vec<Option<Formula>>,
}

impl TexTypesetter {
    #[must_use]
    pub const fn new(fonts: Fonts) -> Self {
        Self {
            fonts,
            formulas: Vec::new(),
        }
    }
}

impl Default for TexTypesetter {
    fn default() -> Self {
        Self::new(Fonts::bundled())
    }
}

impl Typesetter for TexTypesetter {
    fn page(&mut self, formulas: &[String]) {
        let mut macros = Macros::new();
        self.formulas = formulas
            .iter()
            .map(|tex| parse_with(tex, &mut macros).ok())
            .collect();
    }

    fn raster(&mut self, index: usize, display: bool, options: &RenderOptions) -> Option<Raster> {
        let formula = self.formulas.get(index)?.as_ref()?;
        let set = typeset::typeset_with(formula, display, &self.fonts).ok()?;
        Some(typeset::to_rgba(&set, options))
    }
}

/// A formula's picture, sized to whole cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    pub columns: usize,
    pub rows: usize,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Where the text baseline sits in a cell, as a fraction of its height from the top.
const BASELINE: f32 = 0.78;
/// The formula's em against the cell height.
const EM: f32 = 0.9;
/// Smaller than this, a formula is not legible: it stays text.
const MIN_SCALE: f32 = 0.4;

/// Typesets formula `index` to fit `columns` × `rows` cells of `cell` pixels. An inline formula (one row, not
/// display) sits on the text baseline; a display formula is centred in its area.
pub fn fit(
    typesetter: &mut dyn Typesetter,
    index: usize,
    display: bool,
    (columns, rows): (usize, usize),
    cell: (u32, u32),
    colour: Rgb,
) -> Option<Picture> {
    let (cw, ch) = cell;
    let area_w = u32::try_from(columns).ok()?.checked_mul(cw)?;
    let area_h = u32::try_from(rows.max(1)).ok()?.checked_mul(ch)?;
    if area_w == 0 || area_h == 0 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let (fw, fh, cell_h) = (area_w as f32, area_h as f32, ch as f32);
    let baseline_row = (cell_h * BASELINE).round();
    let mut ppem = cell_h * EM;
    let mut scale_total = 1.0f32;
    for _ in 0..4 {
        let options = RenderOptions {
            pixels_per_em: ppem,
            colour: Color::rgb(colour.r, colour.g, colour.b),
            background: None,
            padding: 0,
        };
        let raster = typesetter.raster(index, display, &options)?;
        #[allow(clippy::cast_precision_loss)]
        let (w, h, base) = (
            raster.width as f32,
            raster.height as f32,
            raster.baseline as f32,
        );
        let scale = if display || rows > 1 {
            (fw / w).min(fh / h).min(1.0)
        } else {
            let above = if base > 0.0 { baseline_row / base } else { 1.0 };
            let below = if h > base {
                (cell_h - baseline_row) / (h - base)
            } else {
                1.0
            };
            (fw / w).min(above).min(below).min(1.0)
        };
        if scale >= 1.0 {
            let (x, y) = if display || rows > 1 {
                ((area_w - raster.width) / 2, (area_h - raster.height) / 2)
            } else {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let y = (baseline_row as u32).saturating_sub(raster.baseline);
                ((area_w - raster.width) / 2, y)
            };
            return Some(Picture {
                columns,
                rows: rows.max(1),
                width: area_w,
                height: area_h,
                rgba: pad(&raster, area_w, area_h, x, y),
            });
        }
        scale_total *= scale;
        if scale_total < MIN_SCALE {
            return None;
        }
        ppem *= scale * 0.97;
    }
    None
}

/// `raster` copied onto a transparent `width` × `height` image at (`x`, `y`), clipped.
fn pad(raster: &Raster, width: u32, height: u32, left: u32, top: u32) -> Vec<u8> {
    let (out_w, out_h) = (width as usize, height as usize);
    let mut out = vec![0u8; out_w * out_h * 4];
    let (src_w, src_h) = (raster.width as usize, raster.height as usize);
    let (left, top) = (left as usize, top as usize);
    if left >= out_w {
        return out;
    }
    let count = src_w.min(out_w - left);
    for row in 0..src_h {
        let target = row + top;
        if target >= out_h {
            break;
        }
        let src = &raster.rgba[row * src_w * 4..(row * src_w + count) * 4];
        out[(target * out_w + left) * 4..(target * out_w + left + count) * 4].copy_from_slice(src);
    }
    out
}

/// The cursor to (`column`, `row`), both from 0.
#[must_use]
pub fn move_to(column: usize, row: usize) -> String {
    format!("\x1b[{};{}H", row + 1, column + 1)
}

/// Removes every placement of kitty image `id` and keeps its data, so it can be placed again.
#[must_use]
pub fn kitty_hide(id: u32) -> String {
    format!("\x1b_Ga=d,d=i,i={id},q=2\x1b\\")
}

/// The first kitty image id loupe uses: formula `i` of a page is `KITTY_BASE + i`, image `i` is
/// `KITTY_BASE + KITTY_IMAGES + i`.
pub const KITTY_BASE: u32 = 0x4c00;
/// Where the ids of a page's images start, after its formulas'.
pub const KITTY_IMAGES: u32 = 0x10_0000;

/// The kitty id of `item`.
#[must_use]
pub fn kitty_id(item: Item) -> u32 {
    let n = |i: usize| u32::try_from(i).unwrap_or(KITTY_IMAGES - 1);
    match item {
        Item::Formula(i) => KITTY_BASE + n(i).min(KITTY_IMAGES - 1),
        Item::Image(i) => KITTY_BASE + KITTY_IMAGES + n(i).min(KITTY_IMAGES - 1),
    }
}

/// Sends a PNG to kitty under `id` without showing it (`a=t`), in chunks of at most 4096 bytes of base64.
#[must_use]
pub fn kitty_transmit(png: &[u8], id: u32) -> String {
    let data = quench_core::base64::STANDARD.encode(png);
    let chunks: Vec<&[u8]> = data.as_bytes().chunks(kitty::CHUNK).collect();
    let mut out = String::new();
    if chunks.len() <= 1 {
        let _ = write!(out, "\x1b_Ga=t,f=100,i={id},q=2;{data}\x1b\\");
        return out;
    }
    for (k, chunk) in chunks.iter().enumerate() {
        let chunk = std::str::from_utf8(chunk).unwrap_or_default();
        let more = u8::from(k + 1 < chunks.len());
        if k == 0 {
            let _ = write!(out, "\x1b_Ga=t,f=100,i={id},q=2,m=1;{chunk}\x1b\\");
        } else {
            let _ = write!(out, "\x1b_Gm={more};{chunk}\x1b\\");
        }
    }
    out
}

/// Shows kitty image `id` at the cursor: the source rectangle `(x, y, w, h)` of it (pixels), over `columns` ×
/// `rows` cells, the cursor left where it was.
#[must_use]
pub fn kitty_place(
    id: u32,
    (x, y, w, h): (u32, u32, u32, u32),
    columns: usize,
    rows: usize,
) -> String {
    format!("\x1b_Ga=p,i={id},q=2,x={x},y={y},w={w},h={h},c={columns},r={rows},C=1\x1b\\")
}

/// `image` scaled down (never up) to fit `columns` × `rows` cells of `cell` pixels, on a transparent picture of
/// exactly that many cells, at its top left. Scaling averages the pixels each target pixel covers.
#[must_use]
pub fn fit_image(image: &Image, columns: usize, rows: usize, cell: (u32, u32)) -> Option<Picture> {
    let (cw, ch) = cell;
    let width = u32::try_from(columns).ok()?.checked_mul(cw)?;
    let height = u32::try_from(rows).ok()?.checked_mul(ch)?;
    if width == 0 || height == 0 || image.width == 0 || image.height == 0 {
        return None;
    }
    let scale = (f64::from(width) / f64::from(image.width))
        .min(f64::from(height) / f64::from(image.height))
        .min(1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (tw, th) = (
        ((f64::from(image.width) * scale).round() as u32).clamp(1, width),
        ((f64::from(image.height) * scale).round() as u32).clamp(1, height),
    );
    let (sw, sh) = (image.width as usize, image.height as usize);
    let (out_w, tw, th) = (width as usize, tw as usize, th as usize);
    let mut rgba = vec![0u8; out_w * height as usize * 4];
    for ty in 0..th {
        let (y0, y1) = (
            ty * sh / th,
            ((ty + 1) * sh / th).max(ty * sh / th + 1).min(sh),
        );
        for tx in 0..tw {
            let (x0, x1) = (
                tx * sw / tw,
                ((tx + 1) * sw / tw).max(tx * sw / tw + 1).min(sw),
            );
            // Averaged with alpha weights, so transparent pixels do not darken the edges.
            let mut sum = [0u64; 4];
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let px = &image.rgba[(sy * sw + sx) * 4..(sy * sw + sx) * 4 + 4];
                    let a = u64::from(px[3]);
                    sum[0] += u64::from(px[0]) * a;
                    sum[1] += u64::from(px[1]) * a;
                    sum[2] += u64::from(px[2]) * a;
                    sum[3] += a;
                }
            }
            let count = ((y1 - y0) * (x1 - x0)) as u64;
            let at = (ty * out_w + tx) * 4;
            for c in 0..3 {
                if let Some(v) = sum[c].checked_div(sum[3]) {
                    rgba[at + c] = u8::try_from(v).unwrap_or(u8::MAX);
                }
            }
            rgba[at + 3] = u8::try_from(sum[3] / count).unwrap_or(u8::MAX);
        }
    }
    Some(Picture {
        columns,
        rows,
        width,
        height,
        rgba,
    })
}

/// Rows `skip..skip + shown` (in cells of `cell_height` pixels) of `picture`'s pixels.
fn crop(picture: &Picture, skip: usize, shown: usize, cell_height: u32) -> (Vec<u8>, u32) {
    let row_bytes = picture.width as usize * 4;
    let ch = cell_height as usize;
    let start = (skip * ch).min(picture.height as usize);
    let end = ((skip + shown) * ch).min(picture.height as usize);
    let rgba = picture.rgba[start * row_bytes..end * row_bytes].to_vec();
    (rgba, u32::try_from(end - start).unwrap_or(0))
}

/// Where a kitty image is placed: column, row, rows skipped, rows shown.
type Placed = (usize, usize, usize, usize);

/// Draws formulas and images on screen, remembering what the terminal already holds.
#[derive(Debug)]
pub struct Painter<T> {
    graphics: Graphics,
    typesetter: T,
    generation: Option<u64>,
    /// Pictures; `None` for one that cannot be drawn.
    pictures: HashMap<Item, Option<Picture>>,
    /// Kitty: the ids sent, and where each is placed.
    sent: HashSet<u32>,
    placed: HashMap<u32, Placed>,
    /// Sixel: encoded images by item and crop.
    sixels: HashMap<(Item, usize, usize), String>,
    /// Formulas are typeset; when false they stay text and only images are drawn.
    formulas: bool,
}

impl<T: Typesetter> Painter<T> {
    #[must_use]
    pub fn new(graphics: Graphics, typesetter: T) -> Self {
        Self {
            graphics,
            typesetter,
            generation: None,
            pictures: HashMap::new(),
            sent: HashSet::new(),
            placed: HashMap::new(),
            sixels: HashMap::new(),
            formulas: true,
        }
    }

    /// Whether formulas are typeset (`--math text`, or a math font that cannot be used: they stay text).
    #[must_use]
    pub const fn typesetting(mut self, formulas: bool) -> Self {
        self.formulas = formulas;
        self
    }

    #[must_use]
    pub const fn graphics(&self) -> &Graphics {
        &self.graphics
    }

    /// Readies the painter for laid-out page `generation` with these formulas (TeX in page order). A new
    /// generation drops the pictures and returns what frees the terminal's copies of them.
    pub fn sync(&mut self, generation: u64, formulas: &[String]) -> String {
        if self.generation == Some(generation) {
            return String::new();
        }
        self.generation = Some(generation);
        self.pictures.clear();
        self.sixels.clear();
        self.typesetter.page(formulas);
        self.forget()
    }

    /// What removes and frees every kitty image sent; nothing for sixel.
    pub fn forget(&mut self) -> String {
        let mut out = String::new();
        let mut ids: Vec<u32> = self.sent.drain().collect();
        ids.sort_unstable();
        for id in ids {
            out.push_str(&kitty::delete(id));
        }
        self.placed.clear();
        out
    }

    /// Which of `candidates` have a picture (making it now if need be): the pager blanks their text.
    pub fn drawable(&mut self, candidates: &[Candidate]) -> HashSet<Item> {
        let mut out = HashSet::new();
        for candidate in candidates {
            if self.picture(candidate).is_some() {
                out.insert(candidate.item);
            }
        }
        out
    }

    fn picture(&mut self, candidate: &Candidate) -> Option<&Picture> {
        if !self.formulas && matches!(candidate.item, Item::Formula(_)) {
            return None;
        }
        let graphics = self.graphics;
        let typesetter = &mut self.typesetter;
        self.pictures
            .entry(candidate.item)
            .or_insert_with(|| match (&candidate.content, candidate.item) {
                (Content::Tex { display, .. }, Item::Formula(index)) => fit(
                    typesetter,
                    index,
                    *display,
                    (candidate.width, candidate.height),
                    graphics.cell,
                    graphics.foreground,
                ),
                (Content::Image(image), _) => {
                    fit_image(image, candidate.width, candidate.height, graphics.cell)
                }
                (Content::Tex { .. }, Item::Image(_)) => None,
            })
            .as_ref()
    }

    /// The escape sequences that show `candidates` where they are now, after the text has been drawn, cropped
    /// to their visible rows. Kitty images are sent once, placed again when they move or their crop changes,
    /// and hidden when they leave the screen; sixel images are drawn again each time, since redrawing the text
    /// erases them.
    pub fn paint(&mut self, candidates: &[Candidate]) -> String {
        match self.graphics.protocol {
            Protocol::Kitty => self.paint_kitty(candidates),
            Protocol::Sixel => self.paint_sixel(candidates),
        }
    }

    fn paint_kitty(&mut self, candidates: &[Candidate]) -> String {
        let mut wanted: HashMap<u32, Placed> = HashMap::new();
        let mut order = Vec::new();
        for candidate in candidates {
            if self.picture(candidate).is_some() {
                let id = kitty_id(candidate.item);
                let at = (
                    candidate.column,
                    candidate.row,
                    candidate.skip,
                    candidate.shown,
                );
                wanted.insert(id, at);
                order.push((id, candidate.item, at));
            }
        }
        let mut out = String::new();
        let mut gone: Vec<u32> = self
            .placed
            .iter()
            .filter(|&(id, at)| wanted.get(id) != Some(at))
            .map(|(&id, _)| id)
            .collect();
        gone.sort_unstable();
        for id in gone {
            out.push_str(&kitty_hide(id));
            self.placed.remove(&id);
        }
        let ch = self.graphics.cell.1;
        for (id, item, at) in order {
            if self.placed.get(&id) == Some(&at) {
                continue;
            }
            let Some(Some(picture)) = self.pictures.get(&item) else {
                continue;
            };
            let (column, row, skip, shown) = at;
            if self.sent.insert(id) {
                let png = quench_image::encode_png(picture.width, picture.height, &picture.rgba);
                out.push_str(&kitty_transmit(&png, id));
            }
            let y = u32::try_from(skip).unwrap_or(0).saturating_mul(ch);
            let h = u32::try_from(shown)
                .unwrap_or(0)
                .saturating_mul(ch)
                .min(picture.height.saturating_sub(y));
            out.push_str(&move_to(column, row));
            out.push_str(&kitty_place(
                id,
                (0, y, picture.width, h),
                picture.columns,
                shown,
            ));
            self.placed.insert(id, at);
        }
        out
    }

    fn paint_sixel(&mut self, candidates: &[Candidate]) -> String {
        let mut out = String::new();
        let (background, ch) = (self.graphics.background, self.graphics.cell.1);
        for candidate in candidates {
            if self.picture(candidate).is_none() {
                continue;
            }
            let key = (candidate.item, candidate.skip, candidate.shown);
            if !self.sixels.contains_key(&key) {
                let Some(Some(picture)) = self.pictures.get(&candidate.item) else {
                    continue;
                };
                let (rgba, height) = crop(picture, candidate.skip, candidate.shown, ch);
                let Ok(encoded) =
                    sixel::encode(&rgba, picture.width as usize, height as usize, background)
                else {
                    continue;
                };
                self.sixels.insert(key, encoded);
            }
            let Some(encoded) = self.sixels.get(&key) else {
                continue;
            };
            let _ = write!(out, "{}{encoded}", move_to(candidate.column, candidate.row));
        }
        out
    }

    /// Hides every kitty placement (an overlay covers the page); sixel images go with the text redraw.
    pub fn hide(&mut self) -> String {
        let mut out = String::new();
        let mut ids: Vec<u32> = self.placed.drain().map(|(id, _)| id).collect();
        ids.sort_unstable();
        for id in ids {
            out.push_str(&kitty_hide(id));
        }
        out
    }
}

/// Typesets a sample formula with `fonts`: the size of the picture, or why it failed.
///
/// # Errors
/// The typesetter's error, as text.
pub fn check_typeset(fonts: &Fonts) -> Result<(u32, u32), String> {
    let options = RenderOptions {
        pixels_per_em: 20.0,
        ..RenderOptions::default()
    };
    let formula = quench_tex::parse(r"\sum_{k=1}^{n} k^2 = \frac{n(n+1)(2n+1)}{6}")
        .map_err(|e| e.to_string())?;
    let set = typeset::typeset_with(&formula, true, fonts).map_err(|e| format!("{e:?}"))?;
    let raster = typeset::to_rgba(&set, &options);
    if raster.rgba.iter().skip(3).step_by(4).any(|&a| a > 0) {
        Ok((raster.width, raster.height))
    } else {
        Err("the formula was typeset but drew no ink".to_string())
    }
}

/// Detection on a given terminal, or on fakes of one.
///
/// [`detect::plan`] from `environment`, then (when it says to ask) the queries written to `output` and the
/// answers read from `input` (a terminal in raw mode with a read timeout, or a fake), then [`detect::resolve`]
/// with the terminal's size (columns, rows).
pub fn detect_with(
    environment: &detect::Environment,
    output: &mut dyn std::io::Write,
    input: &mut dyn std::io::Read,
    size: Option<(u16, u16)>,
    elapsed: &mut dyn FnMut() -> std::time::Duration,
) -> Capabilities {
    let plan = detect::plan(environment);
    if !plan.probe {
        return detect::resolve(plan, None, None);
    }
    let replies = detect::probe(output, input, detect::DEFAULT_TIMEOUT, elapsed).ok();
    detect::resolve(plan, replies.as_ref(), size)
}
