//! What each verb does.

use anyhow::{Context, Result, bail};
use quench_cli::prelude::{DIM, RESET};

use crate::config::Role;
use crate::docker;
use crate::estate::Estate;
use crate::services;
use crate::tasks;
use crate::ui;

pub const STARTER_CONFIG: &str = include_str!("../templates/foreman.toml");

pub fn start(estate: &Estate, names: &[String]) -> Result<()> {
    let selection = estate.resolve_selection(names)?;
    if selection.is_empty() {
        bail!(
            "no services are configured in {}",
            estate.config_path.display()
        );
    }

    // Only worth saying when it is not the whole estate; the dependencies that
    // joined on the way in are the interesting part.
    if !estate.is_whole_estate(&selection) {
        ui::info("selection", selection.join(", "));
    }

    estate.ensure_dirs()?;

    for container in &estate.config.containers {
        docker::start(container, &estate.scope())?;
    }

    tasks::run_role(estate, Role::Migrate, &selection)?;

    for name in &selection {
        services::ensure_cert(estate, name)?;
    }

    // The config file's order is the dependency order, and the selection keeps
    // it, so a service starts after whatever it checks at startup.
    let mut failed = Vec::new();
    for name in &selection {
        if !services::start(estate, name)? {
            failed.push(name.clone());
        }
    }

    summary(estate, &selection)?;

    if !failed.is_empty() {
        bail!("{} did not start", failed.join(", "));
    }
    Ok(())
}

pub fn summary(estate: &Estate, selection: &[String]) -> Result<()> {
    ui::blank();

    for note in &estate.config.notes {
        if let Some(required) = &note.when_selected
            && !selection.iter().any(|name| name == required)
        {
            continue;
        }
        ui::say(
            note.tone.into(),
            &note.label,
            estate.scope().expand(&note.message)?,
        );
    }

    ui::info("logs", estate.log_dir().display().to_string());
    ui::blank();
    for name in selection {
        let service = estate.resolve(name)?;
        ui::entry(name, &service.url());
    }
    ui::blank();
    ui::info("stop", "foreman stop");
    Ok(())
}

/// `all` reaches past the services to the containers under them; anything else
/// is a list of services, and no argument is every service.
pub fn stop(estate: &Estate, names: &[String]) -> Result<()> {
    let everything = names.iter().any(|name| name == "all");

    let selection = if everything {
        estate.service_names()
    } else {
        estate.resolve_names(names)?
    };

    services::stop(estate, &selection)?;
    services::report_strays(estate)?;

    if everything {
        for container in &estate.config.containers {
            docker::stop(container);
        }
    }

    Ok(())
}

pub fn status(estate: &Estate) -> Result<()> {
    for container in &estate.config.containers {
        docker::status(container);
    }

    for name in estate.service_names() {
        match services::pid(estate, &name) {
            Some(pid) => {
                let service = estate.resolve(&name)?;
                ui::ok(&name, format!("{} (pid {pid})", service.url()));
            }
            None => ui::warn(&name, "not running"),
        }
    }

    Ok(())
}

pub fn logs(estate: &Estate, name: &str) -> Result<()> {
    estate.service(name)?;
    crate::process::follow(&estate.log_file(name))
}

/// The database and its schemas, without the services on top - for working on
/// migrations, or on something that only needs a database to talk to.
pub fn db(estate: &Estate) -> Result<()> {
    estate.ensure_dirs()?;

    for name in tasks::containers_for_role(estate, Role::Migrate) {
        let container = estate
            .config
            .container(&name)
            .with_context(|| format!("container '{name}'"))?;
        docker::start(container, &estate.scope())?;
    }

    // No selection: `db` is the whole catalog, whatever is running.
    tasks::run_role(estate, Role::Migrate, &[])
}

pub fn reset(estate: &Estate) -> Result<()> {
    if estate.config.tasks_with_role(Role::Reset).is_empty() {
        bail!(
            "no task with `role = \"reset\"` in {}",
            estate.config_path.display()
        );
    }
    estate.ensure_dirs()?;
    tasks::run_role(estate, Role::Reset, &[])
}

/// The suite usually starts its own copies of the services on the same ports as
/// the development estate, so the two cannot be up at once. Rather than fail
/// with a confusing bind error, take the estate down first and say so.
pub fn test(estate: &Estate, args: &[String]) -> Result<i32> {
    let Some(test) = &estate.config.test else {
        bail!("no [test] section in {}", estate.config_path.display());
    };

    if test.stop_services {
        let running = services::running_services(estate);
        if !running.is_empty() {
            ui::warn(
                "test",
                format!(
                    "the estate is up ({}); stopping it - the suite needs the same ports",
                    running.join(", ")
                ),
            );
            services::stop(estate, &running)?;
            services::report_strays(estate)?;
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    }

    // A bare service name is the common case; anything starting with `-` is
    // passed straight through to the suite.
    let mut suite_args: Vec<String> = Vec::new();
    let mut rest = args;
    if let Some(first) = args.first()
        && !first.starts_with('-')
        && !test.service_arg.is_empty()
    {
        estate.service(first)?;
        let scope = estate.scope().with("service", first).with("name", first);
        suite_args.extend(scope.expand_all(&test.service_arg)?);
        rest = &args[1..];
    }
    suite_args.extend(rest.iter().cloned());

    let command = estate.scope().expand_all(&test.command)?;
    let Some((program, fixed)) = command.split_first() else {
        bail!("[test].command is empty");
    };

    let mut full: Vec<String> = fixed.to_vec();
    full.extend(suite_args.clone());

    if suite_args.is_empty() {
        ui::info("test", "running every suite");
    } else {
        ui::info("test", format!("{program} {}", full.join(" ")));
    }

    let passed = crate::process::run(program, &full, &estate.root, &[])?;
    if passed {
        ui::ok("test", "suites passed");
    } else {
        ui::error("test", "suites failed");
    }

    if let Some(note) = &test.note {
        ui::info("test", estate.scope().expand(note)?);
    }

    Ok(if passed { 0 } else { 1 })
}

pub fn run_task(estate: &Estate, task: &str, names: &[String]) -> Result<()> {
    estate.ensure_dirs()?;
    let selection = if names.is_empty() {
        Vec::new()
    } else {
        estate.resolve_selection(names)?
    };
    tasks::run_named(estate, task, &selection)
}

pub fn list(estate: &Estate) -> Result<()> {
    ui::info("config", estate.config_path.display().to_string());
    ui::blank();

    if !estate.config.containers.is_empty() {
        println!("{DIM}containers{RESET}");
        for container in &estate.config.containers {
            ui::entry(&container.name, &container.image);
        }
        ui::blank();
    }

    println!("{DIM}services{RESET}");
    for name in estate.service_names() {
        let service = estate.resolve(&name)?;
        let needs = &estate.service(&name)?.needs;
        let suffix = if needs.is_empty() {
            String::new()
        } else {
            format!("  {DIM}needs {}{RESET}", needs.join(", "))
        };
        ui::entry(&name, &format!("{}{suffix}", service.url()));
    }

    if !estate.config.tasks.is_empty() {
        ui::blank();
        println!("{DIM}tasks{RESET}");
        for (name, task) in &estate.config.tasks {
            let role = match task.role {
                Role::Migrate => "runs before a start, and on `foreman db`",
                Role::Reset => "runs on `foreman reset`",
                Role::Manual => "runs on `foreman run`",
            };
            ui::entry(name, &format!("{DIM}{role}{RESET}"));
        }
    }

    Ok(())
}

/// What one service would actually be started with. The shared environment,
/// the service's own additions and the conditional blocks all end up in one
/// list here, which is the only place you can see the result of that merge
/// before it becomes a running process.
pub fn env(estate: &Estate, name: &str) -> Result<()> {
    let service = estate.resolve(name)?;

    ui::info("binary", service.binary.display().to_string());
    ui::info("workdir", service.workdir.display().to_string());
    ui::info("build", service.build.join(" "));
    ui::info("health", service.health_url());
    for note in &service.notes {
        ui::info("applied", note);
    }
    ui::blank();

    for name in &service.unset {
        println!("  {DIM}unset {name}{RESET}");
    }
    for (key, value) in &service.env {
        println!("  {key}={value}");
    }

    Ok(())
}

pub fn init(force: bool) -> Result<()> {
    let path = std::env::current_dir()?.join("foreman.toml");
    if path.exists() && !force {
        bail!("{} already exists (--force overwrites it)", path.display());
    }

    std::fs::write(&path, STARTER_CONFIG).with_context(|| format!("writing {}", path.display()))?;
    ui::ok("init", format!("wrote {}", path.display()));
    ui::info("next", "edit the services, then `foreman start`");
    Ok(())
}
