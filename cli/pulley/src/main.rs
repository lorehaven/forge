mod config;
mod repl;
mod rsync;

use config::Config;
use quench_cli::terminal::{Tone, print_status};
use repl::Repl;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if handle_version_flag() {
        return Ok(());
    }

    let config = match Config::load_merged() {
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
    };

    let mut repl = Repl::new(config);
    repl.run()?;

    Ok(())
}

fn handle_version_flag() -> bool {
    let mut args = std::env::args().skip(1);
    if let Some(arg) = args.next()
        && (arg == "--version" || arg == "-V")
    {
        println!("pulley {}", env!("CARGO_PKG_VERSION"));
        return true;
    }
    false
}
