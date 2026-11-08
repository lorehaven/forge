use crate::support::{pager, press};
use loupe::graphics::{
    DEFAULT_CELL, Graphics, KITTY_BASE, KITTY_IMAGES, Painter, TexTypesetter, Typesetter,
    detect_with, fit, fit_image, kitty_hide, kitty_id, kitty_place, kitty_transmit, move_to,
};
use loupe::pager::{Candidate, Content, Item, Key};
use quench_cli::graphics::Rgb;
use quench_cli::graphics::detect::{Capabilities, Environment, Protocol};
use quench_image::Image;
use quench_tex::typeset::{Raster, RenderOptions};
use std::cell::RefCell;
use std::io::Cursor;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

/// Draws every formula as an opaque box of 0.6 em by 0.8 em, its baseline at 0.6 em; records what it was asked.
#[derive(Debug, Default, Clone)]
struct Boxes {
    pages: Rc<RefCell<Vec<Vec<String>>>>,
    asked: Rc<RefCell<Vec<(usize, bool, u32)>>>,
    refuse: Option<usize>,
}

impl Typesetter for Boxes {
    fn page(&mut self, formulas: &[String]) {
        self.pages.borrow_mut().push(formulas.to_vec());
    }

    fn raster(&mut self, index: usize, display: bool, options: &RenderOptions) -> Option<Raster> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let em = options.pixels_per_em.round() as u32;
        self.asked.borrow_mut().push((index, display, em));
        if self.refuse == Some(index) {
            return None;
        }
        let (width, height) = ((em * 6 / 10).max(1), (em * 8 / 10).max(1));
        Some(Raster {
            width,
            height,
            baseline: em * 6 / 10,
            rgba: vec![255; (width * height * 4) as usize],
        })
    }
}

fn graphics(protocol: Protocol) -> Graphics {
    Graphics {
        protocol,
        cell: (10, 20),
        foreground: Rgb::WHITE,
        background: Rgb::BLACK,
    }
}

fn inline(index: usize, column: usize, row: usize) -> Candidate {
    Candidate {
        item: Item::Formula(index),
        content: Content::Tex {
            tex: "x".to_string(),
            display: false,
        },
        column,
        row,
        width: 2,
        height: 1,
        skip: 0,
        shown: 1,
    }
}

/// A 40×60 opaque red image.
fn red() -> Arc<Image> {
    Arc::new(Image {
        width: 40,
        height: 60,
        rgba: [255, 0, 0, 255].repeat(40 * 60),
    })
}

fn image(index: usize, row: usize, skip: usize, shown: usize) -> Candidate {
    Candidate {
        item: Item::Image(index),
        content: Content::Image(red()),
        column: 2,
        row,
        width: 4,
        height: 3,
        skip,
        shown,
    }
}

#[quench_test::test]
fn capabilities_give_the_graphics_to_draw_with() {
    assert_eq!(Graphics::from_capabilities(&Capabilities::NONE), None);
    let caps = Capabilities {
        sixel: true,
        background: Some(Rgb::WHITE),
        ..Capabilities::NONE
    };
    let g = Graphics::from_capabilities(&caps).unwrap();
    assert_eq!(g.protocol, Protocol::Sixel);
    assert_eq!(g.cell, DEFAULT_CELL);
    assert_eq!(g.foreground, Rgb::BLACK, "dark text on a light background");
    let caps = Capabilities {
        kitty: true,
        sixel: true,
        cell_size_px: Some((8, 16)),
        foreground: Some(Rgb::new(1, 2, 3)),
        ..Capabilities::NONE
    };
    let g = Graphics::from_capabilities(&caps).unwrap();
    assert_eq!(
        (g.protocol, g.cell, g.foreground),
        (Protocol::Kitty, (8, 16), Rgb::new(1, 2, 3))
    );
}

#[quench_test::test]
fn an_inline_picture_fills_its_cells_and_sits_on_the_baseline() {
    let mut boxes = Boxes::default();
    let picture = fit(&mut boxes, 0, false, (2, 1), (10, 20), Rgb::WHITE).unwrap();
    assert_eq!(
        (picture.columns, picture.rows, picture.width, picture.height),
        (2, 1, 20, 20)
    );
    // 18 px per em is a 10×14 box with its baseline at row 10; the cell's baseline is at row 16 (0.78 × 20):
    // the box is drawn from row 6, centred across (columns 5 to 14).
    let alpha = |x: usize, y: usize| picture.rgba[(y * 20 + x) * 4 + 3];
    assert_eq!(
        (alpha(5, 6), alpha(14, 19), alpha(4, 6), alpha(5, 5)),
        (255, 255, 0, 0)
    );
    assert_eq!(boxes.asked.borrow().as_slice(), [(0, false, 18)]);
}

#[quench_test::test]
fn a_picture_too_big_for_its_cells_is_scaled_down_or_refused() {
    let mut boxes = Boxes::default();
    // One cell of 8 px across for a 10 px box: the em shrinks below 18 px.
    let picture = fit(&mut boxes, 0, false, (1, 1), (8, 20), Rgb::WHITE).unwrap();
    assert_eq!(picture.width, 8);
    let asked = boxes.asked.borrow();
    assert!(
        asked.len() >= 2 && asked.last().unwrap().2 < 18,
        "{asked:?}"
    );
    drop(asked);
    // A 2 px wide cell: smaller than legible, so it stays text.
    assert!(fit(&mut Boxes::default(), 0, false, (1, 1), (2, 20), Rgb::WHITE).is_none());
    let mut refusing = Boxes {
        refuse: Some(0),
        ..Boxes::default()
    };
    assert!(fit(&mut refusing, 0, false, (2, 1), (10, 20), Rgb::WHITE).is_none());
}

#[quench_test::test]
fn kitty_sequences_transmit_and_place_with_a_source_rectangle() {
    assert_eq!(
        kitty_transmit(b"abc", 7),
        "\x1b_Ga=t,f=100,i=7,q=2;YWJj\x1b\\"
    );
    let long = kitty_transmit(&[0u8; 4000], 9);
    assert!(
        long.starts_with("\x1b_Ga=t,f=100,i=9,q=2,m=1;"),
        "{long:.40}"
    );
    assert_eq!(long.matches("\x1b_G").count(), 2);
    assert!(long.contains("\x1b\\\x1b_Gm=0;"));
    assert_eq!(
        kitty_place(7, (0, 20, 40, 40), 4, 2),
        "\x1b_Ga=p,i=7,q=2,x=0,y=20,w=40,h=40,c=4,r=2,C=1\x1b\\"
    );
    assert_eq!(kitty_id(Item::Formula(3)), KITTY_BASE + 3);
    assert_eq!(kitty_id(Item::Image(3)), KITTY_BASE + KITTY_IMAGES + 3);
}

#[quench_test::test]
fn kitty_sends_once_places_again_and_hides() {
    let boxes = Boxes::default();
    let mut painter = Painter::new(graphics(Protocol::Kitty), boxes.clone());
    assert_eq!(painter.sync(1, &["a".to_string(), "b".to_string()]), "");
    assert_eq!(
        boxes.pages.borrow().as_slice(),
        [vec!["a".to_string(), "b".to_string()]]
    );
    let first = [inline(0, 5, 3)];
    assert_eq!(
        painter.drawable(&first).into_iter().collect::<Vec<_>>(),
        [Item::Formula(0)]
    );
    let id = KITTY_BASE;
    let out = painter.paint(&first);
    let place = format!("{}{}", move_to(5, 3), kitty_place(id, (0, 0, 20, 20), 2, 1));
    assert!(
        out.starts_with(&format!("\x1b_Ga=t,f=100,i={id},q=2;")),
        "{out:?}"
    );
    assert!(out.ends_with(&place), "{out:?}");
    // Nothing moved: nothing to send.
    assert_eq!(painter.paint(&first), "");
    // Scrolled up by one line: hidden there, placed again without its data.
    let moved = [inline(0, 5, 2)];
    let out = painter.paint(&moved);
    assert_eq!(
        out,
        format!(
            "{}{}{}",
            kitty_hide(id),
            move_to(5, 2),
            kitty_place(id, (0, 0, 20, 20), 2, 1)
        )
    );
    // Scrolled off: hidden.
    assert_eq!(painter.paint(&[]), kitty_hide(id));
    // Back: placed again, not sent again.
    let out = painter.paint(&moved);
    assert!(out.contains("a=p,") && !out.contains("a=t,"), "{out:?}");
    // A new layout frees what the terminal holds.
    assert_eq!(
        painter.sync(2, &[]),
        format!("\x1b_Ga=d,d=I,i={id},q=2\x1b\\")
    );
    assert_eq!(painter.sync(2, &[]), "");
}

#[quench_test::test]
fn kitty_skips_formulas_that_do_not_typeset() {
    let boxes = Boxes {
        refuse: Some(1),
        ..Boxes::default()
    };
    let mut painter = Painter::new(graphics(Protocol::Kitty), boxes);
    painter.sync(1, &["a".to_string(), "b".to_string()]);
    let both = [inline(0, 0, 0), inline(1, 4, 0)];
    let drawable = painter.drawable(&both);
    assert!(drawable.contains(&Item::Formula(0)) && !drawable.contains(&Item::Formula(1)));
    let out = painter.paint(&both);
    assert!(out.contains(&format!("i={KITTY_BASE},")));
    assert!(!out.contains(&format!("i={},", KITTY_BASE + 1)));
}

#[quench_test::test]
fn formulas_can_be_left_as_text_while_images_are_drawn() {
    let mut painter = Painter::new(graphics(Protocol::Kitty), Boxes::default()).typesetting(false);
    painter.sync(1, &["a".to_string()]);
    let drawable = painter.drawable(&[inline(0, 0, 0), image(0, 1, 0, 3)]);
    assert_eq!(drawable.into_iter().collect::<Vec<_>>(), [Item::Image(0)]);
}

#[quench_test::test]
fn sixel_is_drawn_at_the_formula_every_time() {
    let boxes = Boxes::default();
    let mut painter = Painter::new(graphics(Protocol::Sixel), boxes.clone());
    painter.sync(1, &["a".to_string()]);
    let at = [inline(0, 7, 4)];
    let out = painter.paint(&at);
    assert!(
        out.starts_with(&format!("{}\x1bP0;1;0q\"1;1;20;20", move_to(7, 4))),
        "{out:?}"
    );
    assert!(out.ends_with("\x1b\\"));
    assert_eq!(painter.paint(&at), out, "redrawn after each text redraw");
    assert_eq!(boxes.asked.borrow().len(), 1, "typeset once");
    assert_eq!(painter.hide(), "");
    let moved = painter.paint(&[inline(0, 7, 1)]);
    assert!(moved.starts_with(&move_to(7, 1)));
}

#[quench_test::test]
fn a_display_formula_is_centred_in_its_rows() {
    let mut boxes = Boxes::default();
    let picture = fit(&mut boxes, 0, true, (40, 3), (10, 20), Rgb::WHITE).unwrap();
    assert_eq!((picture.width, picture.height), (400, 60));
    let alpha = |x: usize, y: usize| picture.rgba[(y * 400 + x) * 4 + 3];
    assert_eq!(alpha(200, 30), 255);
    assert_eq!(alpha(0, 0), 0);
}

#[quench_test::test]
fn an_image_is_scaled_into_its_cells() {
    // 40×60 into 4×3 cells of 10×20 (40×60 px): as it is.
    let picture = fit_image(&red(), 4, 3, (10, 20)).unwrap();
    assert_eq!((picture.width, picture.height), (40, 60));
    assert!(picture.rgba.chunks(4).all(|px| px == [255, 0, 0, 255]));
    // Into 2×3 cells (20×60 px): halved to 20×30, the rest transparent.
    let picture = fit_image(&red(), 2, 3, (10, 20)).unwrap();
    assert_eq!((picture.width, picture.height), (20, 60));
    let px = |x: usize, y: usize| &picture.rgba[(y * 20 + x) * 4..(y * 20 + x) * 4 + 4];
    assert_eq!(
        (px(19, 29), px(0, 30)),
        (&[255, 0, 0, 255][..], &[0, 0, 0, 0][..])
    );
    // Half-transparent pixels average with their alpha.
    let mixed = Image {
        width: 2,
        height: 1,
        rgba: vec![255, 255, 255, 255, 0, 0, 0, 0],
    };
    let picture = fit_image(&mixed, 1, 1, (1, 1)).unwrap();
    assert_eq!(picture.rgba, [255, 255, 255, 127]);
    assert!(fit_image(&red(), 0, 3, (10, 20)).is_none());
}

#[quench_test::test]
fn kitty_crops_a_partly_visible_image() {
    let mut painter = Painter::new(graphics(Protocol::Kitty), Boxes::default());
    painter.sync(1, &[]);
    let id = KITTY_BASE + KITTY_IMAGES;
    // Its first row above the screen: rows 1 and 2 shown, from pixel row 20.
    let out = painter.paint(&[image(0, 0, 1, 2)]);
    assert!(
        out.starts_with(&format!("\x1b_Ga=t,f=100,i={id},q=2")),
        "{out:.60}"
    );
    assert!(
        out.ends_with(&format!(
            "{}{}",
            move_to(2, 0),
            kitty_place(id, (0, 20, 40, 40), 4, 2)
        )),
        "{out:?}"
    );
    // Scrolled so all of it shows: placed again with the whole rectangle, not sent again.
    let out = painter.paint(&[image(0, 0, 0, 3)]);
    assert_eq!(
        out,
        format!(
            "{}{}{}",
            kitty_hide(id),
            move_to(2, 0),
            kitty_place(id, (0, 0, 40, 60), 4, 3)
        )
    );
    // Cut by the bottom: one row.
    let out = painter.paint(&[image(0, 9, 0, 1)]);
    assert!(
        out.ends_with(&kitty_place(id, (0, 0, 40, 20), 4, 1)),
        "{out:?}"
    );
}

#[quench_test::test]
fn sixel_crops_rows_before_encoding() {
    let mut painter = Painter::new(graphics(Protocol::Sixel), Boxes::default());
    painter.sync(1, &[]);
    let whole = painter.paint(&[image(0, 0, 0, 3)]);
    assert!(
        whole.starts_with(&format!("{}\x1bP0;1;0q\"1;1;40;60", move_to(2, 0))),
        "{whole:.40}"
    );
    let cut = painter.paint(&[image(0, 0, 1, 2)]);
    assert!(
        cut.starts_with(&format!("{}\x1bP0;1;0q\"1;1;40;40", move_to(2, 0))),
        "{cut:.40}"
    );
}

#[quench_test::test]
fn the_pager_and_painter_together() {
    let mut p = pager("Some $x^2$ here.\n\n$$\n\\frac{a}{b}\n$$\n", 40, 10);
    let mut painter = Painter::new(graphics(Protocol::Kitty), Boxes::default());
    let formulas: Vec<String> = p
        .rendered()
        .formulas
        .iter()
        .map(|f| f.tex.clone())
        .collect();
    painter.sync(p.generation(), &formulas);
    let candidates = p.candidates();
    let drawn = painter.drawable(&candidates);
    assert_eq!(drawn.len(), 2);
    let screen = p.screen_with(&drawn);
    assert_eq!(screen.rows[0].text(), "Some    here.");
    let out = painter.paint(&candidates);
    assert!(
        out.contains(&format!("{}\x1b_Ga=p", move_to(5, 0))),
        "{out:?}"
    );
    assert!(
        out.contains(&format!("{}\x1b_Ga=p", move_to(0, 2))),
        "{out:?}"
    );
    // `m` lays the page out again: the painter starts over.
    press(&mut p, &[Key::Char('m')]);
    let out = painter.sync(p.generation(), &formulas);
    assert!(out.contains("d=I"));
    assert_eq!(p.candidates().len(), 1);
}

#[quench_test::test]
fn real_formulas_typeset_to_cells() {
    let mut tex = TexTypesetter::default();
    tex.page(&[
        "\\newcommand{\\R}{\\mathbb{R}} x^2".to_string(),
        "\\R^n".to_string(),
        "\\boxed{x}".to_string(),
    ]);
    let picture = fit(&mut tex, 0, false, (2, 1), (10, 20), Rgb::WHITE).unwrap();
    assert_eq!((picture.width, picture.height), (20, 20));
    assert!(picture.rgba.chunks(4).any(|px| px[3] > 0 && px[0] == 255));
    assert!(
        fit(&mut tex, 1, false, (2, 1), (10, 20), Rgb::WHITE).is_some(),
        "a macro from an earlier formula"
    );
    assert!(
        fit(&mut tex, 2, false, (8, 1), (10, 20), Rgb::WHITE).is_none(),
        "unknown to quench-tex"
    );
    assert!(fit(&mut tex, 9, false, (2, 1), (10, 20), Rgb::WHITE).is_none());
}

fn terminal(term: &str) -> Environment {
    Environment {
        term: Some(term.to_string()),
        graphics: None,
        multiplexer: false,
        terminal: true,
    }
}

#[quench_test::test]
fn detection_asks_the_given_terminal_and_reads_its_answers() {
    // A kitty terminal's answers, as they would come back on /dev/tty.
    let answers = b"\x1b_Gi=31;OK\x1b\\\x1b[6;20;10t\x1b]10;rgb:eeee/eeee/eeee\x1b\\\x1b]11;rgb:1010/1010/1010\x1b\\\x1b[?62;4c";
    let mut asked = Vec::new();
    let mut input = Cursor::new(answers.to_vec());
    let caps = detect_with(
        &terminal("xterm-kitty"),
        &mut asked,
        &mut input,
        Some((80, 24)),
        &mut || Duration::ZERO,
    );
    assert!(caps.kitty && caps.sixel);
    assert_eq!(caps.cell_size_px, Some((10, 20)));
    assert_eq!(caps.foreground, Some(Rgb::new(0xee, 0xee, 0xee)));
    let asked = String::from_utf8(asked).unwrap();
    assert!(asked.starts_with("\x1b_Gi=31,s=1,v=1,a=q"), "{asked:?}");
    assert!(asked.ends_with("\x1b[c"), "{asked:?}");
}

#[quench_test::test]
fn detection_without_a_terminal_asks_nothing() {
    let mut asked = Vec::new();
    let env = Environment {
        terminal: false,
        ..terminal("xterm-kitty")
    };
    let caps = detect_with(
        &env,
        &mut asked,
        &mut Cursor::new(Vec::new()),
        None,
        &mut || Duration::MAX,
    );
    assert_eq!(caps, Capabilities::NONE);
    assert_eq!(asked, b"");
    // A terminal that answers only the device attributes, without sixel: none.
    let mut asked = Vec::new();
    let caps = detect_with(
        &terminal("xterm"),
        &mut asked,
        &mut Cursor::new(b"\x1b[?62;22c".to_vec()),
        None,
        &mut || Duration::ZERO,
    );
    assert_eq!(caps.protocol(), None);
}
