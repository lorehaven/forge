mod config;
mod repl;
mod rsync;

use config::Config;
use repl::Repl;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = match Config::load_merged() {
        Ok(config) => {
            println!(
                "Configuration loaded successfully. {} job(s) found.\n",
                config.jobs.len()
            );
            config
        }
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
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
