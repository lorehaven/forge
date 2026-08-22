use clap::Parser;
use pulley::cli::{Cli, Command, ServiceAction};
use pulley::config::Config;
use pulley::repl::Repl;
use pulley::{daemon, service};
use quench_cli::prelude::{Tone, print_status, require_binary};

#[cfg(unix)]
const RSYNC_HINT: &str = "pulley shells out to it for every sync job; install it (e.g. `apt install rsync` / `pacman -S rsync`)";
#[cfg(windows)]
const RSYNC_HINT: &str = "pulley shells out to it for every sync job; install it via WSL, MSYS2 (`pacman -S rsync`), or cwRsync, and make sure it's on PATH";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Service { action }) => {
            return match action {
                ServiceAction::Install => service::install(),
                ServiceAction::Uninstall => service::uninstall(),
                ServiceAction::Status => service::status(),
            };
        }
        Some(Command::Daemon) => {
            require_binary("rsync", RSYNC_HINT)?;
            let config = load_config_or_exit();
            service::hide_console_window();
            return daemon::run(&config);
        }
        None => {}
    }

    require_binary("rsync", RSYNC_HINT)?;
    let config = load_config_or_exit();
    let mut repl = Repl::new(config);
    repl.run()?;

    Ok(())
}

fn load_config_or_exit() -> Config {
    match Config::load_merged() {
        Ok(config) => {
            print_status(
                Tone::Success,
                "config",
                &format!("loaded successfully. {} job(s) found.", config.jobs.len()),
            );
            println!();
            config
        }
        Err(e) => {
            print_status(Tone::Error, "config", &format!("failed to load: {e}"));
            eprintln!("\nPulley loads configuration from:");
            if let Some(global_dir) = Config::global_config_dir() {
                eprintln!("  Global: {}/*.toml", global_dir.display());
            }
            eprintln!("  Local:  *.pulley.toml (in current directory)");
            eprintln!("\nMultiple config files are supported.");
            eprintln!("Local jobs override global jobs with matching IDs.");
            eprintln!("\nExample configuration (personal.pulley.toml):");
            eprintln!("[[jobs]]");
            eprintln!("id = \"job1\"");
            eprintln!("desc = \"Backup documents\"");
            eprintln!("src = \"/path/to/source\"");
            eprintln!("dest = \"/path/to/destination\"");
            eprintln!("delete = true");
            eprintln!("skip = [\"temp\", \"logs\"]");
            eprintln!("no-confirm = false");
            std::process::exit(1);
        }
    }
}
