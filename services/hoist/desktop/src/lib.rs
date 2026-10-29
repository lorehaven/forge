//! Hoist on the desktop, without the window: where it keeps its session and the worker that talks to gatehouse and
//! warehouse for the screens (one thing at a time, answers as plain messages).

pub mod check;

use std::path::PathBuf;
use std::sync::mpsc;

use hoist_app::live::{Msg, Request};
use hoist_core::Installed;
use hoist_net::{NetError, Session};

pub const DEFAULT_SERVER: &str = "https://ennor.ddns.net";

/// What hoist keeps on this machine: the server and the refresh token (readable by this user only), and the
/// downloads.
#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
    /// The package formats to offer on Linux; the system's own (from `/etc/os-release`) when `None`.
    formats: Option<Vec<&'static str>>,
}

impl Store {
    /// `$XDG_DATA_HOME/hoist`, else `~/.local/share/hoist`.
    #[must_use]
    pub fn standard() -> Self {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .unwrap_or_else(std::env::temp_dir);
        Self {
            dir: base.join("hoist"),
            formats: None,
        }
    }

    #[must_use]
    pub const fn at(dir: PathBuf) -> Self {
        Self { dir, formats: None }
    }

    /// The package formats this system installs, best first: the ones asked for with
    /// [`with_formats`](Self::with_formats), else by the distribution (`/etc/os-release`).
    #[must_use]
    pub fn installable_formats(&self) -> Vec<&'static str> {
        self.formats.clone().unwrap_or_else(|| {
            package_formats(
                &std::fs::read_to_string("/etc/os-release")
                    .or_else(|_| std::fs::read_to_string("/usr/lib/os-release"))
                    .unwrap_or_default(),
            )
        })
    }

    /// Offers these package formats (best first) instead of the ones this system installs.
    #[must_use]
    pub fn with_formats(mut self, formats: Vec<&'static str>) -> Self {
        self.formats = Some(formats);
        self
    }

    fn file(&self) -> PathBuf {
        self.dir.join("session")
    }

    /// The server and refresh token stored by an earlier run.
    #[must_use]
    pub fn read(&self) -> Option<(String, String)> {
        let text = std::fs::read_to_string(self.file()).ok()?;
        let (server, token) = text.trim().split_once('\n')?;
        Some((server.to_string(), token.to_string()))
    }

    pub fn write(&self, server: &str, token: &str) {
        use std::os::unix::fs::OpenOptionsExt;
        let _ = std::fs::create_dir_all(&self.dir);
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(self.file());
        if let Ok(mut file) = file {
            let _ = std::io::Write::write_all(&mut file, format!("{server}\n{token}\n").as_bytes());
        }
    }

    pub fn clear(&self) {
        let _ = std::fs::remove_file(self.file());
    }

    fn ledger(&self) -> PathBuf {
        self.dir.join("installed")
    }

    /// What was handed to the system's installer, one line each: program, version code, version name.
    #[must_use]
    pub fn installed(&self) -> std::collections::BTreeMap<String, Installed> {
        let text = std::fs::read_to_string(self.ledger()).unwrap_or_default();
        text.lines()
            .filter_map(|line| {
                let mut parts = line.splitn(3, '\t');
                let program = parts.next()?.to_string();
                let version_code = parts.next()?.parse().ok()?;
                let version_name = parts.next()?.to_string();
                Some((
                    program.clone(),
                    Installed {
                        program,
                        version_code,
                        version_name,
                    },
                ))
            })
            .collect()
    }

    /// Remembers that `program` at this version was handed to the installer.
    pub fn record(&self, installed: &Installed) {
        let mut all = self.installed();
        all.insert(installed.program.clone(), installed.clone());
        let text: String = all
            .values()
            .map(|i| {
                format!(
                    "{}\t{}\t{}\n",
                    i.program,
                    i.version_code,
                    i.version_name.replace(['\t', '\n'], " ")
                )
            })
            .collect();
        let _ = std::fs::create_dir_all(&self.dir);
        let _ = std::fs::write(self.ledger(), text);
    }

    fn announced_file(&self) -> PathBuf {
        self.dir.join("announced")
    }

    /// The updates the user was last told about (see [`check::check`]); empty when there were none.
    #[must_use]
    pub fn announced(&self) -> String {
        std::fs::read_to_string(self.announced_file())
            .unwrap_or_default()
            .trim()
            .to_string()
    }

    /// Remembers which updates were announced.
    pub fn set_announced(&self, key: &str) {
        let _ = std::fs::create_dir_all(&self.dir);
        let _ = std::fs::write(self.announced_file(), format!("{key}\n"));
    }

    fn preferences_file(&self) -> PathBuf {
        self.dir.join("preferences")
    }

    /// Whether the background check for updates is switched on (it is, until the user says it is not).
    #[must_use]
    pub fn auto_check(&self) -> bool {
        std::fs::read_to_string(self.preferences_file())
            .ok()
            .is_none_or(|text| !text.lines().any(|l| l.trim() == "auto_check=false"))
    }

    /// Keeps the settings the user changed.
    pub fn save_preferences(&self, auto_check: bool) {
        let _ = std::fs::create_dir_all(&self.dir);
        let _ = std::fs::write(
            self.preferences_file(),
            format!("auto_check={auto_check}\n"),
        );
    }

    #[must_use]
    pub fn downloads(&self) -> PathBuf {
        self.dir.join("downloads")
    }
}

/// This running program as an installed one: it is installed, at the version it is, whatever a ledger of installs made
/// with it knows (a package manager may have put it there).
#[must_use]
pub fn this_app() -> Installed {
    let version = env!("CARGO_PKG_VERSION");
    Installed {
        program: hoist_core::SELF_PROGRAM.to_string(),
        version_code: hoist_core::version_code(version),
        version_name: version.to_string(),
    }
}

/// Tells the user on a desktop, with `notify-send` (libnotify): a notification of the desktop's own. Quietly nothing
/// where that is not installed.
pub fn notify_desktop(title: &str, body: &str) {
    let _ = std::process::Command::new("notify-send")
        .args([
            "--app-name=Hoist",
            "--icon=dev.lorehaven.hoist",
            "--hint=string:desktop-entry:dev.lorehaven.hoist",
        ])
        .arg(title)
        .arg(body)
        .stdin(std::process::Stdio::null())
        .status();
}

/// The package formats this Linux installs, best first, from the text of `/etc/os-release`: the registry holds a
/// build of each (`deb`, `rpm`, `pkg.tar.zst`), and the app offers the one the system's package manager takes. A
/// distribution it does not know gets the source tarball.
#[must_use]
pub fn package_formats(os_release: &str) -> Vec<&'static str> {
    let field = |name: &str| {
        os_release
            .lines()
            .find_map(|l| l.strip_prefix(name)?.strip_prefix('='))
            .map(|v| v.trim_matches('"').to_ascii_lowercase())
            .unwrap_or_default()
    };
    let family = format!("{} {}", field("ID"), field("ID_LIKE"));
    let is = |names: &[&str]| family.split_whitespace().any(|f| names.contains(&f));
    if is(&["arch", "manjaro", "endeavouros", "cachyos"]) {
        vec!["pkg.tar.zst"]
    } else if is(&["debian", "ubuntu", "linuxmint", "pop", "raspbian"]) {
        vec!["deb"]
    } else if is(&[
        "fedora",
        "rhel",
        "centos",
        "rocky",
        "almalinux",
        "suse",
        "opensuse",
    ]) {
        vec!["rpm"]
    } else {
        vec!["tar.gz"]
    }
}

/// `session`, set to write every refresh token into `store` the moment it is given. A refresh spends the old token,
/// so one that is only kept when the request after it succeeds is lost whenever that request fails or the app is
/// closed meanwhile - and the next start is signed out.
fn kept(session: Session, store: &Store, server: &str) -> Session {
    let (writes, reads, server) = (store.clone(), store.clone(), server.to_string());
    session
        .keeping_tokens(move |token| writes.write(&server, token))
        // The look for updates that runs while the window is closed (or open, in the background) shares the session:
        // whichever refreshed last kept the token to use.
        .following(move || reads.read().map(|(_, token)| token))
}

/// How a downloaded package is installed on this desktop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Install {
    /// Handed to whatever the desktop opens that type of file with (`xdg-open`): a software centre for a `.deb` or
    /// an `.rpm`. The install is the user's from there.
    Open,
    /// An Arch package: the desktop opens those with an archive manager, which only offers to unpack it, so the
    /// package manager is run (`pkexec pacman -U`, which asks for the password in the desktop's own dialog).
    Pacman,
}

/// What to do with `path`, by the kind of file it is.
#[must_use]
pub fn install_kind(path: &std::path::Path) -> Install {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if [".pkg.tar.zst", ".pkg.tar.xz", ".pkg.tar.gz"]
        .iter()
        .any(|suffix| name.ends_with(suffix))
    {
        Install::Pacman
    } else {
        Install::Open
    }
}

/// Installs a downloaded package the way this kind of file is installed (see [`Install`]).
///
/// # Errors
/// The program that installs it is not there, was refused, or failed: the message says what to run by hand.
pub fn open_with_desktop(path: &std::path::Path) -> Result<(), String> {
    match install_kind(path) {
        Install::Open => std::process::Command::new("xdg-open")
            .arg(path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("could not open the file: {e}")),
        Install::Pacman => {
            let by_hand = format!("sudo pacman -U {}", path.display());
            let output = std::process::Command::new("pkexec")
                .args(["pacman", "-U", "--noconfirm"])
                .arg(path)
                .stdin(std::process::Stdio::null())
                .output()
                .map_err(|e| format!("could not ask for permission ({e}): run `{by_hand}`"))?;
            if output.status.success() {
                return Ok(());
            }
            // The last thing it said is usually the reason; a cancelled password prompt says nothing.
            let said = String::from_utf8_lossy(&output.stderr);
            let reason = said
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("it was refused or cancelled");
            Err(format!(
                "pacman did not install it ({reason}): run `{by_hand}`"
            ))
        }
    }
}

/// Does what the app asks, one thing at a time, and says what came of it; returns when the app is gone.
pub fn work(requests: &mpsc::Receiver<Request>, answers: &mpsc::Sender<Msg>, store: &Store) {
    work_with(requests, answers, store, &open_with_desktop);
}

/// [`work`], with the way a downloaded file is handed to the system chosen by the caller.
pub fn work_with(
    requests: &mpsc::Receiver<Request>,
    answers: &mpsc::Sender<Msg>,
    store: &Store,
    open: &dyn Fn(&std::path::Path) -> Result<(), String>,
) {
    let stored = store.read();
    let mut server = stored
        .as_ref()
        .map_or_else(|| DEFAULT_SERVER.to_string(), |s| s.0.clone());
    let mut session = stored.map(|(s, t)| kept(Session::resume(&s, &t), store, &s));
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    // On Linux a program is published once in each package format; the app shows the one this system installs.
    let formats = (os == "linux").then(|| store.installable_formats());
    let trace = std::env::var_os("HOIST_TRACE").is_some();
    while let Ok(request) = requests.recv() {
        let answer = match request {
            Request::Login {
                server: url,
                username,
                password,
            } => match hoist_net::login(&url, &username, &password) {
                Ok(s) => {
                    store.write(&url, s.refresh_token());
                    session = Some(kept(s, store, &url));
                    server = url;
                    Msg::LoggedIn
                }
                Err(e) => Msg::LoginFailed(e.to_string()),
            },
            Request::Catalog => match session.as_mut().map(|s| {
                s.catalog(os)
                    .map(|rows| (rows, s.refresh_token().to_string()))
            }) {
                None | Some(Err(NetError::Unauthorized)) => Msg::SignedOut,
                Some(Ok((rows, token))) => {
                    // The refresh token rotates: keep the new one before anything else is asked.
                    store.write(&server, &token);
                    Msg::Catalog(check::offered(rows, arch, formats.as_deref()))
                }
                Some(Err(e)) => Msg::CatalogFailed(e.to_string()),
            },
            Request::Download(artifact) => match session
                .as_mut()
                .map(|s| s.download(&artifact, &store.downloads(), |_, _| {}))
            {
                None | Some(Err(NetError::Unauthorized)) => Msg::SignedOut,
                Some(Ok(path)) => {
                    if let Some(s) = &session {
                        store.write(&server, s.refresh_token());
                    }
                    Msg::Downloaded {
                        program: artifact.program,
                        path: path.display().to_string(),
                    }
                }
                Some(Err(e)) => Msg::DownloadFailed(e.to_string()),
            },
            Request::Install(artifact) => match session
                .as_mut()
                .map(|s| s.download(&artifact, &store.downloads(), |_, _| {}))
            {
                None | Some(Err(NetError::Unauthorized)) => Msg::SignedOut,
                Some(Ok(path)) => {
                    if let Some(s) = &session {
                        store.write(&server, s.refresh_token());
                    }
                    match open(&path) {
                        Ok(()) => {
                            let installed = Installed {
                                program: artifact.program.clone(),
                                version_code: artifact.version_code,
                                version_name: artifact.version_name.clone(),
                            };
                            store.record(&installed);
                            Msg::Installed {
                                program: installed.program,
                                version_code: installed.version_code,
                                version_name: installed.version_name,
                                path: path.display().to_string(),
                            }
                        }
                        Err(why) => {
                            Msg::DownloadFailed(format!("{why} (the file is {})", path.display()))
                        }
                    }
                }
                Some(Err(e)) => Msg::DownloadFailed(e.to_string()),
            },
            Request::Preferences {
                server: url,
                auto_check,
            } => {
                store.save_preferences(auto_check);
                if url != server {
                    // The session carries on at the new address (and is kept there for the next run).
                    session = session
                        .map(|s| kept(Session::resume(&url, s.refresh_token()), store, &url));
                    if let Some(s) = &session {
                        store.write(&url, s.refresh_token());
                    }
                    server = url;
                }
                continue;
            }
            // Only a phone has apps to start, and the public catalogue is Android's.
            Request::Launch(_) | Request::InstallPublic(_) => continue,
            Request::PublicPage(asked) => match session.as_mut().map(|s| {
                s.public_apps(&asked)
                    .map(|page| (page, s.refresh_token().to_string()))
            }) {
                None | Some(Err(NetError::Unauthorized)) => Msg::SignedOut,
                Some(Ok((page, token))) => {
                    store.write(&server, &token);
                    Msg::PublicPage { asked, page }
                }
                Some(Err(e)) => Msg::PublicFailed {
                    asked: Some(asked),
                    why: e.to_string(),
                },
            },
            Request::PublicCategories => match session.as_mut().map(|s| {
                s.public_categories()
                    .map(|rows| (rows, s.refresh_token().to_string()))
            }) {
                None | Some(Err(NetError::Unauthorized)) => Msg::SignedOut,
                Some(Ok((rows, token))) => {
                    store.write(&server, &token);
                    Msg::Categories(rows)
                }
                Some(Err(e)) => Msg::PublicFailed {
                    asked: None,
                    why: e.to_string(),
                },
            },
            Request::DownloadPublic(app) => {
                match Session::download_public(&app, &store.downloads(), |_, _| {}) {
                    Ok(path) => Msg::Downloaded {
                        program: app.package_name,
                        path: path.display().to_string(),
                    },
                    Err(e) => Msg::DownloadFailed(e.to_string()),
                }
            }
            Request::Logout => {
                store.clear();
                session = None;
                Msg::SignedOut
            }
        };
        if trace {
            // What the worker answered, cut short: the kind of answer and a few words, never a request.
            let said: String = format!("{answer:?}").chars().take(160).collect();
            eprintln!("hoist-worker: {said}");
        }
        if answers.send(answer).is_err() {
            return;
        }
    }
}
