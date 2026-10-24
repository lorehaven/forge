use forge_toolbox::{
    ActionDispatch, App, KeyOutcome, PollOutcome, dispatch_action, handle_key, poll_in_flight,
    refresh_app_state, render, run_selected_action,
};
use quench_args::Parser;
use quench_cli::keys::Keys;
use quench_cli::prelude::{Tone, print_status, require_binary};
use quench_cli::term::{ENTER_ALTERNATE_SCREEN, LEAVE_ALTERNATE_SCREEN, clear_screen, size};
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
struct Cli {}

/// Raw mode and the alternate screen for as long as it lives; both are undone on drop.
struct TerminalGuard {
    _raw: Option<RawMode>,
}

impl TerminalGuard {
    fn enter() -> Result<Self> {
        let raw = RawMode::enable(0).context("failed to enable raw mode")?;
        let mut out = stdout();
        out.write_all(ENTER_ALTERNATE_SCREEN.as_bytes())
            .and_then(|()| out.flush())
            .context("failed to enter alternate screen")?;
        Ok(Self { _raw: raw })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut out = stdout();
        let _ = out.write_all(LEAVE_ALTERNATE_SCREEN.as_bytes());
        let _ = out.flush();
    }
}

fn main() -> Result<()> {
    let _ = Cli::parse();
    require_binary(
        "cargo",
        "toolbox drives cargo search/install against the workspace registry",
    )?;
    print_status(Tone::Info, "toolbox", "launching interactive monitor");
    run_repl()
}

fn run_repl() -> Result<()> {
    let mut app = refresh_app_state(0, "Ready")?;
    let _guard = TerminalGuard::enter()?;
    let mut keys = Keys::stdin();
    let mut dirty = true;
    let mut in_flight: Option<forge_toolbox::InFlightAction> = None;

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

        if dirty {
            let transient_status = in_flight.as_ref().map(|job| {
                format!(
                    "{} {}",
                    forge_toolbox::spinner_frames()[job.spinner_idx],
                    job.label
                )
            });
            draw(&app, transient_status.as_deref())?;
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
                match dispatch_action(req.clone()) {
                    ActionDispatch::Done(message) => {
                        app = refresh_app_state(selected, &message)?;
                        dirty = true;
                    }
                    ActionDispatch::Spawn { label } => {
                        let (tx, rx) = mpsc::channel::<Result<String, String>>();
                        thread::spawn(move || {
                            let result = run_selected_action(req).map_err(|err| err.to_string());
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

fn draw(app: &App, transient_status: Option<&str>) -> Result<()> {
    let mut out = stdout();
    clear_screen(&mut out).context("failed to clear terminal")?;
    let (term_width, _) = size().context("failed to read terminal size")?;

    for line in render(app, transient_status, term_width as usize) {
        out.write_all(line.as_bytes())?;
        out.write_all(b"\r\n")?;
    }

    out.flush().context("failed to flush terminal output")?;
    Ok(())
}
