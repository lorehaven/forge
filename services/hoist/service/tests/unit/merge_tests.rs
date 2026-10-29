use hoist_service::merge::*;
use hoist_service::models::*;
use std::collections::HashMap;

fn listing(package_name: &str, source: &str) -> RawListing {
    RawListing {
        source: source.to_string(),
        package_name: package_name.to_string(),
        label: "Example".to_string(),
        version_code: 1,
        version_name: "1.0".to_string(),
        size_bytes: 1000,
        download_url: Some("https://example.test/a.apk".to_string()),
        sha256: Some("abc".to_string()),
        ..Default::default()
    }
}

fn map(pairs: Vec<(&str, Vec<RawListing>)>) -> HashMap<String, Vec<RawListing>> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

mod dedupe_within_source_tests {
    use super::*;

    #[quench_test::test]
    fn single_listing_passes_through() {
        let listings = vec![listing("com.example.app", "fdroid")];
        let result = dedupe_within_source(listings.clone());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].package_name, listings[0].package_name);
    }

    #[quench_test::test]
    fn trusted_wins_over_higher_downloads() {
        let untrusted_but_popular = RawListing {
            downloads: Some(1_000_000),
            malware_rank: None,
            ..listing("com.example.app", "aptoide")
        };
        let trusted = RawListing {
            downloads: Some(10),
            malware_rank: Some("TRUSTED".to_string()),
            ..listing("com.example.app", "aptoide")
        };
        let result = dedupe_within_source(vec![untrusted_but_popular, trusted]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].malware_rank.as_deref(), Some("TRUSTED"));
    }

    #[quench_test::test]
    fn falls_back_to_downloads_when_neither_is_trusted() {
        let low = RawListing {
            downloads: Some(10),
            ..listing("com.example.app", "aptoide")
        };
        let high = RawListing {
            downloads: Some(500),
            ..listing("com.example.app", "aptoide")
        };
        let result = dedupe_within_source(vec![low, high]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].downloads, Some(500));
    }

    #[quench_test::test]
    fn falls_back_to_rating_count_when_downloads_tie() {
        let fewer = RawListing {
            downloads: Some(100),
            rating_count: Some(5),
            ..listing("com.example.app", "aptoide")
        };
        let more = RawListing {
            downloads: Some(100),
            rating_count: Some(200),
            ..listing("com.example.app", "aptoide")
        };
        let result = dedupe_within_source(vec![fewer, more]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].rating_count, Some(200));
    }

    #[quench_test::test]
    fn different_packages_both_survive() {
        let a = listing("com.example.a", "aptoide");
        let b = listing("com.example.b", "aptoide");
        let result = dedupe_within_source(vec![a, b]);
        assert_eq!(result.len(), 2);
    }

    #[quench_test::test]
    fn different_sources_for_the_same_package_both_survive() {
        let fdroid = listing("com.example.app", "fdroid");
        let aptoide = listing("com.example.app", "aptoide");
        let result = dedupe_within_source(vec![fdroid, aptoide]);
        assert_eq!(result.len(), 2);
    }
}

mod merge_all_tests {
    use super::*;

    #[quench_test::test]
    fn fdroid_only_package_gets_a_row() {
        let merged = merge_all(
            map(vec![("fdroid", vec![listing("com.example.app", "fdroid")])]),
            DEFAULT_SOURCE_PRIORITY,
        );
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].install_source, "fdroid");
        assert_eq!(merged[0].download_url, "https://example.test/a.apk");
    }

    #[quench_test::test]
    fn aptoide_only_package_still_gets_a_row() {
        let merged = merge_all(
            map(vec![(
                "aptoide",
                vec![RawListing {
                    download_url: Some("https://aptoide.test/a.apk".to_string()),
                    sha256: None,
                    md5: Some("deadbeef".to_string()),
                    rating_avg: Some(4.5),
                    rating_count: Some(100),
                    downloads: Some(5000),
                    malware_rank: Some("TRUSTED".to_string()),
                    ..listing("com.example.app", "aptoide")
                }],
            )]),
            DEFAULT_SOURCE_PRIORITY,
        );
        assert_eq!(merged.len(), 1);
        let row = &merged[0];
        assert_eq!(row.install_source, "aptoide");
        assert_eq!(row.md5.as_deref(), Some("deadbeef"));
        assert_eq!(row.rating_avg, Some(4.5));
        assert_eq!(row.malware_rank.as_deref(), Some("TRUSTED"));
    }

    #[quench_test::test]
    fn fdroid_wins_the_install_slot_over_aptoide() {
        let merged = merge_all(
            map(vec![
                (
                    "fdroid",
                    vec![RawListing {
                        download_url: Some("https://fdroid.test/a.apk".to_string()),
                        ..listing("com.example.app", "fdroid")
                    }],
                ),
                (
                    "aptoide",
                    vec![RawListing {
                        download_url: Some("https://aptoide.test/a.apk".to_string()),
                        md5: Some("deadbeef".to_string()),
                        sha256: None,
                        ..listing("com.example.app", "aptoide")
                    }],
                ),
            ]),
            DEFAULT_SOURCE_PRIORITY,
        );
        assert_eq!(merged[0].install_source, "fdroid");
        assert_eq!(merged[0].download_url, "https://fdroid.test/a.apk");
    }

    #[quench_test::test]
    fn aptoide_rating_and_downloads_surface_even_when_fdroid_wins_install() {
        let merged = merge_all(
            map(vec![
                ("fdroid", vec![listing("com.example.app", "fdroid")]),
                (
                    "aptoide",
                    vec![RawListing {
                        rating_avg: Some(4.2),
                        rating_count: Some(900),
                        downloads: Some(123456),
                        ..listing("com.example.app", "aptoide")
                    }],
                ),
            ]),
            DEFAULT_SOURCE_PRIORITY,
        );
        let row = &merged[0];
        assert_eq!(row.install_source, "fdroid");
        assert_eq!(row.rating_avg, Some(4.2));
        assert_eq!(row.rating_count, Some(900));
        assert_eq!(row.downloads, Some(123456));
    }

    #[quench_test::test]
    fn malware_rank_is_absent_when_fdroid_wins_the_install_slot() {
        let merged = merge_all(
            map(vec![
                ("fdroid", vec![listing("com.example.app", "fdroid")]),
                (
                    "aptoide",
                    vec![RawListing {
                        malware_rank: Some("TRUSTED".to_string()),
                        ..listing("com.example.app", "aptoide")
                    }],
                ),
            ]),
            DEFAULT_SOURCE_PRIORITY,
        );
        assert_eq!(merged[0].install_source, "fdroid");
        assert_eq!(merged[0].malware_rank, None);
    }

    #[quench_test::test]
    fn category_prefers_fdroid_over_aptoide() {
        let merged = merge_all(
            map(vec![
                (
                    "fdroid",
                    vec![RawListing {
                        category: Some("Internet".to_string()),
                        ..listing("com.example.app", "fdroid")
                    }],
                ),
                (
                    "aptoide",
                    vec![RawListing {
                        category: Some("Should not win".to_string()),
                        ..listing("com.example.app", "aptoide")
                    }],
                ),
            ]),
            DEFAULT_SOURCE_PRIORITY,
        );
        assert_eq!(merged[0].category.as_deref(), Some("Internet"));
    }

    #[quench_test::test]
    fn package_with_no_resolved_download_anywhere_is_dropped() {
        let merged = merge_all(
            map(vec![(
                "aptoide",
                vec![RawListing {
                    download_url: None,
                    sha256: None,
                    ..listing("com.example.app", "aptoide")
                }],
            )]),
            DEFAULT_SOURCE_PRIORITY,
        );
        assert!(merged.is_empty());
    }

    #[quench_test::test]
    fn izzyondroid_is_the_fallback_when_fdroid_has_no_listing() {
        let merged = merge_all(
            map(vec![
                (
                    "izzyondroid",
                    vec![RawListing {
                        download_url: Some("https://izzy.test/a.apk".to_string()),
                        ..listing("com.example.app", "izzyondroid")
                    }],
                ),
                (
                    "aptoide",
                    vec![RawListing {
                        download_url: Some("https://aptoide.test/a.apk".to_string()),
                        ..listing("com.example.app", "aptoide")
                    }],
                ),
            ]),
            DEFAULT_SOURCE_PRIORITY,
        );
        assert_eq!(merged[0].install_source, "izzyondroid");
    }

    #[quench_test::test]
    fn within_aptoide_dedup_still_applies_during_a_full_merge() {
        let merged = merge_all(
            map(vec![(
                "aptoide",
                vec![
                    RawListing {
                        downloads: Some(10),
                        malware_rank: None,
                        ..listing("com.example.app", "aptoide")
                    },
                    RawListing {
                        downloads: Some(999),
                        malware_rank: Some("TRUSTED".to_string()),
                        ..listing("com.example.app", "aptoide")
                    },
                ],
            )]),
            DEFAULT_SOURCE_PRIORITY,
        );
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].downloads, Some(999));
    }
}
