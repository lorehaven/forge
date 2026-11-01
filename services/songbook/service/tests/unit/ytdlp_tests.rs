use songbook_service::download::is_forbidden;
use songbook_service::ytdlp::{asset, checksum_for, program};

#[quench_test::test]
fn the_checksum_of_a_release_file_is_read_from_the_published_list() {
    let sums = "AAA111  yt-dlp\nBBB222  yt-dlp_musllinux\nccc333 *yt-dlp_musllinux_aarch64\n";
    assert_eq!(
        checksum_for(sums, "yt-dlp_musllinux").as_deref(),
        Some("bbb222")
    );
    assert_eq!(
        checksum_for(sums, "yt-dlp_musllinux_aarch64").as_deref(),
        Some("ccc333"),
        "binary-mode marker"
    );
    assert_eq!(checksum_for(sums, "yt-dlp_macos"), None);
    assert_eq!(checksum_for("", "yt-dlp"), None);
}

#[quench_test::test]
fn the_build_that_is_fetched_matches_the_c_library() {
    // The musl build does not run on glibc and the other way round.
    if let Some(name) = asset() {
        assert_eq!(
            name.starts_with("yt-dlp_musllinux"),
            cfg!(target_env = "musl"),
            "{name}"
        );
    }
}

#[quench_test::test]
fn without_a_copy_of_its_own_the_one_on_the_path_is_used() {
    assert_eq!(program().to_string_lossy(), "yt-dlp");
}

#[quench_test::test]
fn a_403_on_the_video_data_is_told_from_other_failures() {
    assert!(is_forbidden(
        "ERROR: unable to download video data: HTTP Error 403: Forbidden"
    ));
    assert!(!is_forbidden("ERROR: Video unavailable"));
    assert!(!is_forbidden("HTTP Error 429: Too Many Requests"));
}
