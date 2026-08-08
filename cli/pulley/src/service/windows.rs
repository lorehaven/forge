use quench_cli::prelude::{Tone, print_status, require_binary};
use std::process::Command;

const TASK_NAME: &str = "Pulley";
const SCHTASKS_HINT: &str =
    "pulley service management needs schtasks.exe, which ships with Windows";

pub fn install() -> Result<(), Box<dyn std::error::Error>> {
    require_binary("schtasks", SCHTASKS_HINT)?;
    let exe = std::env::current_exe()?;
    let task_run = format!("\"{}\" daemon", exe.display());

    run_schtasks(&[
        "/Create", "/TN", TASK_NAME, "/TR", &task_run, "/SC", "ONLOGON", "/RL", "LIMITED", "/F",
    ])?;
    print_status(
        Tone::Success,
        "service",
        &format!("registered scheduled task `{TASK_NAME}` to run `pulley daemon` at logon"),
    );

    let started = Command::new("schtasks")
        .args(["/Run", "/TN", TASK_NAME])
        .status()?
        .success();
    if started {
        print_status(Tone::Success, "service", "started now via `schtasks /Run`");
    } else {
        print_status(
            Tone::Warn,
            "service",
            "task registered but could not be started immediately; it will start at next logon",
        );
    }

    Ok(())
}

pub fn uninstall() -> Result<(), Box<dyn std::error::Error>> {
    require_binary("schtasks", SCHTASKS_HINT)?;
    let _ = Command::new("schtasks")
        .args(["/End", "/TN", TASK_NAME])
        .status();

    let removed = Command::new("schtasks")
        .args(["/Delete", "/TN", TASK_NAME, "/F"])
        .status()?
        .success();
    if removed {
        print_status(
            Tone::Success,
            "service",
            &format!("removed scheduled task `{TASK_NAME}`"),
        );
    } else {
        print_status(
            Tone::Warn,
            "service",
            &format!("`{TASK_NAME}` was not registered"),
        );
    }

    Ok(())
}

pub fn status() -> Result<(), Box<dyn std::error::Error>> {
    require_binary("schtasks", SCHTASKS_HINT)?;
    let _ = Command::new("schtasks")
        .args(["/Query", "/TN", TASK_NAME, "/V", "/FO", "LIST"])
        .status()?;
    Ok(())
}

fn run_schtasks(args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new("schtasks").args(args).status()?;
    if !status.success() {
        return Err(format!("schtasks {} failed", args.join(" ")).into());
    }
    Ok(())
}
