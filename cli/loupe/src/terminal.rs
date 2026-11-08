//! The pager on the real terminal.
//!
//! The alternate screen and raw mode go through quench-cli, keys and graphics detection through `/dev/tty` (so
//! `loupe -` can page what came on stdin), and signals are delivered like keys.
//!
//! Keys and signals arrive on one channel: a thread reads keys, and a small current-thread tokio runtime on
//! another thread turns SIGINT, SIGTERM, SIGTSTP, SIGCONT and SIGWINCH into events. The terminal is restored on
//! every way out: `q`, Ctrl-C (a key in raw mode), SIGINT (exit 130) and SIGTERM (exit 143) from outside, the
//! end of input, an error, and a panic (a hook restores the screen first, so the message is readable). Ctrl-Z
//! and SIGTSTP restore the terminal and stop the process; SIGCONT sets the terminal up again and redraws.

use crate::graphics::{Graphics, Painter, TexTypesetter, detect_with};
use crate::pager::{Effect, Key, Mode, Pager, Signal};
use crate::view::frame;
use quench_cli::graphics::detect::{Capabilities, Environment};
use quench_cli::keys::{KeyCode, KeyEvent, Keys};
use quench_cli::term::{
    self, ENTER_ALTERNATE_SCREEN, HIDE_CURSOR, LEAVE_ALTERNATE_SCREEN, SHOW_CURSOR,
};
use quench_cli::tty::RawMode;
use quench_tex::typeset::Fonts;
use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

/// How often the size is asked for when signals cannot be received (each ask runs `stty size`).
const SIZE_EVERY: Duration = Duration::from_millis(300);

/// SIGTSTP and SIGCONT: tokio names no constructor for them.
#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd"
))]
const SIGTSTP: i32 = 18;
#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd"
))]
const SIGCONT: i32 = 19;
#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd"
)))]
const SIGTSTP: i32 = 20;
#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd"
)))]
const SIGCONT: i32 = 18;

/// Turns a key event of quench-cli into the pager's.
#[must_use]
pub const fn key_of(event: KeyEvent) -> Option<Key> {
    Some(match event.code {
        KeyCode::Char(c) if event.modifiers.ctrl => Key::Ctrl(c),
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Esc,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => Key::BackTab,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        _ => return None,
    })
}

/// The OSC 52 sequence that puts `text` on the clipboard.
#[must_use]
pub fn osc52(text: &str) -> String {
    format!(
        "\x1b]52;c;{}\x07",
        quench_core::base64::STANDARD.encode(text)
    )
}

/// A signal received from outside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Raised {
    Interrupt,
    Terminate,
    Stop,
    Continue,
    WindowChange,
}

/// What the pager loop waits for.
#[derive(Debug)]
pub enum Input {
    Key(KeyEvent),
    Signal(Raised),
    /// The terminal's input ended or failed.
    Ended(io::Error),
}

/// The pager's signal for a raised one; the window change takes the terminal's size, asked by `size`.
pub fn signal_of(raised: Raised, size: impl FnOnce() -> Option<(usize, usize)>) -> Option<Signal> {
    Some(match raised {
        Raised::Interrupt => Signal::Interrupt,
        Raised::Terminate => Signal::Terminate,
        Raised::Stop => Signal::Stop,
        Raised::Continue => Signal::Continue,
        Raised::WindowChange => {
            let (columns, rows) = size()?;
            Signal::Resize(columns, rows)
        }
    })
}

/// Starts the thread that turns signals into [`Input::Signal`]s. Registering replaces the default actions of
/// SIGINT, SIGTERM and SIGTSTP (so the terminal can be restored first).
///
/// # Errors
/// The runtime cannot be built or a signal cannot be registered.
pub fn spawn_signals(events: Sender<Input>) -> io::Result<()> {
    use tokio::signal::unix::{SignalKind, signal};
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let mut streams = {
        let _context = runtime.enter();
        [
            (signal(SignalKind::interrupt())?, Raised::Interrupt),
            (signal(SignalKind::terminate())?, Raised::Terminate),
            (signal(SignalKind::from_raw(SIGTSTP))?, Raised::Stop),
            (signal(SignalKind::from_raw(SIGCONT))?, Raised::Continue),
            (signal(SignalKind::window_change())?, Raised::WindowChange),
        ]
    };
    std::thread::Builder::new()
        .name("loupe-signals".to_string())
        .spawn(move || {
            runtime.block_on(async move {
                loop {
                    let [
                        (int, on_int),
                        (term, on_term),
                        (tstp, on_tstp),
                        (cont, on_cont),
                        (winch, on_winch),
                    ] = &mut streams;
                    let raised = tokio::select! {
                        Some(()) = int.recv() => *on_int,
                        Some(()) = term.recv() => *on_term,
                        Some(()) = tstp.recv() => *on_tstp,
                        Some(()) = cont.recv() => *on_cont,
                        Some(()) = winch.recv() => *on_winch,
                        else => break,
                    };
                    if events.send(Input::Signal(raised)).is_err() {
                        break;
                    }
                }
            });
        })?;
    Ok(())
}

/// Starts the thread that reads keys from the terminal at `fd` into [`Input::Key`]s.
fn spawn_keys(fd: i32, events: Sender<Input>) -> io::Result<()> {
    let mut keys = Keys::on_fd(fd);
    std::thread::Builder::new()
        .name("loupe-keys".to_string())
        .spawn(move || {
            loop {
                match keys.read() {
                    Ok(key) => {
                        if events.send(Input::Key(key)).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = events.send(Input::Ended(e));
                        break;
                    }
                }
            }
        })?;
    Ok(())
}

/// The controlling terminal, `/dev/tty`: keys, detection and raw mode all go through it, whatever stdin is.
///
/// # Errors
/// There is no controlling terminal.
pub fn open_tty() -> io::Result<File> {
    OpenOptions::new().read(true).write(true).open("/dev/tty")
}

/// What the terminal at `tty` shows.
///
/// The environment's plan, then the queries written to the terminal and the answers read back from it in raw
/// mode with a read timeout. Standard output must be a terminal (the one the page is drawn on); standard input
/// may be a pipe.
#[must_use]
pub fn detect(tty: Option<&File>) -> Capabilities {
    let mut environment = Environment::from_process();
    environment.terminal = tty.is_some() && term::stdout_is_tty();
    let Some(tty) = tty else {
        return detect_with(
            &environment,
            &mut io::sink(),
            &mut io::empty(),
            None,
            &mut || Duration::MAX,
        );
    };
    let fd = tty.as_raw_fd();
    let size = term::size_of(fd).ok();
    let (Ok(mut output), Ok(mut input)) = (tty.try_clone(), tty.try_clone()) else {
        environment.terminal = false;
        return detect_with(
            &environment,
            &mut io::sink(),
            &mut io::empty(),
            None,
            &mut || Duration::MAX,
        );
    };
    // Text printed before reaches the terminal first.
    let _ = io::stdout().flush();
    let _ = io::stderr().flush();
    let Ok(Some(raw)) = RawMode::enable_with_read_timeout(fd, 1) else {
        environment.terminal = false;
        return detect_with(
            &environment,
            &mut io::sink(),
            &mut io::empty(),
            None,
            &mut || Duration::MAX,
        );
    };
    let start = Instant::now();
    let capabilities = detect_with(&environment, &mut output, &mut input, size, &mut || {
        start.elapsed()
    });
    drop(raw);
    capabilities
}

/// The terminal while the pager owns it.
struct Session {
    fd: i32,
    raw: Option<RawMode>,
}

impl Session {
    fn open(fd: i32) -> io::Result<Self> {
        let mut session = Self { fd, raw: None };
        session.enter()?;
        Ok(session)
    }

    /// Raw mode and the alternate screen (again: after SIGCONT the shell may have reset the terminal).
    fn enter(&mut self) -> io::Result<()> {
        self.raw = None;
        self.raw = RawMode::enable(self.fd)?;
        if self.raw.is_none() {
            return Err(io::Error::other("/dev/tty is not a terminal"));
        }
        let mut out = io::stdout().lock();
        write!(out, "{ENTER_ALTERNATE_SCREEN}{HIDE_CURSOR}")?;
        out.flush()
    }

    fn leave(&mut self) {
        let mut out = io::stdout().lock();
        let _ = write!(out, "{SHOW_CURSOR}{LEAVE_ALTERNATE_SCREEN}");
        let _ = out.flush();
        self.raw = None;
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.leave();
    }
}

/// Restores the screen before a panic's message is printed (and the terminal's settings, in case the panic
/// aborts instead of unwinding past the `RawMode`).
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let mut out = io::stdout();
        let _ = write!(out, "{SHOW_CURSOR}{LEAVE_ALTERNATE_SCREEN}");
        let _ = out.flush();
        if let Ok(tty) = File::open("/dev/tty") {
            let _ = Command::new("stty").arg("sane").stdin(tty).status();
        }
        previous(info);
    }));
}

/// Stops the process with the terminal restored; returns once it is continued, the terminal set up again.
fn suspend(session: &mut Session) -> io::Result<()> {
    session.leave();
    let _ = Command::new("kill")
        .args(["-STOP", &std::process::id().to_string()])
        .status();
    session.enter()
}

fn open_outside(target: &str) -> String {
    match Command::new("xdg-open")
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(_) => format!("opened {target}"),
        Err(e) => format!("xdg-open failed: {e}"),
    }
}

fn size_of(fd: i32) -> Option<(usize, usize)> {
    term::size_of(fd)
        .ok()
        .map(|(c, r)| (usize::from(c), usize::from(r)))
}

/// The next inputs: waits for one (with signals, as long as it takes; without, at most [`SIZE_EVERY`]), then
/// takes every one already waiting, so a burst of keys is drawn once.
fn next_inputs(events: &Receiver<Input>, polling: bool) -> Result<Vec<Input>, ()> {
    let first = if polling {
        match events.recv_timeout(SIZE_EVERY) {
            Ok(input) => Some(input),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => return Err(()),
        }
    } else {
        Some(events.recv().map_err(|_| ())?)
    };
    let mut inputs: Vec<Input> = first.into_iter().collect();
    inputs.extend(events.try_iter());
    Ok(inputs)
}

/// Pages on the terminal `tty` until the reader quits; returns the exit status.
///
/// `graphics` is what formulas and images are drawn with, when the terminal shows images; formulas are typeset
/// (with `fonts`) only when `typeset` is true, and stay text otherwise.
pub fn run(
    mut pager: Pager,
    graphics: Option<Graphics>,
    fonts: Fonts,
    typeset: bool,
    tty: &File,
) -> io::Result<i32> {
    install_panic_hook();
    let fd = tty.as_raw_fd();
    let mut session = Session::open(fd)?;
    let (sender, events) = mpsc::channel();
    spawn_keys(fd, sender.clone())?;
    let polling = spawn_signals(sender).is_err();
    let mut painter =
        graphics.map(|g| Painter::new(g, TexTypesetter::new(fonts)).typesetting(typeset));
    pager.set_graphics(painter.as_ref().map(|p| p.graphics().cell));
    let mut columns = size_of(fd).map_or(80, |s| s.0);
    if let Some((c, r)) = size_of(fd) {
        pager.resize(c, r);
    }
    let mut dirty = true;
    let mut images_shown = false;
    let result = 'pager: loop {
        if dirty {
            dirty = false;
            draw(&pager, painter.as_mut(), &mut images_shown, columns)?;
        }
        let Ok(inputs) = next_inputs(&events, polling) else {
            break Ok(0);
        };
        if polling && inputs.is_empty() {
            if let Some((c, r)) = size_of(fd) {
                let generation = pager.generation();
                dirty |= c != columns;
                columns = c;
                pager.resize(c, r);
                dirty |= generation != pager.generation();
            }
            continue;
        }
        for input in inputs {
            dirty = true;
            let effect = match input {
                Input::Ended(e) => break 'pager Err(e),
                Input::Key(event) => match key_of(event) {
                    Some(Key::Ctrl('l')) => {
                        if let Some(painter) = &mut painter {
                            io::stdout().lock().write_all(painter.hide().as_bytes())?;
                        }
                        None
                    }
                    Some(key) => pager.key(key),
                    None => None,
                },
                Input::Signal(raised) => {
                    let Some(signal) = signal_of(raised, || size_of(fd)) else {
                        continue;
                    };
                    if let Signal::Resize(c, _) = signal {
                        columns = c;
                    }
                    pager.signal(signal)
                }
            };
            match effect {
                None => {}
                Some(Effect::Quit) => break 'pager Ok(0),
                Some(Effect::Exit(status)) => break 'pager Ok(status),
                Some(Effect::Copy(text)) => {
                    io::stdout().lock().write_all(osc52(&text).as_bytes())?;
                }
                Some(Effect::Open(target)) => pager.set_message(Some(open_outside(&target))),
                Some(Effect::Suspend) => {
                    if let Some(painter) = &mut painter {
                        io::stdout().lock().write_all(painter.hide().as_bytes())?;
                    }
                    images_shown = false;
                    suspend(&mut session)?;
                }
                Some(Effect::Redraw) => {
                    session.enter()?;
                    if let Some(painter) = &mut painter {
                        io::stdout().lock().write_all(painter.hide().as_bytes())?;
                    }
                    images_shown = false;
                }
            }
        }
    };
    if let Some(painter) = &mut painter {
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(painter.forget().as_bytes());
        let _ = stdout.flush();
    }
    drop(session);
    result
}

/// Draws the screen, and the pictures over it.
fn draw(
    pager: &Pager,
    painter: Option<&mut Painter<TexTypesetter>>,
    images_shown: &mut bool,
    columns: usize,
) -> io::Result<()> {
    let mut out = String::new();
    let mut drawn = HashSet::new();
    let mut pictures = String::new();
    if let Some(painter) = painter {
        let formulas: Vec<String> = pager
            .rendered()
            .formulas
            .iter()
            .map(|f| f.tex.clone())
            .collect();
        out.push_str(&painter.sync(pager.generation(), &formulas));
        if matches!(
            pager.mode(),
            Mode::Normal | Mode::Search(_) | Mode::Confirm(_)
        ) {
            let candidates = pager.candidates();
            drawn = painter.drawable(&candidates);
            pictures = painter.paint(&candidates);
            *images_shown = true;
        } else if *images_shown {
            pictures = painter.hide();
            *images_shown = false;
        }
    }
    let screen = pager.screen_with(&drawn);
    out.push_str(&frame(&screen, &pager.rendered().links, columns));
    out.push_str(&pictures);
    let mut stdout = io::stdout().lock();
    stdout.write_all(out.as_bytes())?;
    stdout.flush()
}
