use anyhow::{Context, Result};
use clap::Parser;
use crossterm::cursor::MoveTo;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use forge_toolbox::{
    ActionDispatch, App, KeyOutcome, PollOutcome, dispatch_action, handle_key, poll_in_flight,
    refresh_app_state, render, run_selected_action,
};
use quench_cli::prelude::{Tone, print_status, require_binary};
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

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("failed to enable raw mode")?;
        execute!(stdout(), EnterAlternateScreen).context("failed to enter alternate screen")?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(stdout(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
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

        if !event::poll(Duration::from_millis(80)).context("failed to poll terminal events")? {
            continue;
        }

        let evt = event::read().context("failed to read terminal event")?;
        let Event::Key(key) = evt else {
            continue;
        };

        if key.kind != KeyEventKind::Press {
            continue;
        }

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
    execute!(out, Clear(ClearType::All), MoveTo(0, 0)).context("failed to clear terminal")?;
    let (term_width, _) = crossterm::terminal::size().context("failed to read terminal size")?;

    for line in render(app, transient_status, term_width as usize) {
        out.write_all(line.as_bytes())?;
        out.write_all(b"\r\n")?;
    }

    out.flush().context("failed to flush terminal output")?;
    Ok(())
}
