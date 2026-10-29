//! Forklift on a computer: the device's side of the backup for Linux. A computer has no media library and no battery
//! to wait for, so it backs up the folders the person chose; what comes back from a restore goes into those folders (or
//! a folder of its own). [`Linux`] is the [`Platform`] the shared controller asks; [`run_background`] is a backup with no
//! window, for a timer.

use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forklift_core::{Folder, Found, Remote, short_id};
use forklift_host::worker::{Answer, Shared, Worker, stored_session};
use forklift_host::{Controller, Device, Node, Platform, Target, UploadEvent};
use forklift_net::{Change, NetError, Storage, Store, get_with_token, put_with_token};

/// The largest file this host sends or fetches: it reads a file whole (the HTTP client has no streaming body), so a
/// bigger one would need that much memory, twice. A larger file is left out with the server's own answer for "too
/// large", which the backup does not try again.
pub const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;

/// Names an OS leaves around that nobody wants backed up.
const IGNORED: [&str; 3] = [".ds_store", "thumbs.db", "desktop.ini"];

/// What happens on other threads that the program's own thread must hear of.
pub enum Event {
    /// The answer to a request of the worker.
    Worker(u64, Answer),
    Upload(UploadEvent),
    Download {
        id: String,
        ok: bool,
    },
    /// A sign-in ended.
    Login(Result<(), String>),
    /// Something to tell the person.
    Notice(String),
}

/// How a thread tells the program's thread.
pub type Post = Arc<dyn Fn(Event) + Send + Sync>;

/// A call waiting for its time.
type Timer = (Instant, Box<dyn FnOnce()>);

/// How the platform asks to be called back later.
pub type Later = Box<dyn Fn(u64, Box<dyn FnOnce()>)>;

/// The device's side of the backup on Linux.
pub struct Linux {
    worker: Rc<Worker>,
    post: Post,
    store: Store,
    restore_dir: PathBuf,
    later: Later,
}

impl Linux {
    #[must_use]
    pub fn new(worker: Rc<Worker>, post: Post, store: Store, later: Later) -> Self {
        Self {
            worker,
            post,
            store,
            restore_dir: restore_dir(),
            later,
        }
    }
}

/// Where what has no folder of its own is restored to: `~/Forklift Restore`.
#[must_use]
pub fn restore_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("Forklift Restore")
}

/// A name for this computer's storage, from its host name.
#[must_use]
pub fn default_storage() -> String {
    let name = std::fs::read_to_string("/etc/hostname")
        .ok()
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_default();
    forklift_core::storage_name(&name)
}

/// The zone's offset from UTC in minutes, from `date +%z` (`+0200`).
#[must_use]
pub fn offset_minutes() -> i32 {
    let out = std::process::Command::new("date")
        .arg("+%z")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok());
    let Some(z) = out.map(|s| s.trim().to_string()).filter(|s| s.len() == 5) else {
        return 0;
    };
    let (sign, rest) = z.split_at(1);
    let (Ok(h), Ok(m)) = (rest[..2].parse::<i32>(), rest[2..].parse::<i32>()) else {
        return 0;
    };
    (if sign == "-" { -1 } else { 1 }) * (h * 60 + m)
}

/// What is directly in a folder: its files and its folders. Links are not followed (a link back up the tree would never
/// end) and the leftovers of an operating system are not files of the person's.
///
/// # Errors
/// The folder cannot be read.
pub fn list_dir(dir: &Path) -> Result<Vec<Node>, String> {
    let mut nodes = Vec::new();
    for entry in std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
    {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().to_string();
        if kind.is_symlink()
            || IGNORED.contains(&name.to_lowercase().as_str())
            || name.ends_with(".part")
        {
            continue;
        }
        let path = entry.path().to_string_lossy().to_string();
        if kind.is_dir() {
            nodes.push(Node::Dir(path));
        } else if kind.is_file() {
            let Ok(meta) = entry.metadata() else { continue };
            let modified_ms = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0));
            nodes.push(Node::File(Found {
                // The controller gives a folder's files a short id from the path.
                local_id: short_id(&path),
                name,
                size: meta.len(),
                modified_ms,
                uri: path,
            }));
        }
    }
    nodes.sort_by(|a, b| node_path(a).cmp(node_path(b)));
    Ok(nodes)
}

fn node_path(node: &Node) -> &str {
    match node {
        Node::Dir(path) => path,
        Node::File(f) => &f.uri,
    }
}

/// Sends one file whole and says what the server answered. A file over [`MAX_FILE_BYTES`] is answered `413`
/// without being read.
///
/// # Errors
/// The file cannot be read, or the server cannot be reached.
pub fn upload_file(path: &Path, url: &str, token: &str) -> Result<u16, String> {
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > MAX_FILE_BYTES {
        return Ok(413);
    }
    let body = std::fs::read(path).map_err(|e| e.to_string())?;
    put_with_token(url, token, body).map_err(|e| e.to_string())
}

/// A name that is safe to create in a folder: what the server stores is cleaned already, but a path from a server is not
/// to be trusted to stay inside a folder.
#[must_use]
pub fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | '\0'))
        .collect();
    let cleaned = cleaned.trim_start_matches('.').to_string();
    if cleaned.is_empty() {
        "file".to_string()
    } else {
        cleaned
    }
}

/// Where a restored file goes: photos and videos under the restore folder, a backed-up folder's files back into that
/// folder, anything else under a folder of its own name in the restore folder.
#[must_use]
pub fn restore_path(
    restore_dir: &Path,
    folders: &[Folder],
    target: &Target,
    name: &str,
) -> PathBuf {
    let dir = match target.kind {
        "video" => restore_dir.join("videos"),
        "folder" => folders
            .iter()
            .find(|f| f.name == target.folder)
            .map_or_else(
                || restore_dir.join(safe_name(&target.folder)),
                |f| PathBuf::from(&f.uri),
            ),
        _ => restore_dir.join("photos"),
    };
    dir.join(safe_name(name))
}

/// `path`, or the first of `name (2).ext`, `name (3).ext`... that does not exist (nothing is overwritten).
#[must_use]
pub fn unique(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (2..)
        .map(|n| path.with_file_name(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .unwrap_or_else(|| path.to_path_buf())
}

/// Fetches `url` and writes it to `destination` (a name that is free), by way of a `.part` file that is renamed when
/// it is whole.
///
/// # Errors
/// The server says no, cannot be reached, or the file cannot be written.
pub fn download_file(url: &str, token: &str, destination: &Path) -> Result<PathBuf, String> {
    let (status, body) = get_with_token(url, token).map_err(|e| e.to_string())?;
    if !(200..300).contains(&status) {
        return Err(format!("the server answered {status}"));
    }
    if body.len() as u64 > MAX_FILE_BYTES {
        return Err("the file is too large for this app".into());
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let target = unique(destination);
    let part = target.with_file_name(format!(
        "{}.part",
        target
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default()
    ));
    std::fs::write(&part, body).map_err(|e| e.to_string())?;
    std::fs::rename(&part, &target).map_err(|e| e.to_string())?;
    Ok(target)
}

/// Asks the person for a folder with the chooser the desktop has (zenity, kdialog or yad): its name and its path, or
/// nothing when they cancel.
///
/// # Errors
/// None of the choosers is installed.
pub fn pick_folder_blocking() -> Result<Option<(String, String)>, String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let choosers: [(&str, Vec<String>); 3] = [
        (
            "zenity",
            vec![
                "--file-selection".into(),
                "--directory".into(),
                "--title=Choose a folder to back up".into(),
            ],
        ),
        (
            "kdialog",
            vec![
                "--getexistingdirectory".into(),
                home,
                "--title".into(),
                "Choose a folder to back up".into(),
            ],
        ),
        (
            "yad",
            vec![
                "--file".into(),
                "--directory".into(),
                "--title=Choose a folder to back up".into(),
            ],
        ),
    ];
    for (program, args) in choosers {
        let Ok(output) = std::process::Command::new(program).args(&args).output() else {
            continue;
        };
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !output.status.success() || path.is_empty() {
            return Ok(None);
        }
        let name = Path::new(&path)
            .file_name()
            .map_or_else(|| "Folder".to_string(), |n| n.to_string_lossy().to_string());
        return Ok(Some((name, path)));
    }
    Err("To choose a folder, install zenity or kdialog".into())
}

/// Says something with a desktop notification (`notify-send`), when there is one to say it with.
pub fn notify_desktop(title: &str, body: &str) {
    let _ = std::process::Command::new("notify-send")
        .args(["--app-name=Forklift", title, body])
        .status();
}

impl Platform for Linux {
    fn media(
        &self,
        _kind: &'static str,
        _offset: u32,
        then: Box<dyn FnOnce(Result<Vec<Found>, String>)>,
    ) {
        then(Ok(Vec::new()));
    }

    fn folder(&self, uri: &str, then: Box<dyn FnOnce(Result<Vec<Node>, String>)>) {
        let dir = PathBuf::from(uri);
        self.worker.spawn(move || list_dir(&dir), then);
    }

    fn upload(&self, id: &str, uri: &str, url: &str, token: &str) {
        let (post, id, path, url, token) = (
            Arc::clone(&self.post),
            id.to_string(),
            PathBuf::from(uri),
            url.to_string(),
            token.to_string(),
        );
        std::thread::spawn(move || {
            post(Event::Upload(match upload_file(&path, &url, &token) {
                Ok(status) => UploadEvent::Done { id, status },
                Err(message) => UploadEvent::Failed { id, message },
            }));
        });
    }

    fn cancel(&self, _id: &str) {}

    fn changes(
        &self,
        storage: &str,
        since: i64,
        then: Box<dyn FnOnce(Result<Vec<Change>, NetError>)>,
    ) {
        self.worker.changes(storage, since, then);
    }

    fn listing(&self, storage: &str, then: Box<dyn FnOnce(Result<Vec<Remote>, NetError>)>) {
        self.worker.listing(storage, then);
    }

    fn quota(&self, storage: &str, then: Box<dyn FnOnce(Option<(u64, u64)>)>) {
        self.worker.quota(storage, then);
    }

    fn download(&self, id: &str, url: &str, token: &str, name: &str, target: &Target) {
        let folders = self.store.settings().folders;
        let destination = restore_path(&self.restore_dir, &folders, target, name);
        let (post, id, url, token) = (
            Arc::clone(&self.post),
            id.to_string(),
            url.to_string(),
            token.to_string(),
        );
        std::thread::spawn(move || {
            let ok = download_file(&url, &token, &destination).is_ok();
            post(Event::Download { id, ok });
        });
    }

    fn cancel_download(&self, _id: &str) {}

    fn storages(&self, then: Box<dyn FnOnce(Result<Vec<Storage>, NetError>)>) {
        self.worker.storages(then);
    }

    fn token(&self, force: bool, then: Box<dyn FnOnce(Result<String, NetError>)>) {
        self.worker.token(force, then);
    }

    fn device(&self, then: Box<dyn FnOnce(Device)>) {
        // A computer is on the network when it runs, and on power as far as anyone can tell.
        then(Device {
            online: true,
            wifi: true,
            charging: true,
        });
    }

    fn pick_folder(&self, then: Box<dyn FnOnce(Option<(String, String)>)>) {
        let post = Arc::clone(&self.post);
        self.worker
            .spawn(pick_folder_blocking, move |picked| match picked {
                Ok(folder) => then(folder),
                Err(why) => {
                    post(Event::Notice(why));
                    then(None);
                }
            });
    }

    fn now_ms(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
    }

    fn later(&self, ms: u64, f: Box<dyn FnOnce()>) {
        (self.later)(ms, f);
    }
}

/// How a background run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ran {
    /// Files the server took.
    pub sent: usize,
    /// Why nothing was done, if that was so.
    pub skipped: Option<&'static str>,
}

/// One backup with no window: scans the folders, sends what is new, and ends when nothing is left to do. For a timer.
///
/// # Errors
/// The data directory cannot be used.
pub fn run_background(store: &Store) -> io::Result<Ran> {
    let skipped = |why| {
        Ok(Ran {
            sent: 0,
            skipped: Some(why),
        })
    };
    let settings = store.settings();
    if !settings.auto {
        return skipped("automatic backup is off");
    }
    if !settings.storage_confirmed {
        return skipped("no storage chosen yet");
    }
    let Some(session) = stored_session(store, &settings.server) else {
        return skipped("not signed in");
    };
    let session: Shared = Arc::new(Mutex::new(Some(session)));

    let (tx, rx) = mpsc::channel::<Event>();
    let (worker_tx, post_tx) = (Mutex::new(tx.clone()), Mutex::new(tx));
    let worker = Worker::new(session, move |id, answer| {
        let _ = worker_tx.lock().unwrap().send(Event::Worker(id, answer));
    });
    let post: Post = Arc::new(move |event| {
        let _ = post_tx.lock().unwrap().send(event);
    });
    let timers: Rc<RefCell<Vec<Timer>>> = Rc::default();
    let queue = Rc::clone(&timers);
    let later: Later = Box::new(move |ms, f| {
        queue
            .borrow_mut()
            .push((Instant::now() + Duration::from_millis(ms), f))
    });
    let platform = Rc::new(Linux::new(Rc::clone(&worker), post, store.clone(), later));
    let controller = Controller::new(platform, store.clone(), Rc::new(|_| {}));
    controller.scan(false);

    let mut sent = 0;
    while !controller.is_idle() {
        let due: Vec<Box<dyn FnOnce()>> = {
            let mut timers = timers.borrow_mut();
            let now = Instant::now();
            let (ready, later): (Vec<_>, Vec<_>) = timers.drain(..).partition(|(at, _)| *at <= now);
            *timers = later;
            ready.into_iter().map(|(_, f)| f).collect()
        };
        for f in due {
            f();
        }
        let wait = timers
            .borrow()
            .iter()
            .map(|(at, _)| at.saturating_duration_since(Instant::now()))
            .min()
            .unwrap_or(Duration::from_secs(1))
            .min(Duration::from_secs(1));
        match rx.recv_timeout(wait) {
            Ok(Event::Worker(id, answer)) => worker.complete(id, answer),
            Ok(Event::Upload(event)) => {
                if matches!(
                    event,
                    UploadEvent::Done {
                        status: 200..=299,
                        ..
                    }
                ) {
                    sent += 1;
                }
                controller.upload_event(&event);
            }
            Ok(Event::Download { .. } | Event::Login(_) | Event::Notice(_)) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    controller.sign_out_keep_session();
    Ok(Ran {
        sent,
        skipped: None,
    })
}
