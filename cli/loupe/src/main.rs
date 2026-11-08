use loupe::app::{Env, Outcome, PagerOptions, run, subcommand_hint};
use loupe::cli::Cli;
use loupe::doctor::{Probe, report};
use loupe::fonts::MathFont;
use loupe::graphics::check_typeset;
use loupe::pager::Pager;
use loupe::released::{Credentials, Http};
use quench_args::Parser;
use quench_cli::graphics::detect::{Capabilities, Environment};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if let Some(hint) = std::env::current_dir()
        .ok()
        .and_then(|cwd| subcommand_hint(&args, &cwd))
    {
        eprintln!("{hint}");
    }
    let cli = Cli::parse();
    let env = environment();
    let net = match Http::new(credentials(&env)) {
        Ok(net) => net,
        Err(e) => {
            eprintln!("loupe: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr();
    let outcome = run(
        &cli,
        &env,
        &net,
        &mut io::stdin().lock(),
        &mut stdout,
        &mut stderr,
    );
    drop(stdout);
    match outcome {
        Outcome::Exit(status) => exit(status),
        Outcome::Doctor(font) => doctor(&font),
        Outcome::Pager(pager, options) => page(*pager, options),
        Outcome::Login(args, url) => login(&args, url, &env),
    }
}

/// loupe's config directory: where the config file is, and `loupe login` keeps its token.
fn config_dir(env: &Env) -> Option<PathBuf> {
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    loupe::config::path(var("XDG_CONFIG_HOME").as_deref(), env.home.as_deref())
        .and_then(|file| file.parent().map(Path::to_path_buf))
}

fn credentials(env: &Env) -> Credentials {
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
    Credentials {
        cwd: env.cwd.clone(),
        config_dir: config_dir(env),
        token: var("LOUPE_WAREHOUSE_TOKEN"),
        anvil_app: var("ANVIL_APP_TOKEN").zip(var("ANVIL_GATEHOUSE_URL")),
    }
}

fn login(args: &loupe::cli::LoginArgs, url: Option<String>, env: &Env) -> ExitCode {
    let Some(dir) = config_dir(env) else {
        eprintln!("loupe: no config directory to keep the token in: set XDG_CONFIG_HOME or HOME");
        return ExitCode::FAILURE;
    };
    let mut ask = |label: &str| -> Result<String, String> {
        print!("{label}");
        io::stdout().flush().map_err(|e| e.to_string())?;
        let mut line = String::new();
        io::stdin()
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        Ok(line.trim().to_string())
    };
    match loupe::login::login(args, url, &dir, &mut ask) {
        Ok(path) => {
            println!("Stored an application token in {}", path.display());
            println!("Revoke it from Gatehouse if the machine is lost.");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("loupe: {e}");
            ExitCode::FAILURE
        }
    }
}

fn exit(status: i32) -> ExitCode {
    ExitCode::from(u8::try_from(status).unwrap_or(1))
}

fn environment() -> Env {
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    let no_color = var("NO_COLOR").is_some();
    let home = var("HOME");
    let config = loupe::config::path(var("XDG_CONFIG_HOME").as_deref(), home.as_deref())
        .and_then(|path| std::fs::read_to_string(&path).ok().map(|text| (path, text)));
    #[cfg(unix)]
    let (stdout_tty, size) = {
        use quench_cli::term;
        let stdout_tty = term::stdout_is_tty();
        let size = stdout_tty
            .then(|| term::size().ok())
            .flatten()
            .map(|(c, r)| (usize::from(c), usize::from(r)));
        (stdout_tty, size)
    };
    #[cfg(not(unix))]
    let (stdout_tty, size) = (false, None);
    let cache = loupe::released::cache_root(var("XDG_CACHE_HOME").as_deref(), home.as_deref());
    Env {
        no_color,
        stdout_tty,
        size,
        config,
        home,
        cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        cache,
        warehouse_url: var("LOUPE_WAREHOUSE_URL"),
        storage: var("LOUPE_STORAGE"),
        gatehouse_url: var("LOUPE_GATEHOUSE_URL"),
    }
}

/// What the terminal shows, asked through `/dev/tty`.
fn capabilities() -> Capabilities {
    #[cfg(unix)]
    {
        let tty = loupe::terminal::open_tty().ok();
        loupe::terminal::detect(tty.as_ref())
    }
    #[cfg(not(unix))]
    {
        use quench_cli::graphics::detect;
        detect::resolve(detect::plan(&Environment::from_process()), None, None)
    }
}

fn doctor(font: &MathFont) -> ExitCode {
    let mut environment = Environment::from_process();
    // Detection goes through /dev/tty, so stdin may be a pipe.
    #[cfg(unix)]
    {
        environment.terminal = quench_cli::term::stdout_is_tty() && Path::new("/dev/tty").exists();
    }
    let probe = Probe {
        capabilities: capabilities(),
        environment,
        font: font.source.describe(),
        typeset: font
            .fonts
            .as_ref()
            .map_err(String::clone)
            .and_then(check_typeset),
    };
    match report(&probe, &mut io::stdout().lock()) {
        Ok(status) => exit(status),
        Err(e) => {
            let _ = writeln!(io::stderr(), "loupe: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(unix)]
fn page(mut pager: Pager, options: PagerOptions) -> ExitCode {
    use loupe::graphics::Graphics;
    let tty = match loupe::terminal::open_tty() {
        Ok(tty) => tty,
        Err(e) => {
            eprintln!("loupe: no terminal to page on (/dev/tty: {e}); try --plain");
            return ExitCode::FAILURE;
        }
    };
    let PagerOptions {
        images,
        mut typeset,
        font,
    } = options;
    // The chosen font failed: its warning is on the status line already, and formulas stay text. Images of
    // the page are still drawn.
    typeset &= font.fonts.is_ok();
    let fonts = font
        .fonts
        .unwrap_or_else(|_| quench_tex::typeset::Fonts::bundled());
    let graphics = if images {
        Graphics::from_capabilities(&loupe::terminal::detect(Some(&tty)))
    } else {
        None
    };
    if graphics.is_some()
        && typeset
        && let Err(e) = check_typeset(&fonts)
    {
        pager.set_warning(Some(format!(
            "formulas are shown as text: the math font does not typeset ({e})"
        )));
        typeset = false;
    }
    match loupe::terminal::run(pager, graphics, fonts, typeset, &tty) {
        Ok(status) => exit(status),
        Err(e) => {
            eprintln!("loupe: the terminal failed: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(unix))]
fn page(pager: Pager, _options: PagerOptions) -> ExitCode {
    let mut out = io::stdout().lock();
    let _ = out.write_all(quench_markdown_term::to_plain(pager.rendered()).as_bytes());
    ExitCode::SUCCESS
}
