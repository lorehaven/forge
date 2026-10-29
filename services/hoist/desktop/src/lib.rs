//! Hoist on the desktop, without the window: where it keeps its session and the worker that talks to gatehouse and
//! warehouse for the screens (one thing at a time, answers as plain messages).

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
        }
    }

    #[must_use]
    pub const fn at(dir: PathBuf) -> Self {
        Self { dir }
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

/// Hands a downloaded file to the desktop (`xdg-open`: the installer or package manager that handles its type).
///
/// # Errors
/// `xdg-open` is not there or could not start.
pub fn open_with_desktop(path: &std::path::Path) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not open the file: {e}"))
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
    let mut session = stored.map(|(s, t)| Session::resume(&s, &t));
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    while let Ok(request) = requests.recv() {
        let answer = match request {
            Request::Login {
                server: url,
                username,
                password,
            } => match hoist_net::login(&url, &username, &password) {
                Ok(s) => {
                    store.write(&url, s.refresh_token());
                    server = url;
                    session = Some(s);
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
                    Msg::Catalog(
                        rows.into_iter()
                            .filter(|a| {
                                a.arch
                                    .as_deref()
                                    .is_none_or(|x| x == arch || x == "universal")
                            })
                            .collect(),
                    )
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
                    session = session.map(|s| Session::resume(&url, s.refresh_token()));
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
        if answers.send(answer).is_err() {
            return;
        }
    }
}
