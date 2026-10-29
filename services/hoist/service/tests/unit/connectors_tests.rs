use hoist_service::connectors::aptoide::{parse_app, parse_body};
use hoist_service::connectors::fdroid_format::parse_index;
use quench_serde::{Value, json};

fn fixture() -> String {
    json!({
        "repo": {
            "address": "https://f-droid.org/repo",
            "categories": { "Internet": {"name": {"en-US": "Internet"}} },
        },
        "packages": {
            "org.fdroid.fdroid": {
                "metadata": {
                    "added": 1000,
                    "categories": ["Internet"],
                    "name": {"en-US": "F-Droid"},
                    "summary": {"en-US": "F-Droid client"},
                    "description": {"en-US": "The F-Droid client itself."},
                    "icon": {"en-US": {"name": "/icons/org.fdroid.fdroid.png"}},
                },
                "versions": {
                    "hash-old": {
                        "file": {"name": "/org.fdroid.fdroid_100.apk", "sha256": "aaa111", "size": 1000},
                        "manifest": {"versionName": "1.0", "versionCode": 100},
                    },
                    "hash-new": {
                        "file": {"name": "/org.fdroid.fdroid_200.apk", "sha256": "bbb222", "size": 2000},
                        "manifest": {"versionName": "2.0", "versionCode": 200},
                    },
                },
            },
            "com.example.nofile": {
                "metadata": {"name": {"en-US": "Broken"}},
                "versions": {"hash": {"manifest": {"versionName": "1.0", "versionCode": 1}}},
            },
        },
    })
    .to_string()
}

#[quench_test::test]
fn keeps_only_the_highest_version_code_per_package() {
    let (listings, _) = parse_index(&fixture()).unwrap();
    assert_eq!(listings.len(), 1);
    let app = &listings[0];
    assert_eq!(app.package_name, "org.fdroid.fdroid");
    assert_eq!((app.version_code, app.version_name.as_str()), (200, "2.0"));
    assert_eq!(app.sha256.as_deref(), Some("bbb222"));
    assert_eq!(app.size_bytes, 2000);
}

#[quench_test::test]
fn a_version_with_no_file_entry_is_skipped_entirely() {
    let (listings, _) = parse_index(&fixture()).unwrap();
    assert!(
        listings
            .iter()
            .all(|a| a.package_name != "com.example.nofile")
    );
}

#[quench_test::test]
fn metadata_and_categories_are_mapped() {
    let (listings, categories) = parse_index(&fixture()).unwrap();
    let app = &listings[0];
    assert_eq!(app.label, "F-Droid");
    assert_eq!(app.summary.as_deref(), Some("F-Droid client"));
    assert_eq!(
        app.description.as_deref(),
        Some("The F-Droid client itself.")
    );
    assert_eq!(app.category.as_deref(), Some("Internet"));
    assert_eq!(
        app.icon_url.as_deref(),
        Some("https://f-droid.org/repo/icons/org.fdroid.fdroid.png")
    );
    assert_eq!(
        app.download_url.as_deref(),
        Some("https://f-droid.org/repo/org.fdroid.fdroid_200.apk")
    );
    assert_eq!(categories.get("Internet"), Some(&"Internet".to_string()));
    assert!(parse_index("not json").is_err());
}

fn browse_row() -> Value {
    json!({
        "package": "com.google.android.apps.youtube.music",
        "name": "YouTube Music",
        "icon": "https://pool.img.aptoide.com/market2019arabic/icon.png",
        "file": { "vername": "9.37.54", "vercode": 93_754_240, "md5sum": "48bf4314f7c0b7daef06106ee09c0684", "filesize": 72_178_364 },
        "stats": { "downloads": 2_000_000_000_i64, "rating": {"avg": 4.67, "total": 3} },
    })
}

fn get_meta_row() -> Value {
    let mut row = browse_row();
    row["file"]["path"] = json!("https://pool.apk.aptoide.com/market2019arabic/youtube-music.apk");
    row["file"]["malware"] = json!({"rank": "TRUSTED"});
    row["media"] = json!({
        "summary": "Enjoy your favorite music.",
        "description": "YouTube Music brings you a new way to find music.",
    });
    row
}

#[quench_test::test]
fn a_browse_row_has_no_download_path_or_malware_verdict() {
    let app = parse_app(&browse_row(), false).unwrap();
    assert!(app.download_url.is_none() && app.malware_rank.is_none());
    assert_eq!(app.md5.as_deref(), Some("48bf4314f7c0b7daef06106ee09c0684"));
    assert_eq!(app.downloads, Some(2_000_000_000));
    assert_eq!(app.rating_avg, Some(4.67));
    assert_eq!(app.rating_count, Some(3));
    assert_eq!(app.version_code, 93_754_240);
}

#[quench_test::test]
fn media_is_ignored_unless_the_row_was_resolved() {
    let mut row = browse_row();
    row["media"] = json!({"summary": "should be ignored"});
    assert!(parse_app(&row, false).unwrap().summary.is_none());
    let full = parse_app(&get_meta_row(), true).unwrap();
    assert_eq!(full.summary.as_deref(), Some("Enjoy your favorite music."));
    assert_eq!(full.malware_rank.as_deref(), Some("TRUSTED"));
    assert!(
        full.download_url
            .as_deref()
            .unwrap()
            .ends_with("youtube-music.apk")
    );
}

#[quench_test::test]
fn a_row_without_a_package_or_a_version_is_not_a_listing() {
    assert!(parse_app(&json!({"name": "x"}), false).is_none());
    let mut row = browse_row();
    row["file"] = json!({});
    assert!(parse_app(&row, false).is_none());
    assert!(parse_body("{").is_err());
    assert_eq!(parse_body("{\"a\": 1}").unwrap()["a"].as_i64(), Some(1));
}
