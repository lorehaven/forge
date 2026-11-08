use crate::support::Scratch;
use loupe::fonts::{MathFont, Source, load, validate};
use std::path::{Path, PathBuf};

/// Noto Sans Math as quench-text ships it, from the cargo registry's sources; `None` when it is not there.
fn noto_math() -> Option<Vec<u8>> {
    let cargo = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".cargo")))?;
    for registry in std::fs::read_dir(cargo.join("registry/src"))
        .ok()?
        .flatten()
    {
        for package in std::fs::read_dir(registry.path()).ok()?.flatten() {
            let file = package.path().join("fonts/NotoSansMath-Regular.ttf");
            if package
                .file_name()
                .to_string_lossy()
                .starts_with("quench-text-")
                && file.is_file()
            {
                return std::fs::read(file).ok();
            }
        }
    }
    None
}

/// Where the `MATH` table of `font` starts, and where its record is in the directory.
fn math_table(font: &[u8]) -> (usize, usize) {
    let count = usize::from(u16::from_be_bytes([font[4], font[5]]));
    let record = (0..count)
        .map(|i| 12 + i * 16)
        .find(|&r| &font[r..r + 4] == b"MATH")
        .unwrap();
    let offset = u32::from_be_bytes([
        font[record + 8],
        font[record + 9],
        font[record + 10],
        font[record + 11],
    ]);
    (offset as usize, record)
}

#[quench_test::test]
fn a_cff_font_is_refused_by_name() {
    let mut cff = b"OTTO".to_vec();
    cff.extend_from_slice(&[0; 60]);
    assert_eq!(
        validate(&cff).unwrap_err(),
        "is a CFF font: CFF outlines are not read yet"
    );
}

#[quench_test::test]
fn what_is_not_a_font_is_refused() {
    assert_eq!(
        validate(b"hello, world").unwrap_err(),
        "is not a TrueType font"
    );
    let error = load(Path::new("/no/such/font.ttf")).unwrap_err();
    assert!(
        error.starts_with("/no/such/font.ttf cannot be read"),
        "{error}"
    );
}

#[quench_test::test]
fn noto_sans_math_is_a_math_font() {
    let Some(font) = noto_math() else { return };
    assert!(validate(&font).is_ok());
    let dir = Scratch::new("font");
    let path = dir.write("Math.ttf", "");
    std::fs::write(&path, &font).unwrap();
    let chosen = MathFont::choose(Some(&path), "--math-font");
    assert!(chosen.fonts.is_ok());
    assert_eq!(chosen.warning(), None);
    assert_eq!(
        chosen.source.describe(),
        format!("{} (--math-font)", path.display())
    );
    assert!(loupe::graphics::check_typeset(chosen.fonts.as_ref().unwrap()).is_ok());
}

#[quench_test::test]
fn a_font_without_a_math_table_or_with_parts_missing_is_refused() {
    let Some(font) = noto_math() else { return };
    let (table, record) = math_table(&font);
    let mut renamed = font.clone();
    renamed[record..record + 4].copy_from_slice(b"XATH");
    assert_eq!(
        validate(&renamed).unwrap_err(),
        "has no MATH table: it is not a math font"
    );
    let mut no_info = font.clone();
    no_info[table + 6..table + 8].copy_from_slice(&[0, 0]);
    assert_eq!(
        validate(&no_info).unwrap_err(),
        "has a MATH table without its glyph info"
    );
    let mut no_variants = font;
    no_variants[table + 8..table + 10].copy_from_slice(&[0, 0]);
    assert_eq!(
        validate(&no_variants).unwrap_err(),
        "has a MATH table without its variants"
    );
}

#[quench_test::test]
fn the_bundled_font_needs_no_file() {
    let bundled = MathFont::choose(None, "--math-font");
    assert_eq!(bundled.source, Source::Bundled);
    assert_eq!(
        bundled.source.describe(),
        "Noto Sans Math, bundled (quench-text)"
    );
    assert!(bundled.fonts.is_ok());
    let broken = MathFont::choose(
        Some(Path::new("/no/font.ttf")),
        "the config file's math_font",
    );
    assert_eq!(
        broken.warning().unwrap(),
        "no math font: /no/font.ttf cannot be read: No such file or directory (os error 2); formulas are \
         shown as text"
    );
}
