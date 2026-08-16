use super::Backend;
use quench_cli::prelude::{Tone, print_status, require_binary};
use std::path::PathBuf;
use std::process::Command;

const UNIT_NAME: &str = "pulley.service";
const SYSTEMCTL_HINT: &str = "pulley service management needs a systemd user session";

pub struct Systemd;

impl Backend for Systemd {
    fn install(&self) -> Result<(), Box<dyn std::error::Error>> {
        require_binary("systemctl", SYSTEMCTL_HINT)?;
        let exe = std::env::current_exe()?;
        let unit_dir = user_unit_dir()?;
        std::fs::create_dir_all(&unit_dir)?;
        let unit_path = unit_dir.join(UNIT_NAME);

        let unit = format!(
            "[Unit]\n\
             Description=Pulley continuous sync daemon\n\
             After=network-online.target\n\
             Wants=network-online.target\n\
             \n\
             [Service]\n\
             Type=simple\n\
             ExecStart={} daemon\n\
             Restart=on-failure\n\
             RestartSec=5\n\
             \n\
             [Install]\n\
             WantedBy=default.target\n",
            exe.display()
        );
        std::fs::write(&unit_path, unit)?;
        print_status(
            Tone::Success,
            "service",
            &format!("wrote {}", unit_path.display()),
        );

        run_systemctl(&["daemon-reload"])?;
        run_systemctl(&["enable", "--now", UNIT_NAME])?;
        print_status(
            Tone::Success,
            "service",
            "enabled and started pulley.service",
        );

        if !linger_enabled().unwrap_or(false) {
            print_status(
                Tone::Warn,
                "service",
                &format!(
                    "user services only start at login by default; to start pulley at boot run: loginctl enable-linger {}",
                    whoami()
                ),
            );
        }

        Ok(())
    }

    fn uninstall(&self) -> Result<(), Box<dyn std::error::Error>> {
        require_binary("systemctl", SYSTEMCTL_HINT)?;
        let _ = run_systemctl(&["disable", "--now", UNIT_NAME]);

        let unit_path = user_unit_dir()?.join(UNIT_NAME);
        if unit_path.exists() {
            std::fs::remove_file(&unit_path)?;
            print_status(
                Tone::Success,
                "service",
                &format!("removed {}", unit_path.display()),
            );
        }

        run_systemctl(&["daemon-reload"])?;
        Ok(())
    }

    fn status(&self) -> Result<(), Box<dyn std::error::Error>> {
        require_binary("systemctl", SYSTEMCTL_HINT)?;
        let _ = Command::new("systemctl")
            .args(["--user", "status", UNIT_NAME])
            .status()?;
        Ok(())
    }
}

fn run_systemctl(args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let mut full_args = vec!["--user"];
    full_args.extend_from_slice(args);
    let status = Command::new("systemctl").args(&full_args).status()?;
    if !status.success() {
        return Err(format!("systemctl {} failed", args.join(" ")).into());
    }
    Ok(())
}

fn user_unit_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home = std::env::var("HOME")?;
    Ok(PathBuf::from(home).join(".config/systemd/user"))
}

fn linger_enabled() -> Result<bool, Box<dyn std::error::Error>> {
    let user = whoami();
    let output = Command::new("loginctl")
        .args(["show-user", &user, "--property=Linger"])
        .output()?;
    Ok(String::from_utf8_lossy(&output.stdout).trim() == "Linger=yes")
}

fn whoami() -> String {
    std::env::var("USER").unwrap_or_else(|_| "$USER".to_string())
}
