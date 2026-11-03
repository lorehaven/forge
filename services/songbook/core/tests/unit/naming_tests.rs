use songbook_core::naming::*;

#[quench_test::test]
fn norm_keeps_letters_and_digits_of_any_script() {
    assert_eq!(norm("Pięć Dwa Dębiec"), "pięćdwadębiec");
    assert_eq!(norm("A$AP Rocky!"), "aaprocky");
    assert_eq!(norm("Ż"), "ż");
    assert_ne!(norm("Ó"), norm("Ż"));
}

#[quench_test::test]
fn titles_lose_feat_and_remaster_but_keep_other_versions() {
    assert_eq!(
        clean_title("Never Leave Me Alone (feat. Snoop Dogg)"),
        "Never Leave Me Alone"
    );
    assert_eq!(clean_title("Song [ft. X] (Live)"), "Song (Live)");
    assert_eq!(clean_title("Song (Featuring Y)"), "Song");
    assert_eq!(clean_title("Song (Remastered 2011)"), "Song");
    assert_eq!(clean_title("Song - 2011 Remaster"), "Song");
    assert_eq!(clean_title("Song - Remastered"), "Song");
    assert_eq!(clean_title("Song (Sped Up)"), "Song (Sped Up)");
    assert_eq!(clean_title("Song (Remix)"), "Song (Remix)");
    assert_eq!(clean_title("Featuring Love"), "Featuring Love");
    assert_eq!(clean_title("(feat. X)"), "(feat. X)");
}

#[quench_test::test]
fn the_same_song_has_one_key() {
    let a = song_key(
        &["Nate Dogg", "Snoop Dogg"],
        "Never Leave Me Alone (feat. Snoop Dogg)",
    );
    let b = song_key(
        &["snoop dogg", "NATE DOGG"],
        "Never Leave Me Alone - 2004 Remaster",
    );
    assert_eq!(a, b);
    assert_eq!(a, "natedogg|snoopdogg||neverleavemealone");
    assert_ne!(a, song_key(&["Nate Dogg"], "Never Leave Me Alone"));
    assert_eq!(
        song_key(&["A"], "Me & My Homies"),
        song_key(&["A"], "Me and My Homies")
    );
}

#[quench_test::test]
fn album_editions_are_one_album() {
    assert_eq!(
        album_key("Illmatic (Deluxe Edition)"),
        album_key("Illmatic")
    );
    assert_eq!(album_key("Stillmatic [Explicit]"), album_key("Stillmatic"));
    assert_ne!(album_key("Illmatic XX"), album_key("Illmatic"));
}

#[quench_test::test]
fn file_names_list_artists_alphabetically() {
    assert_eq!(
        make_filename(
            &["Snoop Dogg", "Nate Dogg"],
            "Never Leave Me Alone (feat. Snoop Dogg)"
        ),
        "Nate Dogg, Snoop Dogg - Never Leave Me Alone"
    );
    assert_eq!(make_filename(&["b", "A"], "T"), "A, b - T");
    assert_eq!(
        make_filename(&["AC/DC"], "Who Made Who?"),
        "AC_DC - Who Made Who_"
    );
    assert_eq!(
        make_filename(&[".voodoo people", "Opał"], "HALO"),
        "voodoo people, Opał - HALO"
    );
    assert_eq!(make_filename(&[], "T"), "Unknown Artist - T");
}

#[quench_test::test]
fn long_names_are_cut_to_fit_a_filesystem() {
    let artists: Vec<String> = (0..60).map(|i| format!("Artist Number {i:02}")).collect();
    let names: Vec<&str> = artists.iter().map(String::as_str).collect();
    let name = make_filename(&names, "A Title");
    assert!(name.len() <= MAX_NAME, "{}", name.len());
    assert!(name.ends_with(" - A Title"));
    assert!(disambiguate(&name, "k").len() <= MAX_NAME);
}

#[quench_test::test]
fn disambiguation_is_stable_and_differs_per_key() {
    assert_eq!(disambiguate("A - T", "k1"), disambiguate("A - T", "k1"));
    assert_ne!(disambiguate("A - T", "k1"), disambiguate("A - T", "k2"));
    assert!(disambiguate("A - T", "k1").starts_with("A - T ["));
}

#[quench_test::test]
fn a_disambiguated_long_name_keeps_its_title() {
    let artists: Vec<String> = (0..60).map(|i| format!("Artist Number {i:02}")).collect();
    let names: Vec<&str> = artists.iter().map(String::as_str).collect();
    let long = make_filename(&names, "A Title");
    let tagged = disambiguate(&long, "key");
    assert!(
        tagged.len() <= MAX_NAME && tagged.contains(" - A Title ["),
        "{tagged}"
    );
}

#[quench_test::test]
fn a_search_compares_letters_without_their_marks() {
    use songbook_core::plain;
    assert_eq!(plain("  Zażółć gęślą jaźń "), "zazolc gesla jazn");
    assert_eq!(plain("Beyoncé Björk Sigur Rós"), "beyonce bjork sigur ros");
    assert_eq!(plain("Straße Æon"), "strasse aeon");
    // A decomposed letter (a + combining ogonek) is the same as the composed one.
    assert_eq!(plain("a\u{328}"), plain("ą"));
    // Letters that are not Latin are only lowercased.
    assert_eq!(plain("Привет"), "привет");
    assert_eq!(plain("ą"), plain("a"));
}
