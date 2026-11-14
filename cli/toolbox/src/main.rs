use forge_toolbox::{
    ActionDispatch, App, KeyOutcome, PollOutcome, SelfUpdate, dispatch_action,
    fetch_latest_registry_version, frame, handle_key, installed_versions, plan_self_update,
    poll_in_flight, refresh_app_state, render_with, run_selected_action, self_update_command,
};
use forge_tui::console::{Tone, print_status};
use forge_tui::{ColorLevel, Theme};
use quench_args::{Parser, Subcommand};
use quench_cli::keys::Keys;
use quench_cli::prelude::require_binary;
use quench_cli::term::{
    CLEAR_ALL, ENTER_ALTERNATE_SCREEN, HIDE_CURSOR, LEAVE_ALTERNATE_SCREEN, SHOW_CURSOR, size,
};
use quench_cli::tty::RawMode;
use quench_core::{Context, Result};
use std::io::{Write, stdout};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(
    name = "forge-toolbox",
    version,
    about = "Interactive monitor and installer for Forge crates"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Install the newest forge-toolbox from the registry, replacing this one
    SelfUpdate,
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

fn main() -> Result<()> {
    let cli = Cli::parse();
    require_binary(
        "cargo",
        "toolbox drives cargo search/install against the workspace registry",
    )?;
    if matches!(cli.command, Some(Command::SelfUpdate)) {
        return self_update();
    }
    print_status(Tone::Info, "toolbox", "launching interactive monitor");
    run_repl()
}

/// `forge-toolbox self-update`: runs `cargo install` with its output on the terminal, since a build takes minutes.
fn self_update() -> Result<()> {
    let installed = installed_versions()?;
    let latest = fetch_latest_registry_version("forge-toolbox")?;
    match plan_self_update(installed.get("forge-toolbox"), latest.as_ref()) {
        SelfUpdate::UpToDate(version) => {
            print_status(
                Tone::Info,
                "toolbox",
                &format!("forge-toolbox {version} is up to date"),
            );
            Ok(())
        }
        SelfUpdate::NotPublished => quench_core::bail!("forge-toolbox is not in the registry"),
        SelfUpdate::Install(version) => {
            print_status(
                Tone::Info,
                "toolbox",
                &format!("installing forge-toolbox {version}"),
            );
            let status = self_update_command()
                .status()
                .context("failed to run cargo install")?;
            if !status.success() {
                quench_core::bail!("cargo install forge-toolbox failed ({status})");
            }
            print_status(
                Tone::Info,
                "toolbox",
                &format!("forge-toolbox {version} installed"),
            );
            Ok(())
        }
    }
}

fn run_repl() -> Result<()> {
    let mut app = refresh_app_state(0, "Ready")?;
    let _guard = TerminalGuard::enter()?;
    let mut keys = Keys::stdin();
    let mut dirty = true;
    let mut in_flight: Option<forge_toolbox::InFlightAction> = None;
    let theme = Theme::from_env();
    let level = ColorLevel::detect();
    let mut last = Painted::default();

    loop {
        if let Some(job) = in_flight.as_mut() {
            match poll_in_flight(job, Duration::from_millis(120)) {
                PollOutcome::Done { selected, message } => {
                    in_flight = None;
                    app = refresh_app_state(selected, &message)?;
                    dirty = true;
                }
                PollOutcome::Tick => dirty = true,
                PollOutcome::Pending => {}
            }
        }

        // The terminal can change size after the screen is up (a full-screen app takes over the window), and the
        // layout is centred on the width, so a resize must repaint.
        if size().is_ok_and(|now| now != last.size) {
            dirty = true;
        }

        if dirty {
            let transient_status = in_flight.as_ref().map(|job| {
                format!(
                    "{} {}",
                    forge_toolbox::spinner_frames()[job.spinner_idx],
                    job.label
                )
            });
            draw(&app, transient_status.as_deref(), &theme, level, &mut last)?;
            dirty = false;
        }

        if !keys
            .poll(Duration::from_millis(80))
            .context("failed to poll terminal events")?
        {
            continue;
        }

        let key = keys.read().context("failed to read terminal event")?;

        match handle_key(
            key.code,
            app.selected,
            app.statuses.len(),
            in_flight.is_some(),
            &app.statuses,
        ) {
            KeyOutcome::MoveUp => {
                app.selected = app.selected.saturating_sub(1);
                dirty = true;
            }
            KeyOutcome::MoveDown => {
                app.selected += 1;
                dirty = true;
            }
            KeyOutcome::Quit => break,
            KeyOutcome::Refresh => {
                app = refresh_app_state(app.selected, "Status refreshed")?;
                dirty = true;
            }
            KeyOutcome::Busy | KeyOutcome::Ignore => {}
            KeyOutcome::RunAction(req) => {
                let selected = app.selected;
                match dispatch_action(&req) {
                    ActionDispatch::Done(message) => {
                        app = refresh_app_state(selected, &message)?;
                        dirty = true;
                    }
                    ActionDispatch::Spawn { label } => {
                        let (tx, rx) = mpsc::channel::<Result<String, String>>();
                        thread::spawn(move || {
                            let result = run_selected_action(&req).map_err(|err| err.to_string());
                            let _ = tx.send(result);
                        });
                        in_flight = Some(forge_toolbox::InFlightAction {
                            rx,
                            selected,
                            spinner_idx: 0,
                            label,
                            last_tick: Instant::now(),
                        });
                        dirty = true;
                    }
                }
            }
        }
    }

    Ok(())
}

/// What is on the screen: the last frame sent and the terminal size it was drawn for.
#[derive(Debug, Default)]
struct Painted {
    frame: String,
    size: (u16, u16),
}

/// Paints the screen over the old one (see [`frame`]). An unchanged screen is not sent again; after a resize the
/// terminal is cleared first, since the old frame was laid out for another size.
fn draw(
    app: &App,
    transient_status: Option<&str>,
    theme: &Theme,
    level: ColorLevel,
    last: &mut Painted,
) -> Result<()> {
    let size_now = size().context("failed to read terminal size")?;
    let (term_width, term_height) = size_now;
    let rows = render_with(
        app,
        transient_status,
        usize::from(term_width).saturating_sub(1),
        usize::from(term_height),
        theme,
        level,
    );
    let resized = size_now != last.size;
    let painted = frame(&rows, &theme.bg.background_escape(level));
    if painted == last.frame && !resized {
        return Ok(());
    }
    let mut out = stdout();
    if resized {
        out.write_all(CLEAR_ALL.as_bytes())?;
    }
    out.write_all(painted.as_bytes())?;
    out.flush().context("failed to flush terminal output")?;
    *last = Painted {
        frame: painted,
        size: size_now,
    };
    Ok(())
}
