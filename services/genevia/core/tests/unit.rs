use genevia_core::{Date, Sex, Tree};

const SAMPLE: &str = "\u{feff}0 HEAD\r\n0 @I1@ INDI\r\n1 NAME Paweł Jan /Walus/\r\n1 SEX M\r\n1 BIRT\r\n2 DATE 3 SEP 1993\r\n2 PLAC Mikołów\r\n1 FAMC @F1@\r\n1 _AHN 1\r\n1 NOTE first line\r\n2 CONT second\r\n2 CONC  more\r\n0 @I2@ INDI\r\n1 NAME Krzysztof /Walus/\r\n1 SEX M\r\n1 FAMS @F1@\r\n1 FAMC @F2@\r\n0 @I3@ INDI\r\n1 NAME Bogumiła /Węgrzyn/\r\n1 SEX F\r\n1 FAMS @F1@\r\n0 @I4@ INDI\r\n1 NAME Czesław /Walus/\r\n1 SEX M\r\n1 FAMS @F2@\r\n0 @F1@ FAM\r\n1 HUSB @I2@\r\n1 WIFE @I3@\r\n1 CHIL @I1@\r\n1 MARR\r\n2 DATE 21 SEP 1991\r\n0 @F2@ FAM\r\n1 HUSB @I4@\r\n1 CHIL @I2@\r\n0 TRLR\r\n";

fn tree() -> Tree {
    Tree::parse("sample", SAMPLE).expect("parses")
}

#[quench_test::test]
fn people_and_families_are_read() {
    let t = tree();
    assert_eq!(t.people().len(), 4);
    assert_eq!(t.families().len(), 2);
    let p = t.person("I1").expect("I1");
    assert_eq!(p.full_name(), "Paweł Jan Walus");
    assert_eq!(p.sex, Sex::Male);
    assert_eq!(p.birth_year(), Some(1993));
    assert_eq!(p.notes, vec!["first line\nsecond more".to_string()]);
}

#[quench_test::test]
fn dates_print_in_polish() {
    assert_eq!(Date::parse("3 SEP 1993").display(), "3 wrz 1993");
    assert_eq!(Date::parse("ABT 1765").display(), "ok. 1765");
    assert_eq!(Date::parse("SEP 1811").display(), "wrz 1811");
    assert_eq!(Date::parse("BEF 1904").display(), "przed 1904");
    assert_eq!(
        Date::parse("FROM 1925 TO 1960").display(),
        "od 1925 do 1960"
    );
    assert_eq!(
        Date::parse("BET MAR 1900 AND 1901").display(),
        "między mar 1900 a 1901"
    );
}

#[quench_test::test]
fn ancestors_follow_the_ahnentafel() {
    let t = tree();
    let a = t.ancestors("I1", 5);
    let numbers: Vec<u32> = a.iter().map(|s| s.ahn).collect();
    assert_eq!(numbers, vec![1, 2, 3, 4]);
    assert_eq!(a[3].xref, "I4");
    assert_eq!(a[3].generation, 2);
    assert_eq!(t.ancestors("I1", 1).len(), 3);
}

#[quench_test::test]
fn root_is_the_numbered_one() {
    assert_eq!(tree().default_root().xref, "I1");
}

#[quench_test::test]
fn search_ignores_diacritics() {
    let t = tree();
    let hits = t.search("wegrzyn", 10);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].xref, "I3");
    assert_eq!(t.search("mikolow", 10).len(), 1);
}

#[quench_test::test]
fn the_real_file_has_the_expected_shape() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../data/walus_family_tree.ged");
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    let t = Tree::parse("walus", &text).expect("parses");
    assert_eq!(t.people().len(), 193);
    assert_eq!(t.families().len(), 96);
    let root = t.default_root();
    assert_eq!(root.full_name(), "Paweł Jan Walus");
    assert_eq!(t.ancestors(&root.xref, 20).len(), 193);
}

#[quench_test::test]
fn fan_sectors_tile_each_ring() {
    use genevia_core::fan::{generation_of, sector};
    assert_eq!(generation_of(1), 0);
    assert_eq!(generation_of(7), 2);
    assert_eq!(generation_of(8), 3);
    // Father and mother share the span between them, father on the left.
    let f = sector(2, 4, 180.0, 10.0);
    let m = sector(3, 4, 180.0, 10.0);
    assert_eq!((f.start, f.end, m.start, m.end), (-90.0, 0.0, 0.0, 90.0));
    // The last ring ends at the edge of the box.
    let last = sector(31, 4, 360.0, 10.0);
    assert!((last.outer - 50.0).abs() < 1e-9);
    // Rings do not overlap.
    assert!((sector(4, 4, 360.0, 10.0).outer - sector(8, 4, 360.0, 10.0).inner).abs() < 1e-9);
}

#[quench_test::test]
fn labels_shrink_to_fit_or_vanish() {
    use genevia_core::fan::{self, Role};
    // A wide inner sector takes the full name on separate lines.
    let s = fan::sector(2, 6, 270.0, 11.0);
    let p = fan::plan(&s, &fan::label(&s), "Krzysztof Jan", "Walus", "* 1967", 1.0).expect("fits");
    assert_eq!(p.lines[0].role, Role::Given);
    assert_eq!(p.lines[0].text, "Krzysztof Jan");
    // A narrow outer sector gets a short form, in text no larger than the room allows.
    let s = fan::sector(40, 6, 270.0, 11.0);
    let l = fan::label(&s);
    let p = fan::plan(&s, &l, "Waleska Franciszka", "Bromboszcz", "", 1.0);
    if let Some(p) = p {
        assert_eq!(p.lines.len(), 1);
        assert!(p.lines[0].text.chars().count() <= "Bromboszcz Waleska Franciszka".chars().count());
        assert!(p.font >= fan::MIN_FONT);
    }
    // The outermost ring of a deep fan is too narrow to read, until you zoom.
    let s = fan::sector(300, 8, 180.0, 11.0);
    let l = fan::label(&s);
    assert!(fan::plan(&s, &l, "Maria", "Kowalska", "", 1.0).is_none());
    assert!(fan::plan(&s, &l, "Maria", "Kowalska", "", 4.0).is_some());
}

#[quench_test::test]
fn name_forms_run_from_long_to_short() {
    let f = genevia_core::fan::name_forms("Waleska Franciszka", "Bromboszcz", false);
    assert_eq!(f[0], "Waleska Franciszka Bromboszcz");
    assert_eq!(f[1], "Waleska Bromboszcz");
    assert_eq!(f[2], "W. Bromboszcz");
    assert_eq!(f[3], "Bromboszcz");
}

#[quench_test::test]
fn a_tree_rebuilt_from_its_parts_answers_the_same() {
    let t = tree();
    let again =
        Tree::from_parts("again", t.people().to_vec(), t.families().to_vec()).expect("builds");
    assert_eq!(again.people().len(), t.people().len());
    assert_eq!(again.ancestors("I1", 5), t.ancestors("I1", 5));
    assert_eq!(again.person("I2").expect("I2").fams, vec!["F1".to_string()]);
    assert_eq!(again.person("I2").expect("I2").famc, vec!["F2".to_string()]);
    assert!(Tree::from_parts("none", Vec::new(), Vec::new()).is_err());
}

const MYHERITAGE: &str = "0 HEAD
0 @S1@ SOUR
1 TITL Księga chrztów parafii Mikołów
0 @I1@ INDI
1 NAME Anna Maria /Kowalska/
2 GIVN Anna Maria
2 SURN Kowalska
2 NICK Ania
2 NPFX Dr
1 NAME Anna /Nowak/
2 TYPE birth
2 SURN Nowak
1 SEX F
1 BIRT
2 DATE 4 MAR 1901
2 PLAC Mikołów, Śląsk
2 SOUR @S1@
3 PAGE s. 12, poz. 34
1 CHR
2 DATE 10 MAR 1901
2 PLAC Mikołów
1 DEAT
2 DATE 1 JAN 1980
2 CAUS zawał serca
1 BURI
2 PLAC cmentarz Mikołów
1 OCCU nauczycielka
2 DATE FROM 1925 TO 1960
2 PLAC Mikołów
1 EDUC Seminarium nauczycielskie
1 RELI rzymskokatolicka
1 EVEN
2 TYPE Służba wojskowa
2 NOTE w armii
1 NOTE Pierwsza uwaga
1 SOUR Zapis rodzinny
0 TRLR
";

#[quench_test::test]
fn standard_tags_become_fields() {
    let t = Tree::parse("mh", MYHERITAGE).expect("parses");
    let p = t.person("I1").expect("I1");
    assert_eq!(
        (p.given.as_str(), p.surname.as_str()),
        ("Anna Maria", "Kowalska")
    );
    assert_eq!(
        (
            p.nickname.as_str(),
            p.prefix.as_str(),
            p.maiden_name.as_str()
        ),
        ("Ania", "Dr", "Nowak")
    );
    let birth = p.birth.as_ref().expect("birth");
    assert_eq!(birth.sources[0].title, "Księga chrztów parafii Mikołów");
    assert_eq!(birth.sources[0].detail, "s. 12, poz. 34");
    assert_eq!(
        p.death.as_ref().and_then(|d| d.cause.as_deref()),
        Some("zawał serca")
    );
    let kinds: Vec<&str> = p.facts.iter().map(|f| f.kind.as_str()).collect();
    assert_eq!(kinds, vec!["CHR", "BURI", "OCCU", "EDUC", "RELI", "EVEN"]);
    let occ = &p.facts[2];
    assert_eq!(occ.value, "nauczycielka");
    assert_eq!(occ.event.place.as_deref(), Some("Mikołów"));
    assert_eq!(p.facts[5].value, "Służba wojskowa");
    assert_eq!(p.sources[0].title, "Zapis rodzinny");
    assert_eq!(p.notes, vec!["Pierwsza uwaga".to_string()]);
    // The maiden name and the nickname are searchable.
    assert_eq!(t.search("nowak", 5).len(), 1);
    assert_eq!(t.search("ania", 5).len(), 1);
}

#[quench_test::test]
fn witness_tags_and_burials_are_read() {
    let text = "0 @I1@ INDI\n1 NAME Jan /Walus/\n1 BIRT\n2 DATE 1 JAN 1900\n2 _WITN Anna Kowal\n2 _WITN Piotr Nowak\n2 SOUR Akt urodzenia\n3 PAGE USC Mikołów 1900 nr 5\n1 DEAT\n2 CAUS wylew\n2 NOTE Wiek w chwili zgonu: 70 lat\n1 BURI\n2 PLAC cmentarz Mikołów\n1 OCCU górnik\n";
    let t = Tree::parse("w", text).expect("parses");
    let p = t.person("I1").expect("I1");
    let birth = p.birth.as_ref().expect("birth");
    assert_eq!(
        birth.witnesses,
        vec!["Anna Kowal".to_string(), "Piotr Nowak".to_string()]
    );
    assert_eq!(
        (
            birth.sources[0].title.as_str(),
            birth.sources[0].detail.as_str()
        ),
        ("Akt urodzenia", "USC Mikołów 1900 nr 5")
    );
    assert_eq!(
        p.death.as_ref().and_then(|d| d.cause.as_deref()),
        Some("wylew")
    );
    assert_eq!(p.facts[0].kind, "BURI");
    assert_eq!(p.facts[0].event.place.as_deref(), Some("cmentarz Mikołów"));
    assert_eq!(p.facts[1].value, "górnik");
}

#[quench_test::test]
fn the_notes_of_the_old_conversion_become_fields() {
    let text = "0 @I1@ INDI\n1 NAME Jan /Walus/\n1 BIRT\n2 DATE 1 JAN 1900\n1 NOTE Chrzestni: Anna Kowal, Piotr Nowak\n1 NOTE Akt urodzenia: USC Mikołów 1900 nr 5\n1 NOTE Akt zgonu: USC Mikołów 1970 nr 9\n1 NOTE Zawód: hutnik\n1 NOTE Zrodlo: arkusz Walus, wiersz 8\n1 NOTE Uwagi: szpital\n0 @F1@ FAM\n1 HUSB @I1@\n1 MARR\n2 NOTE Swiadkowie: A B, C D\n2 NOTE Akt slubu: USC Pilchowice 1991 nr 32\n2 NOTE lat 20\n";
    let t = Tree::parse("old", text).expect("parses");
    let p = t.person("I1").expect("I1");
    let birth = p.birth.as_ref().expect("birth");
    assert_eq!(
        birth.witnesses,
        vec!["Anna Kowal".to_string(), "Piotr Nowak".to_string()]
    );
    assert_eq!(birth.sources[0].detail, "USC Mikołów 1900 nr 5");
    assert_eq!(
        p.death.as_ref().expect("death made for its record").sources[0].title,
        "Akt zgonu"
    );
    assert_eq!(
        (p.facts[0].kind.as_str(), p.facts[0].value.as_str()),
        ("OCCU", "hutnik")
    );
    assert_eq!(p.sources[0].detail, "Walus, wiersz 8");
    assert_eq!(p.notes, vec!["Uwagi: szpital".to_string()]);
    let m = t
        .family("F1")
        .expect("F1")
        .marriage
        .as_ref()
        .expect("marriage");
    assert_eq!(m.witnesses, vec!["A B".to_string(), "C D".to_string()]);
    assert_eq!(m.sources[0].title, "Akt ślubu");
    assert_eq!(m.notes, vec!["lat 20".to_string()]);
}

#[quench_test::test]
fn dates_go_between_typing_and_gedcom() {
    use genevia_core::date::{from_input, to_input};
    for (typed, raw) in [
        ("21.08.1967", "21 AUG 1967"),
        ("08.1967", "AUG 1967"),
        ("1967", "1967"),
        ("ok. 1765", "ABT 1765"),
        ("przed 03.1904", "BEF MAR 1904"),
        ("po 1900", "AFT 1900"),
        ("od 1925 do 1960", "FROM 1925 TO 1960"),
        ("między 1900 a 1901", "BET 1900 AND 1901"),
        ("", ""),
    ] {
        assert_eq!(from_input(typed).as_deref(), Ok(raw), "{typed}");
        assert_eq!(to_input(raw), typed, "{raw}");
    }
    assert_eq!(from_input("3 SEP 1993").as_deref(), Ok("3 SEP 1993"));
    assert!(from_input("32.01.1900").is_err());
    assert!(from_input("jutro").is_err());
}

quench_test::main!();
