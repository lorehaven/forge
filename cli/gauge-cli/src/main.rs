use forge_tui::console::{Tone, print_status};
use forge_tui::{ColorLevel, Theme};
use gauge_cli::REFRESH_SECS;
use gauge_cli::config::{FileConfig, config_path};
use gauge_cli::source::{Config, Flags, Source, server_of};
use gauge_cli::state::{App, Data, Effect, handle_key};
use gauge_cli::view::{frame, render};
use quench_args::{Parser, Subcommand};
use quench_cli::keys::Keys;
use quench_cli::term::{
    CLEAR_ALL, ENTER_ALTERNATE_SCREEN, HIDE_CURSOR, LEAVE_ALTERNATE_SCREEN, SHOW_CURSOR, size,
};
use quench_cli::tty::RawMode;
use quench_core::{Context, Result};
use std::io::{Write, stdout};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Parser)]
#[command(
    name = "gauge",
    version,
    about = "Interactive host metrics from Gauge, refreshed every few seconds"
)]
struct Cli {
    /// The server Gauge and Gatehouse are on, e.g. `https://ennor.ddns.net`. Defaults to `$GAUGE_URL`, then `url`
    /// in `~/.config/gauge/config.toml`.
    #[arg(long, global = true, value_name = "URL")]
    url: Option<String>,

    /// Gauge's path under the server. Defaults to `$GAUGE_PATH`, then `path` in the config file, else `/gauge`.
    #[arg(long, global = true, value_name = "PATH")]
    path: Option<String>,

    /// Account to sign in as, instead of the stored session. Defaults to `$GAUGE_USERNAME`.
    #[arg(long, global = true, value_name = "NAME")]
    username: Option<String>,

    /// Its password. Defaults to `$GAUGE_PASSWORD`.
    #[arg(long, global = true, value_name = "PASSWORD")]
    password: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Sign in once and keep the session in `~/.config/gauge/config.toml`, so `gauge` needs no arguments
    Login,
}

/// Raw mode and the alternate screen for as long as it lives; both are undone on drop.
struct TerminalGuard {
    _raw: Option<RawMode>,
}

impl TerminalGuard {
    fn enter() -> Result<Self> {
        let raw = RawMode::enable(0).context("failed to enable raw mode")?;
        let mut out = stdout();
        out.write_all(ENTER_ALTERNATE_SCREEN.as_bytes())
            .and_then(|()| out.write_all(HIDE_CURSOR.as_bytes()))
            .and_then(|()| out.flush())
            .context("failed to enter alternate screen")?;
        Ok(Self { _raw: raw })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut out = stdout();
        let _ = out.write_all(SHOW_CURSOR.as_bytes());
        let _ = out.write_all(LEAVE_ALTERNATE_SCREEN.as_bytes());
        let _ = out.flush();
    }
}

fn main() {
    let _ = quench_core::dotenv::load();
    if let Err(error) = run() {
        print_status(Tone::Error, "gauge", &error.to_string());
        std::process::exit(1);
    }
}

fn env(key: &str) -> Option<String> {
    Some(quench_core::env::get_or(key, "")).filter(|v| !v.is_empty())
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let file_path = config_path(env);
    let file = match &file_path {
        Some(path) => FileConfig::load(path)?,
        None => FileConfig::default(),
    };
    if matches!(cli.command, Some(Command::Login)) {
        return login(&cli, file, file_path);
    }
    let flags = Flags {
        url: cli.url,
        path: cli.path,
        username: cli.username,
        password: cli.password,
    };
    let config = Config::resolve(flags, &file, file_path, env)?;
    // Sign in before the screen is taken over, so a wrong password is an error line and not a blank screen.
    let source = Source::connect(&config)?;
    let (requests, results) = spawn_worker(source);
    let _guard = TerminalGuard::enter()?;
    event_loop(&requests, &results)
}

/// `gauge login`: signs in with the account's password and keeps the server and the session's token. The password is
/// not kept, and the token is never printed.
fn login(cli: &Cli, mut file: FileConfig, file_path: Option<std::path::PathBuf>) -> Result<()> {
    let path =
        file_path.context("cannot tell where the config file goes: set HOME or XDG_CONFIG_HOME")?;
    let server = server_of(cli.url.clone(), &file, &env)?;
    let pick = |flag: &Option<String>, key: &str| flag.clone().or_else(|| env(key));
    let username = pick(&cli.username, "GAUGE_USERNAME")
        .context("no account: pass --username or set GAUGE_USERNAME")?;
    let password = pick(&cli.password, "GAUGE_PASSWORD")
        .context("no password: pass --password or set GAUGE_PASSWORD")?;
    let session = gatehouse_client::login(&server, &username, &password)
        .map_err(|e| quench_core::err!("{e}"))?;
    file.url = Some(server);
    if let Some(p) = cli.path.clone().or_else(|| env("GAUGE_PATH")) {
        file.path = Some(p);
    }
    file.token = Some(session.refresh_token().to_string());
    file.save(&path)?;
    print_status(
        Tone::Info,
        "gauge",
        &format!(
            "signed in as {username}; session kept in {}",
            path.display()
        ),
    );
    Ok(())
}

/// The worker owns the session: one request is one fetch, so the screen never waits on the network.
fn spawn_worker(
    mut source: Source,
) -> (
    mpsc::Sender<Option<String>>,
    mpsc::Receiver<std::result::Result<Data, String>>,
) {
    let (request_tx, request_rx) = mpsc::channel::<Option<String>>();
    let (result_tx, result_rx) = mpsc::channel();
    thread::spawn(move || {
        while let Ok(open) = request_rx.recv() {
            let result = source.fetch(open.as_deref()).map_err(|e| e.to_string());
            if result_tx.send(result).is_err() {
                break;
            }
        }
    });
    (request_tx, result_rx)
}

fn unix_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

fn event_loop(
    requests: &mpsc::Sender<Option<String>>,
    results: &mpsc::Receiver<std::result::Result<Data, String>>,
) -> Result<()> {
    let mut app = App::new();
    let mut keys = Keys::stdin();
    let theme = Theme::from_env();
    let level = ColorLevel::detect();
    let mut painted = String::new();
    let mut painted_size = (0, 0);
    let mut tick = 0_usize;
    let mut last_request: Option<Instant> = None;
    let mut last_second = 0_u64;

    loop {
        let due = last_request.is_none_or(|at| at.elapsed() >= Duration::from_secs(REFRESH_SECS));
        if due && !app.loading && requests.send(app.detail.clone()).is_ok() {
            app.loading = true;
            last_request = Some(Instant::now());
        }
        while let Ok(result) = results.try_recv() {
            app.apply(result, unix_now());
        }

        // The clock text ("updated 4 s ago") and the spinner change with time, so a second ticking over repaints.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "seconds"
        )]
        let second = unix_now() as u64;
        if app.loading || app.fetched_at.is_none() || second != last_second {
            tick = tick.wrapping_add(1);
            last_second = second;
        }
        draw(
            &mut app,
            tick,
            &theme,
            level,
            &mut painted,
            &mut painted_size,
        )?;

        if !keys
            .poll(Duration::from_millis(100))
            .context("failed to poll terminal events")?
        {
            continue;
        }
        let key = keys.read().context("failed to read terminal event")?;
        match handle_key(&mut app, key) {
            Effect::Quit => return Ok(()),
            Effect::Refresh => last_request = None,
            Effect::Redraw | Effect::Nothing => {}
        }
    }
}

/// Paints the screen over the old one. An unchanged screen is not sent again; after a resize the terminal is cleared
/// first, since the old frame was laid out for another size.
fn draw(
    app: &mut App,
    tick: usize,
    theme: &Theme,
    level: ColorLevel,
    painted: &mut String,
    painted_size: &mut (u16, u16),
) -> Result<()> {
    let size_now = size().context("failed to read terminal size")?;
    let (width, height) = size_now;
    let rows = render(
        app,
        unix_now(),
        tick,
        usize::from(width).saturating_sub(1),
        usize::from(height),
        theme,
        level,
    );
    let resized = size_now != *painted_size;
    let next = frame(&rows, &theme.bg.background_escape(level));
    if next == *painted && !resized {
        return Ok(());
    }
    let mut out = stdout();
    if resized {
        out.write_all(CLEAR_ALL.as_bytes())?;
    }
    out.write_all(next.as_bytes())?;
    out.flush().context("failed to flush terminal output")?;
    *painted = next;
    *painted_size = size_now;
    Ok(())
}
