use crate::support::{Scratch, press, rows, settings};
use docs_book::Book;
use loupe::pager::{Content, ImageState, Item, Key, Pager, image_cells};
use quench_image::Image;

/// Writes a `width` × `height` opaque PNG.
fn png(dir: &Scratch, name: &str, width: u32, height: u32) {
    let rgba = [10, 200, 30, 255].repeat((width * height) as usize);
    let path = dir.write(name, "");
    std::fs::write(path, quench_image::encode_png(width, height, &rgba)).unwrap();
}

/// A pager over `markdown` written to `page.md` in `dir`, on a 40 × 30 terminal with 10 × 20 px cells.
fn open(dir: &Scratch, markdown: &str, cell: Option<(u32, u32)>) -> Pager {
    let file = dir.write("page.md", markdown);
    let mut pager = Pager::file(
        "page.md",
        Some(file),
        markdown.to_string(),
        settings(),
        None,
        (40, 30),
    );
    pager.set_graphics(cell);
    pager
}

const CELL: Option<(u32, u32)> = Some((10, 20));

#[quench_test::test]
fn an_image_takes_rows_under_its_text() {
    let dir = Scratch::new("image");
    png(&dir, "dot.png", 100, 60);
    let p = open(&dir, "# T\n\n![a dot](dot.png)\n\nafter\n", CELL);
    assert_eq!(
        rows(&p)[..9],
        [
            "T",
            &"═".repeat(40),
            "",
            "[image: a dot]",
            "",
            "",
            "",
            "",
            "after"
        ]
    );
    let ImageState::Shown {
        columns,
        rows: height,
        first_row,
        ..
    } = &p.images()[0].state
    else {
        panic!("{:?}", p.images()[0].state);
    };
    assert_eq!((*columns, *height, *first_row), (10, 3, 4));
    let candidates = p.candidates();
    assert_eq!(candidates.len(), 1);
    let c = &candidates[0];
    assert_eq!(
        (c.item, c.column, c.row, c.width, c.height, c.skip, c.shown),
        (Item::Image(0), 0, 4, 10, 3, 0, 3)
    );
    assert!(matches!(&c.content, Content::Image(image) if image.width == 100));
}

#[quench_test::test]
fn images_are_scaled_down_to_the_width_and_a_height() {
    let image = Image {
        width: 1000,
        height: 1000,
        rgba: Vec::new(),
    };
    assert_eq!(image_cells(&image, (10, 20), 40, 20), (40, 20));
    let wide = Image {
        width: 2000,
        height: 100,
        rgba: Vec::new(),
    };
    assert_eq!(image_cells(&wide, (10, 20), 40, 20), (40, 1));
    let small = Image {
        width: 15,
        height: 15,
        rgba: Vec::new(),
    };
    assert_eq!(
        image_cells(&small, (10, 20), 40, 20),
        (2, 1),
        "never scaled up"
    );
}

#[quench_test::test]
fn the_headings_below_an_image_move_with_it() {
    let dir = Scratch::new("headings");
    png(&dir, "dot.png", 100, 60);
    let mut p = open(
        &dir,
        "![a dot](dot.png)\n\n[down](#below)\n\n## Below\n\ntext\n",
        CELL,
    );
    press(&mut p, &[Key::Tab, Key::Tab, Key::Enter]);
    assert_eq!(rows(&p)[0], "Below");
}

#[quench_test::test]
fn a_partly_visible_image_is_cropped() {
    let dir = Scratch::new("crop");
    png(&dir, "dot.png", 100, 60);
    let mut page = String::from("![a dot](dot.png)\n\n");
    page.push_str(&"line\n\n".repeat(20));
    let mut p = open(&dir, &page, CELL);
    p.resize(40, 6);
    press(&mut p, &[Key::Char('j'), Key::Char('j')]);
    let c = &p.candidates()[0];
    assert_eq!((c.row, c.skip, c.shown), (0, 1, 2));
}

#[quench_test::test]
fn without_graphics_an_image_stays_text_and_says_why() {
    let dir = Scratch::new("text");
    png(&dir, "dot.png", 100, 60);
    let mut p = open(&dir, "See ![a dot](dot.png) here.\n", None);
    assert_eq!(rows(&p)[..2], ["See [image: a dot] here.", ""]);
    assert_eq!(p.candidates(), []);
    press(&mut p, &[Key::Tab]);
    let status = p.screen().status;
    assert!(
        status.contains("image not shown:") && status.contains("the terminal shows no images"),
        "{status}"
    );
    // With graphics, the focused image says what it is.
    p.set_graphics(CELL);
    press(&mut p, &[Key::Tab]);
    assert!(
        p.screen().status.contains("(100×60 px)"),
        "{}",
        p.screen().status
    );
}

#[quench_test::test]
fn what_cannot_be_drawn_says_why() {
    let dir = Scratch::new("why");
    dir.write("drawing.svg", "<svg xmlns=\"http://www.w3.org/2000/svg\"/>");
    let mut p = open(
        &dir,
        "![remote](https://example.com/a.png) ![gone](gone.png) ![vector](drawing.svg)\n",
        CELL,
    );
    let states: Vec<String> = p
        .images()
        .iter()
        .map(|i| match &i.state {
            ImageState::Text(why) => why.clone(),
            ImageState::Shown { .. } => "shown".to_string(),
        })
        .collect();
    assert_eq!(
        states[0],
        "https://example.com/a.png: remote images are not fetched"
    );
    assert!(
        states[1].starts_with("cannot read ") && states[1].contains("gone.png"),
        "{}",
        states[1]
    );
    assert!(
        states[2].ends_with("drawing.svg: SVG images are not supported"),
        "{}",
        states[2]
    );
    assert_eq!(p.candidates(), []);
    press(&mut p, &[Key::Tab]);
    assert!(p.screen().status.contains("remote images are not fetched"));
}

#[quench_test::test]
fn book_images_resolve_from_the_page_and_the_root() {
    let dir = Scratch::new("bookimg");
    dir.write("_config.yml", "title: B\nbaseurl: /book\n");
    png(&dir, "assets/logo.png", 20, 20);
    png(&dir, "guide/near.png", 20, 20);
    dir.write(
        "guide/page.md",
        "---\ntitle: Page\n---\n![near](near.png)\n\n![root](/assets/logo.png)\n\n![base]({{ site.baseurl }}/assets/logo.png)\n",
    );
    let book = Book::open(dir.path()).unwrap();
    let page = book.page("guide/page").unwrap();
    let mut p = Pager::book(book, page, settings(), None, (40, 30));
    p.set_graphics(CELL);
    for spot in p.images() {
        assert!(
            matches!(spot.state, ImageState::Shown { .. }),
            "{:?}",
            spot.state
        );
    }
    assert_eq!(p.images().len(), 3);
}

#[quench_test::test]
fn a_taller_terminal_lays_images_out_again() {
    let dir = Scratch::new("tall");
    png(&dir, "tall.png", 10, 2000);
    let mut p = open(&dir, "![tall](tall.png)\n", CELL);
    let rows_at = |p: &Pager| match &p.images()[0].state {
        ImageState::Shown { rows, .. } => *rows,
        ImageState::Text(why) => panic!("{why}"),
    };
    assert_eq!(rows_at(&p), 20);
    p.resize(40, 8);
    assert_eq!(
        rows_at(&p),
        6,
        "at most the screen's rows less the text line"
    );
}
