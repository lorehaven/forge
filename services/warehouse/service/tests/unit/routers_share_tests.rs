//! Share links: what a link may reach, and what the page says, without a database. The lookup by token and the dynamic
//! storages need Postgres and are covered where that is available.

use std::path::PathBuf;
use warehouse_service::routers::share::{
    Entry, Found, clean_path, escape, inline_safe, join, render_listing, resolve_static, size_text,
    sort, static_entries,
};

/// `<temp>/storage/shared/{a.txt, sub/b.txt, .c.part}`, `<temp>/storage/secret.txt`, `<temp>/elsewhere/loot.txt`, and two
/// links out of `shared`: a file and a directory.
fn estate() -> (quench_core::tmp::TempDir, PathBuf, PathBuf) {
    let temp = quench_core::tmp::TempDir::new().expect("temp dir");
    let storage = temp.path().join("storage");
    let shared = storage.join("shared");
    let elsewhere = temp.path().join("elsewhere");
    std::fs::create_dir_all(shared.join("sub")).unwrap();
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::write(shared.join("a.txt"), b"alpha").unwrap();
    std::fs::write(shared.join("sub/b.txt"), b"bravo!").unwrap();
    std::fs::write(shared.join(".c.part"), b"staging").unwrap();
    std::fs::write(storage.join("secret.txt"), b"not shared").unwrap();
    std::fs::write(elsewhere.join("loot.txt"), b"outside").unwrap();
    std::os::unix::fs::symlink(storage.join("secret.txt"), shared.join("sibling-file")).unwrap();
    std::os::unix::fs::symlink(&elsewhere, shared.join("out-dir")).unwrap();
    (temp, storage, shared)
}

// ── Paths ───────────────────────────────────────────────────────────────────────────────────────────────────────

#[quench_test::test]
fn a_path_is_relative_and_plain_or_it_is_refused() {
    assert_eq!(clean_path(""), Some(String::new()));
    assert_eq!(clean_path("  "), Some(String::new()));
    assert_eq!(clean_path("a/b.txt"), Some("a/b.txt".to_string()));
    assert_eq!(clean_path("./a//b"), Some("a/b".to_string()));
    for bad in ["../x", "a/../../x", "/etc/passwd", "a/b\0c", "a\nb"] {
        assert_eq!(clean_path(bad), None, "{bad:?}");
    }
}

#[quench_test::test]
fn joining_leaves_out_what_is_empty() {
    assert_eq!(join("", ""), "");
    assert_eq!(join("a", ""), "a");
    assert_eq!(join("", "b"), "b");
    assert_eq!(join("a/", "b"), "a/b");
    assert_eq!(join("a", "b/c"), "a/b/c");
}

// ── What a link can reach ───────────────────────────────────────────────────────────────────────────────────────

#[quench_test::test]
async fn a_shared_directory_reaches_everything_below_it_at_any_depth() {
    let (_temp, storage, shared) = estate();
    assert_eq!(
        resolve_static(&storage, &shared, "").await,
        Found::Dir(shared.clone())
    );
    assert_eq!(
        resolve_static(&storage, &shared, "a.txt").await,
        Found::File(shared.join("a.txt"))
    );
    assert_eq!(
        resolve_static(&storage, &shared, "sub").await,
        Found::Dir(shared.join("sub"))
    );
    assert_eq!(
        resolve_static(&storage, &shared, "sub/b.txt").await,
        Found::File(shared.join("sub/b.txt"))
    );
}

#[quench_test::test]
async fn nothing_outside_the_shared_directory_is_reachable() {
    let (_temp, storage, shared) = estate();
    // Spelled out of it.
    for sub in ["../secret.txt", "sub/../../secret.txt", "/etc/passwd"] {
        assert_eq!(
            resolve_static(&storage, &shared, sub).await,
            Found::Missing,
            "{sub}"
        );
    }
    // Led out of it by a link, to a sibling in the same storage and to a directory elsewhere.
    assert_eq!(
        resolve_static(&storage, &shared, "sibling-file").await,
        Found::Missing
    );
    assert_eq!(
        resolve_static(&storage, &shared, "out-dir").await,
        Found::Missing
    );
    assert_eq!(
        resolve_static(&storage, &shared, "out-dir/loot.txt").await,
        Found::Missing
    );
    assert_eq!(
        resolve_static(&storage, &shared, "nothing").await,
        Found::Missing
    );
}

#[quench_test::test]
async fn a_share_whose_directory_is_a_link_out_of_the_storage_is_not_served() {
    let (temp, storage, _shared) = estate();
    let escape = storage.join("escape");
    std::os::unix::fs::symlink(temp.path().join("elsewhere"), &escape).unwrap();
    assert_eq!(resolve_static(&storage, &escape, "").await, Found::Missing);
    assert_eq!(
        resolve_static(&storage, &escape, "loot.txt").await,
        Found::Missing
    );
}

#[quench_test::test]
async fn a_listing_shows_directories_first_and_hides_staging_files_and_links_out() {
    let (_temp, _storage, shared) = estate();
    let (entries, truncated) = static_entries(&shared, &shared).await;
    assert!(!truncated);
    assert_eq!(
        entries,
        [
            Entry {
                name: "sub".into(),
                is_dir: true,
                size: 0
            },
            Entry {
                name: "a.txt".into(),
                is_dir: false,
                size: 5
            },
        ]
    );
}

// ── The page ────────────────────────────────────────────────────────────────────────────────────────────────────

#[quench_test::test]
fn what_is_written_into_the_page_is_escaped() {
    assert_eq!(
        escape(r#"<b a="1" b='2'>&"#),
        "&lt;b a=&quot;1&quot; b=&#39;2&#39;&gt;&amp;"
    );

    let entries = [
        Entry {
            name: "<script>alert(1)</script>".into(),
            is_dir: true,
            size: 0,
        },
        Entry {
            name: "a&b \"c\".txt".into(),
            is_dir: false,
            size: 3,
        },
    ];
    let page = render_listing("<img src=x onerror=alert(1)>", "", &entries, false);
    assert!(!page.contains("<script>alert"), "{page}");
    assert!(!page.contains("<img src=x"), "{page}");
    assert!(
        page.contains("&lt;script&gt;alert(1)&lt;/script&gt;/"),
        "{page}"
    );
    assert!(page.contains("a&amp;b &quot;c&quot;.txt"), "{page}");
    // The name travels in the link percent-encoded, not raw.
    assert!(
        page.contains("?path=%3Cscript%3Ealert%281%29%3C%2Fscript%3E"),
        "{page}"
    );
}

#[quench_test::test]
fn the_page_links_down_and_back_up_relative_to_the_share() {
    let entries = [
        Entry {
            name: "deep".into(),
            is_dir: true,
            size: 0,
        },
        Entry {
            name: "photo.png".into(),
            is_dir: false,
            size: 2048,
        },
        Entry {
            name: "page.html".into(),
            is_dir: false,
            size: 10,
        },
    ];
    let page = render_listing("Holiday", "2026/summer", &entries, false);
    assert!(page.contains(r#"<a href="?">Holiday</a> / <a href="?path=2026">2026</a> / <a href="?path=2026%2Fsummer">summer</a>"#), "{page}");
    assert!(
        page.contains(r#"<a href="?path=2026">&#8593; ..</a>"#),
        "{page}"
    );
    assert!(
        page.contains(r#"<a href="?path=2026%2Fsummer%2Fdeep">deep/</a>"#),
        "{page}"
    );
    assert!(page.contains("2.0 KB"), "{page}");
    // An image can be viewed in place; a page of HTML can only be saved.
    assert!(
        page.contains("?path=2026%2Fsummer%2Fphoto.png&amp;inline=1"),
        "{page}"
    );
    assert!(
        !page.contains("page.html&amp;inline=1") && !page.contains("page.html\">view"),
        "{page}"
    );

    // At the top there is no way further up; an empty share says so.
    let top = render_listing("Holiday", "", &[], false);
    assert!(
        !top.contains("&#8593;") && top.contains("Nothing here."),
        "{top}"
    );
    assert!(render_listing("Holiday", "", &entries, true).contains("Only the first"));
}

#[quench_test::test]
fn only_kinds_that_cannot_run_script_are_shown_in_place() {
    // Source files are `text/plain` in the type table: a script is shown as text and, with `nosniff`, never run.
    for safe in [
        "a.png",
        "a.JPG",
        "clip.mp4",
        "song.mp3",
        "doc.pdf",
        "notes.txt",
        "app.js",
    ] {
        assert!(inline_safe(safe), "{safe}");
    }
    for unsafe_ in [
        "a.svg", "a.html", "a.htm", "a.xml", "a.json", "noext", "a.exe",
    ] {
        assert!(!inline_safe(unsafe_), "{unsafe_}");
    }
}

#[quench_test::test]
fn sizes_and_ordering() {
    assert_eq!(size_text(0), "0 B");
    assert_eq!(size_text(1023), "1023 B");
    assert_eq!(size_text(1536), "1.5 KB");
    assert_eq!(size_text(5 * 1024 * 1024), "5.0 MB");

    let mut entries = vec![
        Entry {
            name: "b.txt".into(),
            is_dir: false,
            size: 1,
        },
        Entry {
            name: "Zed".into(),
            is_dir: true,
            size: 0,
        },
        Entry {
            name: "a.txt".into(),
            is_dir: false,
            size: 1,
        },
        Entry {
            name: "alpha".into(),
            is_dir: true,
            size: 0,
        },
    ];
    sort(&mut entries);
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["alpha", "Zed", "a.txt", "b.txt"]);
}
