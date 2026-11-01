use crate::support::*;
use songbook_service::convert::{ffmpeg_args, media_files, same_length};
use songbook_service::download::output_template;
use std::path::Path;
use std::process::Command;

#[quench_test::test]
fn media_files_are_found_below_the_folder_and_temporary_ones_ignored() {
    let dir = Scratch::new();
    for name in [
        "A - B.webm",
        "A - C.mp3",
        "A - D.mp3.part",
        "notes.txt",
        "A - E.webm.part",
    ] {
        std::fs::write(dir.path().join(name), b"x").unwrap();
    }
    std::fs::create_dir(dir.path().join("old")).unwrap();
    std::fs::write(dir.path().join("old").join("X - Y.MKV"), b"x").unwrap();
    let found: Vec<String> = media_files(dir.path())
        .iter()
        .map(|p| p.strip_prefix(dir.path()).unwrap().display().to_string())
        .collect();
    assert_eq!(found, vec!["A - B.webm", "A - C.mp3", "old/X - Y.MKV"]);
}

#[quench_test::test]
fn the_ffmpeg_command_makes_a_vbr_mp3_from_the_first_audio_stream() {
    let args = ffmpeg_args(Path::new("in.webm"), Path::new("out.mp3.part"));
    let joined = args.join(" ");
    assert!(joined.contains("-i in.webm -vn -map 0:a:0"), "{joined}");
    assert!(
        joined.contains("-codec:a libmp3lame -q:a 0 -f mp3"),
        "{joined}"
    );
    assert!(joined.ends_with("out.mp3.part"), "{joined}");
}

#[quench_test::test]
fn a_percent_in_a_file_name_is_not_a_template_field() {
    assert_eq!(
        output_template(Path::new("/music"), "100% Pure - Love"),
        "/music/100%% Pure - Love.%(ext)s"
    );
}

fn tone(path: &Path, seconds: u32) -> bool {
    Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
        ])
        .arg(format!("sine=frequency=440:duration={seconds}"))
        .arg(path)
        .status()
        .is_ok_and(|s| s.success())
}

#[quench_test::test]
async fn convert_makes_the_mp3_removes_the_source_and_prunes_only_what_matches() {
    let dir = Scratch::new();
    let probe = dir.path().join("probe.mp3");
    if !tone(&probe, 1) {
        eprintln!("ffmpeg is not available: skipping");
        return;
    }
    std::fs::remove_file(&probe).unwrap();
    // a song only as a webm, a song with a webm leftover next to a complete mp3, and one whose mp3 is a different length
    assert!(tone(&dir.path().join("Only - Webm.webm"), 2));
    assert!(tone(&dir.path().join("Both - Same.webm"), 2));
    assert!(tone(&dir.path().join("Both - Same.mp3"), 2));
    assert!(tone(&dir.path().join("Both - Short.webm"), 6));
    assert!(tone(&dir.path().join("Both - Short.mp3"), 1));
    // a video-only file has nothing to convert
    let silent = dir.path().join("Video - Only.mp4");
    assert!(
        Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=64x64:d=1",
                "-an"
            ])
            .arg(&silent)
            .status()
            .unwrap()
            .success()
    );

    assert_eq!(
        same_length(
            &dir.path().join("Both - Same.webm"),
            &dir.path().join("Both - Same.mp3")
        )
        .await,
        Some(true)
    );
    assert_eq!(
        same_length(
            &dir.path().join("Both - Short.webm"),
            &dir.path().join("Both - Short.mp3")
        )
        .await,
        Some(false)
    );

    let jobs = songbook_service::jobs::Jobs::default();
    let root = dir.path().to_path_buf();
    assert!(jobs.start("convert", move |log| async move {
        songbook_service::convert::run(&root, true, &log).await
    }));
    while jobs.running() {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let job = jobs.read(0);
    assert_eq!(job.error, None, "{}", job.log);
    let has = |name: &str| dir.path().join(name).is_file();
    assert!(
        has("Only - Webm.mp3") && !has("Only - Webm.webm"),
        "{}",
        job.log
    );
    assert!(
        has("Both - Same.mp3") && !has("Both - Same.webm"),
        "the matching leftover is removed: {}",
        job.log
    );
    assert!(
        has("Both - Short.webm") && has("Both - Short.mp3"),
        "the different length is kept: {}",
        job.log
    );
    assert!(
        has("Video - Only.mp4") && !has("Video - Only.mp3"),
        "{}",
        job.log
    );
    assert!(job.log.contains("no audio stream"), "{}", job.log);
    assert!(
        !std::fs::read_dir(dir.path()).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".part"))
    );
}
