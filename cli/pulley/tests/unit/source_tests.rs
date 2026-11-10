//! Whether a source is there: what each finding means, and the check against local directories and, with a stand-in for `ssh`,
//! against remote ones that are up, down, or up and empty.

use crate::env_support::{ENV_LOCK, EnvGuard};
use pulley::config::Job;
use pulley::rsync::remote_shell;
use pulley::source::{Availability, check, classify_probe, has_entries, probe_target};

/// A job that deletes, as the ones that need guarding do.
fn job(src: &str, marker: Option<&str>) -> Job {
    Job {
        id: "j".to_string(),
        desc: "desc".to_string(),
        src: src.to_string(),
        dest: "/dest".to_string(),
        delete: true,
        skip: Vec::new(),
        no_confirm: true,
        interval: Some(60),
        marker: marker.map(str::to_string),
    }
}

/// A job that only copies: an empty source cannot cost it anything.
fn keeping(src: &str, marker: Option<&str>) -> Job {
    Job {
        delete: false,
        ..job(src, marker)
    }
}

// ── What a finished check means ──────────────────────────────────────────────────────────────────────────────────

#[quench_test::test]
fn a_clean_listing_is_ready() {
    assert_eq!(classify_probe(Some(0), ""), Availability::Ready);
}

#[quench_test::test]
fn ssh_failing_is_unreachable_with_its_own_last_line_as_the_reason() {
    let stderr = "debug1: connecting\nssh: connect to host lorien port 22: No route to host\nrsync: connection unexpectedly closed (0 bytes received so far) [Receiver]\n";
    let found = classify_probe(
        Some(255),
        "ssh: connect to host lorien port 22: No route to host\n",
    );
    assert_eq!(
        found,
        Availability::Unreachable("ssh: connect to host lorien port 22: No route to host".into())
    );
    assert!(!found.is_ready());
    // The line that says why, not rsync's boilerplate after it.
    assert_eq!(
        classify_probe(Some(255), stderr).reason(),
        "debug1: connecting",
        "the first line that is not rsync's own summary"
    );
    let real = "ssh: connect to host lorien port 22: No route to host\nrsync: connection unexpectedly closed (0 bytes received so far) [Receiver]\nrsync error: unexplained error (code 255) at io.c(228) [Receiver=3.5.2]\n";
    assert_eq!(
        classify_probe(Some(255), real).reason(),
        "ssh: connect to host lorien port 22: No route to host"
    );
    // Only boilerplate: the last line is all there is.
    assert_eq!(
        classify_probe(
            Some(255),
            "rsync error: unexplained error (code 255) at io.c(228)\n"
        )
        .reason(),
        "rsync error: unexplained error (code 255) at io.c(228)"
    );
}

#[quench_test::test]
fn a_host_that_answers_without_the_thing_is_not_ready_rather_than_unreachable() {
    let stderr = "rsync: [sender] link_stat \"/data/projects\" failed: No such file or directory (2)\nrsync error: some files/attrs were not transferred (code 23)\n";
    assert!(matches!(
        classify_probe(Some(23), stderr),
        Availability::NotReady(_)
    ));
    let denied = "rsync: [sender] opendir \"/data\" failed: Permission denied (13)\n";
    assert!(matches!(
        classify_probe(Some(23), denied),
        Availability::NotReady(_)
    ));
}

#[quench_test::test]
fn anything_else_is_unreachable_and_says_what_rsync_said() {
    assert_eq!(
        classify_probe(Some(12), "rsync: command not found\n"),
        Availability::Unreachable("rsync exited with 12: rsync: command not found".into())
    );
    assert_eq!(
        classify_probe(None, ""),
        Availability::Unreachable("the check was interrupted".into())
    );
    assert_eq!(
        classify_probe(Some(255), "").reason(),
        "no message",
        "a failure with nothing to say still has a reason"
    );
}

#[quench_test::test]
fn two_findings_of_one_kind_are_the_same_news() {
    let a = Availability::Unreachable("one".into());
    let b = Availability::Unreachable("another".into());
    assert!(a.same_kind(&b));
    assert!(!a.same_kind(&Availability::NotReady("one".into())));
    assert!(!a.same_kind(&Availability::Ready));
}

#[quench_test::test]
fn the_check_lists_the_marker_else_the_directory_with_or_without_its_contents() {
    assert_eq!(
        probe_target("me@h:/data", Some(".ok"), false),
        "me@h:/data/.ok"
    );
    assert_eq!(
        probe_target("me@h:/data//", Some("./sub/.ok"), true),
        "me@h:/data/sub/.ok"
    );
    assert_eq!(
        probe_target("rsync://h/mod", Some(".ok"), false),
        "rsync://h/mod/.ok"
    );
    // A directory that must hold something is listed with a trailing slash, which makes rsync list what is in it.
    assert_eq!(probe_target("me@h:/data/", None, true), "me@h:/data/");
    assert_eq!(probe_target("me@h:/data", None, true), "me@h:/data/");
    // One that only has to be there, as itself.
    assert_eq!(probe_target("me@h:/data/", None, false), "me@h:/data");
}

#[quench_test::test]
fn a_listing_has_entries_when_it_shows_more_than_the_directory_itself() {
    let empty = "drwxr-xr-x             40 2026/11/10 14:36:12 .\n";
    assert!(!has_entries(empty));
    assert!(!has_entries(""));
    let full = "drwxr-xr-x             80 2026/11/10 14:36:12 .\n-rw-r--r--              2 2026/11/10 14:36:12 a.txt\n";
    assert!(has_entries(full));
    // Names with spaces, and one that is only dots and spaces, are names; the line for `.` alone is not.
    assert!(has_entries(
        "drwxr-xr-x  40 2026/11/10 14:36:12 .\ndrwxr-xr-x  60 2026/11/10 14:36:12 my project\n"
    ));
    assert!(has_entries(
        "drwxr-xr-x  40 2026/11/10 14:36:12 .\n-rw-r--r--   1 2026/11/10 14:36:12 x .\n"
    ));
    assert!(has_entries(
        "drwxr-xr-x  40 2026/11/10 14:36:12 .\n-rw-r--r--   1 2026/11/10 14:36:12 ..\n"
    ));
    // Sizes are right-aligned, so there can be many spaces before them.
    assert!(has_entries(
        "drwxr-xr-x          4,096 2026/11/10 14:36:12 .\n-rw-r--r--     12,345,678 2026/11/10 14:36:12 big.bin\n"
    ));
}

// ── A source on this machine ─────────────────────────────────────────────────────────────────────────────────────

#[quench_test::test]
fn a_local_source_that_deletes_must_hold_something() {
    let src = quench_core::tmp::TempDir::new().unwrap();
    let path = src.path().display().to_string();

    // Empty: what an unmounted drive looks like. A job that deletes does not run from it; one that only copies may.
    let found = check(&job(&path, None));
    assert!(
        matches!(&found, Availability::NotReady(why) if why.contains("empty")),
        "{found:?}"
    );
    assert_eq!(check(&keeping(&path, None)), Availability::Ready);

    std::fs::write(src.path().join("a.txt"), "x").unwrap();
    assert_eq!(check(&job(&path, None)), Availability::Ready);
}

#[quench_test::test]
fn a_marker_is_any_path_that_is_already_there() {
    let src = quench_core::tmp::TempDir::new().unwrap();
    let path = src.path().display().to_string();

    // Not there: not ready, whatever kind of job.
    for found in [
        check(&job(&path, Some("projects"))),
        check(&keeping(&path, Some("projects"))),
    ] {
        assert!(
            matches!(&found, Availability::NotReady(why) if why.contains("projects")),
            "{found:?}"
        );
    }

    // An existing directory will do; nothing has to be created for it. It also says the source is not empty.
    std::fs::create_dir(src.path().join("projects")).unwrap();
    assert_eq!(check(&job(&path, Some("projects"))), Availability::Ready);
    std::fs::write(src.path().join("projects/readme.md"), "x").unwrap();
    assert_eq!(
        check(&job(&path, Some("projects/readme.md"))),
        Availability::Ready
    );
}

#[quench_test::test]
fn a_local_source_that_is_not_there_is_not_ready() {
    let missing =
        std::env::temp_dir().join(format!("pulley-source-missing-{}", std::process::id()));
    let found = check(&job(&missing.display().to_string(), None));
    assert!(
        matches!(found, Availability::NotReady(ref why) if why.contains("does not exist")),
        "{found:?}"
    );
}

#[quench_test::test]
fn a_marker_that_could_point_outside_the_source_is_refused() {
    let src = quench_core::tmp::TempDir::new().unwrap();
    // A file that exists, outside the source: a marker may not reach it.
    std::fs::write(src.path().join("../pulley-outside-marker"), "").ok();
    let path = src.path().display().to_string();
    for marker in [
        "../pulley-outside-marker",
        "/etc/hostname",
        "a/../../x",
        " ",
    ] {
        let found = check(&job(&path, Some(marker)));
        assert!(
            matches!(found, Availability::NotReady(ref why) if why.contains("relative path")),
            "{marker:?}: {found:?}"
        );
    }
    std::fs::remove_file(src.path().join("../pulley-outside-marker")).ok();
}

// ── A source on another host, with a stand-in for ssh ───────────────────────────────────────────────────────────

/// A remote shell that never leaves this machine: `down` is a host that is off, any other name runs what rsync asks for
/// locally, as if it were the far end.
#[cfg(unix)]
fn fake_ssh(dir: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("fake-ssh");
    std::fs::write(
        &path,
        "#!/bin/sh\nhost=\"$1\"; shift\ncase \"$host\" in\n  down) echo \"ssh: connect to host down port 22: No route to host\" >&2; exit 255 ;;\nesac\nexec \"$@\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(unix)]
#[quench_test::test]
fn a_remote_source_is_reachable_unreachable_or_up_and_empty() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let scratch = quench_core::tmp::TempDir::new().unwrap();
    let shell = fake_ssh(scratch.path());
    let _rsh = EnvGuard::set("RSYNC_RSH", shell.to_str().unwrap());
    let data = scratch.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let data = data.display().to_string();

    // Up, and the directory is there: enough for a job that only copies.
    assert_eq!(
        check(&keeping(&format!("up:{data}"), None)),
        Availability::Ready
    );
    // Up, but not the directory.
    for job in [
        job(&format!("up:{data}-nope"), None),
        keeping(&format!("up:{data}-nope"), None),
    ] {
        let found = check(&job);
        assert!(matches!(found, Availability::NotReady(_)), "{found:?}");
    }

    // Up, the directory there and empty, as an unmounted drive's mount point is: a job that deletes does not run.
    let found = check(&job(&format!("up:{data}"), None));
    assert!(
        matches!(&found, Availability::NotReady(why) if why.contains("empty")),
        "{found:?}"
    );
    // With its data it does, and with a marker that is already there it does; one that is not there, it does not.
    std::fs::create_dir(std::path::Path::new(&data).join("projects")).unwrap();
    assert_eq!(
        check(&job(&format!("up:{data}"), None)),
        Availability::Ready
    );
    assert_eq!(
        check(&job(&format!("up:{data}"), Some("projects"))),
        Availability::Ready
    );
    let found = check(&job(&format!("up:{data}"), Some("absent")));
    assert!(matches!(found, Availability::NotReady(_)), "{found:?}");

    // Off: ssh cannot connect, whatever else is asked.
    for marker in [None, Some("projects")] {
        let found = check(&job(&format!("down:{data}"), marker));
        assert!(
            matches!(&found, Availability::Unreachable(why) if why.contains("No route to host")),
            "{found:?}"
        );
    }
}

#[cfg(unix)]
#[quench_test::test]
fn the_remote_shell_has_timeouts_unless_the_user_chose_one() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    {
        let _unset = EnvGuard::unset("RSYNC_RSH");
        let transfer = remote_shell(false).expect("a default shell");
        assert!(
            transfer.starts_with("ssh ") && transfer.contains("ConnectTimeout=10"),
            "{transfer}"
        );
        assert!(transfer.contains("ServerAliveInterval"), "{transfer}");
        assert!(
            !transfer.contains("BatchMode"),
            "a transfer may ask for a password: {transfer}"
        );

        let probe = remote_shell(true).expect("a default shell");
        assert!(
            probe.contains("BatchMode=yes") && probe.contains("ConnectTimeout=5"),
            "{probe}"
        );
    }
    let _custom = EnvGuard::set("RSYNC_RSH", "ssh -p 2222");
    assert_eq!(
        remote_shell(false),
        None,
        "the user's own choice is left alone"
    );
    assert_eq!(remote_shell(true), None);
}
