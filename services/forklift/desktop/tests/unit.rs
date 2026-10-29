// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use forklift_core::{Folder, Settings, Status};
use forklift_desktop::{
    MAX_FILE_BYTES, download_file, list_dir, restore_path, run_background, safe_name, unique,
    upload_file,
};
use forklift_host::{Node, Target};
use forklift_net::{Store, upload_url};

fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("forklift-desktop-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

/// What the stand-in server saw.
#[derive(Clone, Debug)]
struct Seen {
    method: String,
    auth: String,
}

/// A gatehouse that numbers its tokens and a warehouse with one storage, `moto`, that keeps what is put in it.
type Stored = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

fn stand_in() -> (String, Arc<Mutex<Vec<Seen>>>, Stored) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let log: Arc<Mutex<Vec<Seen>>> = Arc::default();
    let stored: Stored = Arc::default();
    let (seen_log, kept) = (Arc::clone(&log), Arc::clone(&stored));
    let issued = Arc::new(Mutex::new(0u32));
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let (log, stored, issued) = (
                Arc::clone(&seen_log),
                Arc::clone(&kept),
                Arc::clone(&issued),
            );
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let mut parts = line.split_whitespace();
                let (method, target) = (
                    parts.next().unwrap().to_string(),
                    parts.next().unwrap().to_string(),
                );
                let (mut length, mut auth) = (0usize, String::new());
                loop {
                    let mut header = String::new();
                    reader.read_line(&mut header).unwrap();
                    if header.trim().is_empty() {
                        break;
                    }
                    let (name, value) = header.split_once(':').unwrap();
                    match name.to_ascii_lowercase().as_str() {
                        "content-length" => length = value.trim().parse().unwrap(),
                        "authorization" => auth = value.trim().to_string(),
                        _ => {}
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                log.lock().unwrap().push(Seen {
                    method: method.clone(),
                    auth,
                });
                let (status, reply): (u16, Vec<u8>) = if target.starts_with("/gatehouse/") {
                    let mut n = issued.lock().unwrap();
                    *n += 1;
                    (200, format!("{{\"access_token\":\"access-{n}\",\"refresh_token\":\"refresh-{n}\",\"expires_in\":900}}").into_bytes())
                } else if method == "PUT" {
                    let path = target.split("path=").nth(1).unwrap_or_default().to_string();
                    stored.lock().unwrap().push((path, body));
                    (200, b"{}".to_vec())
                } else if target.contains("/sync") {
                    (200, b"[]".to_vec())
                } else if target.contains("/file?path=") {
                    (200, b"restored bytes".to_vec())
                } else {
                    (
                        200,
                        br#"[{"name":"moto","owner":"me","quota_bytes":1000000,"used_bytes":10}]"#
                            .to_vec(),
                    )
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    reply.len()
                );
                let _ = stream.write_all(&reply);
            });
        }
    });
    (format!("http://{addr}"), log, stored)
}

#[quench_test::test]
fn a_folder_lists_its_files_and_folders_but_not_links_or_leftovers() {
    let root = dir("list");
    write(&root.join("a.txt"), b"a");
    write(&root.join("sub/b.txt"), b"bb");
    write(&root.join(".DS_Store"), b"x");
    write(&root.join("Thumbs.db"), b"x");
    write(&root.join("half.jpg.part"), b"x");
    std::os::unix::fs::symlink(&root, root.join("loop")).unwrap();
    let nodes = list_dir(&root).unwrap();
    let names: Vec<String> = nodes
        .iter()
        .map(|n| match n {
            Node::Dir(p) => format!(
                "dir:{}",
                Path::new(p).file_name().unwrap().to_string_lossy()
            ),
            Node::File(f) => format!("file:{}:{}", f.name, f.size),
        })
        .collect();
    assert_eq!(names, ["file:a.txt:1", "dir:sub"]);
    assert!(list_dir(&root.join("missing")).is_err());
    let _ = std::fs::remove_dir_all(root);
}

#[quench_test::test]
fn names_from_a_server_cannot_leave_their_folder() {
    assert_eq!(safe_name("../../etc/passwd"), "etcpasswd");
    assert_eq!(safe_name(".hidden"), "hidden");
    assert_eq!(safe_name(".."), "file");
    assert_eq!(safe_name("IMG_1.jpg"), "IMG_1.jpg");
}

#[quench_test::test]
fn a_restored_file_goes_to_its_folder_or_one_of_its_own_and_never_over_another() {
    let restore = PathBuf::from("/home/me/Forklift Restore");
    let docs = Folder {
        slug: "docs".into(),
        name: "Documents".into(),
        uri: "/home/me/Documents".into(),
        enabled: true,
        wifi_only: false,
    };
    let target = |kind, folder: &str| Target {
        kind,
        folder: folder.into(),
    };
    assert_eq!(
        restore_path(&restore, &[], &target("images", ""), "a.jpg"),
        restore.join("photos/a.jpg")
    );
    assert_eq!(
        restore_path(&restore, &[], &target("video", ""), "a.mp4"),
        restore.join("videos/a.mp4")
    );
    assert_eq!(
        restore_path(
            &restore,
            std::slice::from_ref(&docs),
            &target("folder", "Documents"),
            "r.pdf"
        ),
        PathBuf::from("/home/me/Documents/r.pdf")
    );
    assert_eq!(
        restore_path(&restore, &[docs], &target("folder", "Old Stuff"), "r.pdf"),
        restore.join("Old Stuff/r.pdf")
    );
    let root = dir("unique");
    write(&root.join("a.jpg"), b"1");
    write(&root.join("a (2).jpg"), b"2");
    assert_eq!(unique(&root.join("a.jpg")), root.join("a (3).jpg"));
    assert_eq!(unique(&root.join("new.jpg")), root.join("new.jpg"));
    let _ = std::fs::remove_dir_all(root);
}

#[quench_test::test]
fn a_file_is_sent_whole_and_a_huge_one_is_not_read() {
    let (base, log, stored) = stand_in();
    let root = dir("upload");
    write(&root.join("p.jpg"), b"picture");
    let url = upload_url(&base, "moto", "docs/1-p.jpg");
    assert_eq!(upload_file(&root.join("p.jpg"), &url, "tok").unwrap(), 200);
    assert_eq!(
        stored.lock().unwrap().as_slice(),
        [("docs%2F1-p.jpg".to_string(), b"picture".to_vec())]
    );
    assert_eq!(log.lock().unwrap().last().unwrap().auth, "Bearer tok");
    assert!(upload_file(&root.join("gone.jpg"), &url, "tok").is_err());
    // A file past the limit is answered "too large" without being read: a sparse file stands in for it.
    let huge = root.join("huge.bin");
    std::fs::File::create(&huge)
        .unwrap()
        .set_len(MAX_FILE_BYTES + 1)
        .unwrap();
    let before = log.lock().unwrap().len();
    assert_eq!(upload_file(&huge, &url, "tok").unwrap(), 413);
    assert_eq!(log.lock().unwrap().len(), before, "nothing was sent");
    let _ = std::fs::remove_dir_all(root);
}

#[quench_test::test]
fn a_file_is_fetched_into_a_free_name_through_a_part_file() {
    let (base, _log, _stored) = stand_in();
    let root = dir("download");
    let url = upload_url(&base, "moto", "photos/1-a.jpg");
    let first = download_file(&url, "tok", &root.join("sub/a.jpg")).unwrap();
    assert_eq!(
        (first.clone(), std::fs::read(&first).unwrap()),
        (root.join("sub/a.jpg"), b"restored bytes".to_vec())
    );
    let second = download_file(&url, "tok", &root.join("sub/a.jpg")).unwrap();
    assert_eq!(
        second,
        root.join("sub/a (2).jpg"),
        "the first is not overwritten"
    );
    assert!(!root.join("sub/a.jpg.part").exists());
    let _ = std::fs::remove_dir_all(root);
}

fn store_with(name: &str, server: &str, folders: Vec<Folder>) -> Store {
    let store = Store::new(dir(&format!("store-{name}")));
    store.set_settings(&Settings {
        server: server.into(),
        storage: "moto".into(),
        storage_confirmed: true,
        photos: false,
        videos: false,
        folders,
        ..Settings::default()
    });
    store.set_token("t0");
    store
}

fn folder(root: &Path) -> Folder {
    Folder {
        slug: "docs".into(),
        name: "Docs".into(),
        uri: root.to_string_lossy().to_string(),
        enabled: true,
        wifi_only: false,
    }
}

#[quench_test::test]
fn a_background_run_sends_the_new_files_of_the_folders_and_the_next_has_nothing_to_do() {
    let (base, log, stored) = stand_in();
    let root = dir("tree");
    write(&root.join("a.txt"), b"alpha");
    write(&root.join("deep/er/b.txt"), b"beta");
    write(&root.join(".DS_Store"), b"junk");
    let store = store_with("run", &base, vec![folder(&root)]);
    let ran = run_background(&store).unwrap();
    assert_eq!((ran.sent, ran.skipped), (2, None));
    let paths: Vec<String> = stored
        .lock()
        .unwrap()
        .iter()
        .map(|(p, _)| p.clone())
        .collect();
    assert_eq!(paths.len(), 2);
    assert!(
        paths.iter().all(|p| p.starts_with("custom%3Adocs%2F")),
        "{paths:?}"
    );
    assert!(
        paths.iter().any(|p| p.ends_with("-a.txt")) && paths.iter().any(|p| p.ends_with("-b.txt")),
        "{paths:?}"
    );
    assert!(
        log.lock()
            .unwrap()
            .iter()
            .any(|s| s.method == "PUT" && s.auth.starts_with("Bearer access-"))
    );
    assert_eq!(
        store
            .manifest()
            .items()
            .filter(|i| i.status == Status::Done)
            .count(),
        2
    );
    // A second run finds nothing new; a changed file goes again.
    let puts = |log: &Mutex<Vec<Seen>>| {
        log.lock()
            .unwrap()
            .iter()
            .filter(|s| s.method == "PUT")
            .count()
    };
    assert_eq!(run_background(&store).unwrap().sent, 0);
    assert_eq!(puts(&log), 2);
    write(&root.join("a.txt"), b"alpha, changed");
    assert_eq!(run_background(&store).unwrap().sent, 1);
    assert_eq!(puts(&log), 3);
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(store.dir());
}

#[quench_test::test]
fn a_background_run_does_nothing_when_off_unsigned_or_without_a_chosen_storage() {
    let root = dir("none");
    for (name, change) in [
        (
            "off",
            (|s: &mut Settings| s.auto = false) as fn(&mut Settings),
        ),
        ("unchosen", |s| s.storage_confirmed = false),
    ] {
        let store = store_with(name, "http://127.0.0.1:1", vec![folder(&root)]);
        let mut settings = store.settings();
        change(&mut settings);
        store.set_settings(&settings);
        let ran = run_background(&store).unwrap();
        assert_eq!((ran.sent, ran.skipped.is_some()), (0, true), "{name}");
        let _ = std::fs::remove_dir_all(store.dir());
    }
    let store = store_with("unsigned", "http://127.0.0.1:1", vec![folder(&root)]);
    store.clear_token();
    assert_eq!(
        run_background(&store).unwrap().skipped,
        Some("not signed in")
    );
    let _ = std::fs::remove_dir_all(store.dir());
    let _ = std::fs::remove_dir_all(root);
}

quench_test::main!();
