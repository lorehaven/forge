use anyhow::{Context, Result};
use clap::Parser;
use crossterm::cursor::MoveTo;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::style::Stylize;
use crossterm::terminal::{
    Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use forge_toolbox::{
    CrateStatus, REGISTRY_INDEX, REGISTRY_NAME, action_label, action_request, collect_statuses,
    content_widths, display_installed, display_latest, display_updatable, fit_cell,
    installed_versions, make_border, planned_action, run_selected_action, shrink_widths_to_fit,
    spinner_frames, toolbox_note,
};
use quench_cli::prelude::{Tone, print_status, require_binary};
use std::io::{Write, stdout};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(
    name = "forge-toolbox",
    version,
    about = "Interactive monitor and installer for Forge crates"
)]
struct Cli {}

struct App {
    selected: usize,
    statuses: Vec<CrateStatus>,
    toolbox_note: String,
    message: String,
}

struct InFlightAction {
    rx: Receiver<Result<String, String>>,
    selected: usize,
    spinner_idx: usize,
    label: String,
    last_tick: Instant,
}

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
    let mut in_flight: Option<InFlightAction> = None;

    loop {
        if let Some(job) = in_flight.as_mut() {
            match job.rx.try_recv() {
                Ok(result) => {
                    let message = match result {
                        Ok(msg) => msg,
                        Err(err) => format!("action failed: {err}"),
                    };
                    let selected = job.selected;
                    in_flight = None;
                    app = refresh_app_state(selected, &message)?;
                    dirty = true;
                }
                Err(TryRecvError::Empty) => {
                    if job.last_tick.elapsed() >= Duration::from_millis(120) {
                        job.spinner_idx = (job.spinner_idx + 1) % spinner_frames().len();
                        job.last_tick = Instant::now();
                        dirty = true;
                    }
                }
                Err(TryRecvError::Disconnected) => {
                    let selected = job.selected;
                    in_flight = None;
                    app = refresh_app_state(selected, "action failed: worker disconnected")?;
                    dirty = true;
                }
            }
        }

        if dirty {
            let transient_status = in_flight
                .as_ref()
                .map(|job| format!("{} {}", spinner_frames()[job.spinner_idx], job.label));
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

        match key.code {
            KeyCode::Up => {
                app.selected = app.selected.saturating_sub(1);
                dirty = true;
            }
            KeyCode::Down if app.selected + 1 < app.statuses.len() => {
                app.selected += 1;
                dirty = true;
            }
            KeyCode::Char('q') => break,
            KeyCode::Char('r') => {
                if in_flight.is_some() {
                    continue;
                }
                app = refresh_app_state(app.selected, "Status refreshed")?;
                dirty = true;
            }
            KeyCode::Enter => {
                if in_flight.is_some() {
                    continue;
                }
                let selected = app.selected;
                let req = action_request(&app.statuses[selected]);
                let Some(planned) = planned_action(&req) else {
                    let message = run_selected_action(req)
                        .unwrap_or_else(|err| format!("action failed: {err}"));
                    app = refresh_app_state(selected, &message)?;
                    dirty = true;
                    continue;
                };
                let label = match planned {
                    forge_toolbox::PlannedAction::Install => {
                        format!("running {} (installing)", req.package)
                    }
                    forge_toolbox::PlannedAction::Update => {
                        format!("running {} (updating)", req.package)
                    }
                };
                let (tx, rx) = mpsc::channel::<Result<String, String>>();
                thread::spawn(move || {
                    let result = run_selected_action(req).map_err(|err| err.to_string());
                    let _ = tx.send(result);
                });
                in_flight = Some(InFlightAction {
                    rx,
                    selected,
                    spinner_idx: 0,
                    label,
                    last_tick: Instant::now(),
                });
                dirty = true;
            }
            _ => {}
        }
    }

    Ok(())
}

fn refresh_app_state(selected: usize, message: &str) -> Result<App> {
    let installed = installed_versions()?;
    let statuses = collect_statuses(&installed);
    let toolbox_note = toolbox_note(&installed);

    let safe_selected = if statuses.is_empty() {
        0
    } else {
        selected.min(statuses.len() - 1)
    };

    Ok(App {
        selected: safe_selected,
        statuses,
        toolbox_note,
        message: message.to_string(),
    })
}

fn draw(app: &App, transient_status: Option<&str>) -> Result<()> {
    let mut out = stdout();
    execute!(out, Clear(ClearType::All), MoveTo(0, 0)).context("failed to clear terminal")?;
    let (term_width, _) = crossterm::terminal::size().context("failed to read terminal size")?;
    let usable_width = (term_width as usize).saturating_sub(1);
    let mut widths = content_widths(&app.statuses);
    shrink_widths_to_fit(&mut widths, usable_width);
    let row_len = widths.package
        + widths.binary
        + widths.installed
        + widths.latest
        + widths.updatable
        + widths.action
        + 10;
    let sep_len = row_len.min(usable_width);

    let mut line = |s: String| -> Result<()> {
        out.write_all(s.as_bytes())?;
        out.write_all(b"\r\n")?;
        Ok(())
    };

    line(format!("{}", "Forge Toolbox Status".bold().underlined()))?;
    line(String::new())?;
    line(format!(
        "{} {} {}",
        "registry:".dark_grey(),
        REGISTRY_NAME.cyan().bold(),
        format!("({REGISTRY_INDEX})").dark_grey()
    ))?;
    line(format!("{}", app.toolbox_note.as_str().yellow()))?;
    line(format!(
        "{} {}",
        "controls:".dark_grey(),
        "↑/↓ move, Enter install/update, r refresh, q quit".cyan()
    ))?;
    line(String::new())?;

    let top = make_border('┌', '┬', '┐', 2, &widths);
    let mid = make_border('├', '┼', '┤', 2, &widths);
    let bottom = make_border('└', '┴', '┘', 2, &widths);
    let top = fit_cell(&top, sep_len);
    let mid = fit_cell(&mid, sep_len);
    let bottom = fit_cell(&bottom, sep_len);

    line(format!("{}", top.dark_cyan()))?;
    line(format!(
        "│{}│{}│{}│{}│{}│{}│{}│",
        fit_cell("", 2).dark_cyan(),
        fit_cell("package", widths.package).bold().white(),
        fit_cell("binary", widths.binary).bold().white(),
        fit_cell("installed", widths.installed).bold().white(),
        fit_cell("latest", widths.latest).bold().white(),
        fit_cell("updatable", widths.updatable).bold().white(),
        fit_cell("action", widths.action).bold().white(),
    ))?;
    line(format!("{}", mid.dark_cyan()))?;

    for (idx, status) in app.statuses.iter().enumerate() {
        let marker = if idx == app.selected { ">" } else { " " };
        let installed = display_installed(status);
        let latest = display_latest(status);
        let updatable = display_updatable(status);
        let action = action_label(status);
        let marker = if idx == app.selected {
            fit_cell(marker, 2).green().bold().to_string()
        } else {
            fit_cell(marker, 2).dark_grey().to_string()
        };

        let installed_cell = match (&status.installed_version, status.installable) {
            (Some(_), _) => fit_cell(&installed, widths.installed).green().to_string(),
            (None, true) => fit_cell(&installed, widths.installed).yellow().to_string(),
            (None, false) => fit_cell(&installed, widths.installed)
                .dark_grey()
                .to_string(),
        };
        let latest_cell = if status.latest_version.is_some() {
            fit_cell(&latest, widths.latest).cyan().to_string()
        } else {
            fit_cell(&latest, widths.latest).dark_grey().to_string()
        };
        let updatable_cell = if status.updatable {
            fit_cell(updatable, widths.updatable)
                .yellow()
                .bold()
                .to_string()
        } else {
            fit_cell(updatable, widths.updatable)
                .dark_grey()
                .to_string()
        };
        let action_cell = match action {
            "install" => fit_cell(action, widths.action).green().bold().to_string(),
            "update" => fit_cell(action, widths.action).yellow().bold().to_string(),
            _ => fit_cell(action, widths.action).dark_grey().to_string(),
        };

        line(format!(
            "│{}│{}│{}│{}│{}│{}│{}│",
            marker,
            fit_cell(status.package, widths.package).white(),
            fit_cell(status.binary, widths.binary).white(),
            installed_cell,
            latest_cell,
            updatable_cell,
            action_cell,
        ))?;

        if let Some(error) = &status.error {
            line(format!("{} {}", "warn:".red().bold(), error.as_str().red()))?;
        }
    }
    line(format!("{}", bottom.dark_cyan()))?;

    line(String::new())?;
    let status_text = transient_status.unwrap_or(app.message.as_str());
    line(format!("{} {}", "status:".dark_grey(), status_text.white()))?;

    out.flush().context("failed to flush terminal output")?;
    Ok(())
}
