//! The program a runner Job runs (alongside `riveter` and `kubectl` in the runner image). Reads its plan
//! from `GANTRY_PLAN`, executes it and prints the outcome as the last line of its output.
//!
//! `GANTRY_VALUES_DIR`, `GANTRY_PACKAGES_DIR` and `GANTRY_NAMESPACES` say where things are; `GANTRY_DRY_RUN`
//! prints every command that would change the cluster instead of running it.

use gantry_service::domain::commands::Context;
use gantry_service::domain::runner::{Outcome, ProcessExec, run};
use gantry_service::domain::steps::Plan;
use std::path::PathBuf;

fn main() {
    let Ok(raw) = std::env::var("GANTRY_PLAN") else {
        eprintln!("GANTRY_PLAN is not set");
        std::process::exit(2);
    };
    let plan: Plan = match serde_json::from_str(&raw) {
        Ok(plan) => plan,
        Err(error) => {
            println!(
                "{}",
                Outcome::Failed(format!("the plan is not valid: {error}")).marker()
            );
            std::process::exit(2);
        }
    };

    let ctx = Context {
        values_dir: PathBuf::from(envmnt::get_or("GANTRY_VALUES_DIR", "/values")),
        packages_dir: PathBuf::from(envmnt::get_or("GANTRY_PACKAGES_DIR", "/work/packages")),
        allowed_namespaces: envmnt::get_or("GANTRY_NAMESPACES", "")
            .split(',')
            .map(str::trim)
            .filter(|namespace| !namespace.is_empty())
            .map(str::to_string)
            .collect(),
        dry_run: envmnt::is_or("GANTRY_DRY_RUN", false),
        source_dir: Some(envmnt::get_or("GANTRY_SOURCE_PACKAGES_DIR", ""))
            .filter(|dir| !dir.is_empty())
            .map(PathBuf::from),
    };
    let _ = std::fs::create_dir_all(&ctx.packages_dir);

    let outcome = run(&plan, &ctx, &mut ProcessExec, &mut std::io::stdout());
    std::process::exit(i32::from(outcome != Outcome::Succeeded));
}
