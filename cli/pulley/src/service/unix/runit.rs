use super::Backend;
use quench_cli::prelude::{Tone, print_status, require_binary};
use std::path::{Path, PathBuf};
use std::process::Command;

const SERVICE_NAME: &str = "pulley";
const SOURCE_DIR: &str = "/etc/sv/pulley";
const SV_HINT: &str = "pulley service management needs `sv` and `chpst`, which ship with runit";
// Checked in order; the first that exists is assumed to be the directory
// `runsvdir` is scanning. Covers Void (`/var/service`), classic runit /
// Devuan (`/etc/service`), and Artix (`/run/runit/service`).
const SCAN_DIRS: &[&str] = &["/var/service", "/etc/service", "/run/runit/service"];

pub struct Runit;

impl Backend for Runit {
    fn install(&self) -> Result<(), Box<dyn std::error::Error>> {
        require_binary("sv", SV_HINT)?;
        require_binary("chpst", SV_HINT)?;
        require_root("install")?;

        let scan_dir = scan_dir().ok_or(NO_SCAN_DIR)?;
        let link_path = scan_dir.join(SERVICE_NAME);
        let source_dir = PathBuf::from(SOURCE_DIR);
        let exe = std::env::current_exe()?;
        let user = target_user();

        std::fs::create_dir_all(&source_dir)?;
        let run_path = source_dir.join("run");
        std::fs::write(
            &run_path,
            format!(
                "#!/bin/sh\nexec chpst -u {user} {} daemon\n",
                exe.display()
            ),
        )?;
        set_executable(&run_path)?;
        print_status(
            Tone::Success,
            "service",
            &format!("wrote {}", run_path.display()),
        );

        if !link_path.exists() {
            std::os::unix::fs::symlink(&source_dir, &link_path)?;
            print_status(
                Tone::Success,
                "service",
                &format!("linked {} -> {}", link_path.display(), source_dir.display()),
            );
        }

        let started = Command::new("sv")
            .args(["up", &link_path.to_string_lossy()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if started {
            print_status(Tone::Success, "service", "started via `sv up`");
        } else {
            print_status(
                Tone::Warn,
                "service",
                "linked but not started yet; runsvdir polls for new services every few seconds \
                 — run `pulley service status` shortly, or `sv up pulley` manually",
            );
        }

        Ok(())
    }

    fn uninstall(&self) -> Result<(), Box<dyn std::error::Error>> {
        require_binary("sv", SV_HINT)?;
        require_root("uninstall")?;

        if let Some(scan_dir) = scan_dir() {
            let link_path = scan_dir.join(SERVICE_NAME);
            if link_path.exists() {
                let _ = Command::new("sv")
                    .args(["down", &link_path.to_string_lossy()])
                    .status();
                std::fs::remove_file(&link_path)?;
                print_status(
                    Tone::Success,
                    "service",
                    &format!("removed {}", link_path.display()),
                );
            } else {
                print_status(
                    Tone::Warn,
                    "service",
                    &format!("`{SERVICE_NAME}` was not linked into {}", scan_dir.display()),
                );
            }
        }

        let source_dir = PathBuf::from(SOURCE_DIR);
        if source_dir.exists() {
            std::fs::remove_dir_all(&source_dir)?;
            print_status(
                Tone::Success,
                "service",
                &format!("removed {}", source_dir.display()),
            );
        }

        Ok(())
    }

    fn status(&self) -> Result<(), Box<dyn std::error::Error>> {
        require_binary("sv", SV_HINT)?;
        let scan_dir = scan_dir().ok_or(NO_SCAN_DIR)?;
        let link_path = scan_dir.join(SERVICE_NAME);
        let _ = Command::new("sv")
            .args(["status", &link_path.to_string_lossy()])
            .status()?;
        Ok(())
    }
}

const NO_SCAN_DIR: &str =
    "no runit scan directory found (looked for /var/service, /etc/service, /run/runit/service)";

fn scan_dir() -> Option<PathBuf> {
    SCAN_DIRS.iter().map(PathBuf::from).find(|p| p.is_dir())
}

fn set_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)
}

/// The service file lives under `/etc`, and the scan-dir symlink activates a
/// process running as root unless dropped via `chpst -u`, so both need root.
fn require_root(action: &str) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("id").arg("-u").output()?;
    if String::from_utf8_lossy(&output.stdout).trim() != "0" {
        // `sudo` resets PATH by default, so a plain `sudo pulley` often
        // fails with "command not found" for a user-locally-installed
        // binary; hand back the resolved path so the retry just works.
        let exe = std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "pulley".to_string());
        return Err(format!(
            "pulley service management on runit writes system-wide service files under \
             /etc/sv and needs root; re-run as: sudo {exe} service {action}"
        )
        .into());
    }
    Ok(())
}

/// The user the daemon should actually run as (root only owns the service
/// definition; `chpst -u` drops to this user so it reads their own config).
fn target_user() -> String {
    std::env::var("SUDO_USER").unwrap_or_else(|_| std::env::var("USER").unwrap_or_else(|_| "root".to_string()))
}
