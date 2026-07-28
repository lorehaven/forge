//! Tasks: the work that has to happen around the services rather than in them.
//!
//! Schema installation is the reason this exists. It has to run after the
//! database is up and before the first service connects, and it has to know
//! which services are starting - a subset of the estate installs a subset of
//! the schemas, so they arrive when the service that needs them does rather
//! than all at once at the first boot.

use anyhow::{Context, Result, bail};
use std::path::PathBuf;

use crate::config::{Role, SelectionMode, Task};
use crate::docker;
use crate::estate::Estate;
use crate::services;
use crate::ui;

/// The element of a task's `command` that the per-service arguments replace.
const SELECTION_PLACEHOLDER: &str = "${selection}";

pub fn run_named(estate: &Estate, name: &str, selection: &[String]) -> Result<()> {
    let task = estate.config.tasks.get(name).with_context(|| {
        let known: Vec<&str> = estate.config.tasks.keys().map(String::as_str).collect();
        format!(
            "unknown task '{name}'{}",
            if known.is_empty() {
                String::new()
            } else {
                format!(" (known: {})", known.join(", "))
            }
        )
    })?;

    run(estate, name, task, selection)
}

pub fn run_role(estate: &Estate, role: Role, selection: &[String]) -> Result<()> {
    for (name, task) in estate.config.tasks_with_role(role) {
        run(estate, name, task, selection)?;
    }
    Ok(())
}

/// Containers a role's tasks need, so `foreman db` can bring up the database
/// without starting everything else the estate sits on.
pub fn containers_for_role(estate: &Estate, role: Role) -> Vec<String> {
    let mut wanted: Vec<String> = Vec::new();
    for (_, task) in estate.config.tasks_with_role(role) {
        for container in &task.containers {
            if !wanted.contains(container) {
                wanted.push(container.clone());
            }
        }
    }
    wanted
}

pub fn run(estate: &Estate, name: &str, task: &Task, selection: &[String]) -> Result<()> {
    if task.stop_services {
        services::stop(estate, &estate.service_names())?;
    }

    for container in &task.containers {
        let container = estate
            .config
            .container(container)
            .with_context(|| format!("task '{name}' wants container '{container}'"))?;
        docker::start(container, &estate.scope())?;
    }

    let scope = estate.scope();

    if let Some(warning) = &task.warn {
        ui::warn(name, scope.expand(warning)?);
    }

    // Whether the selection reaches the command line at all. Starting the whole
    // estate means the task's own configuration already covers everything, so
    // narrowing it would be a no-op at best and a mistake at worst.
    let scoped = match task.selection {
        SelectionMode::Never => false,
        SelectionMode::Always => !task.each_selected.is_empty(),
        SelectionMode::SubsetOnly => {
            !task.each_selected.is_empty()
                && !selection.is_empty()
                && !estate.is_whole_estate(selection)
        }
    };

    let mut selection_args = Vec::new();
    if scoped {
        for service in selection {
            let service_scope = estate
                .scope()
                .with("service", service)
                .with("name", service);
            selection_args.extend(service_scope.expand_all(&task.each_selected)?);
        }
        ui::info(name, format!("applying {}", selection.join(", ")));
    } else if let Some(description) = &task.description {
        ui::info(name, scope.expand(description)?);
    }

    if let Some((program, args)) = task.build.split_first() {
        let args = scope.expand_all(args)?;
        if !crate::process::run(&scope.expand(program)?, &args, &estate.root, &[])? {
            bail!("task '{name}' build step failed");
        }
    }

    // The placeholder marks where the per-service arguments belong. Position
    // matters: a trailing verb has to stay last.
    let mut command = Vec::new();
    let mut placed = false;
    for element in &task.command {
        if element == SELECTION_PLACEHOLDER {
            command.extend(selection_args.clone());
            placed = true;
        } else {
            command.push(scope.expand(element)?);
        }
    }
    if !placed {
        command.extend(selection_args);
    }

    let Some((program, args)) = command.split_first() else {
        bail!("task '{name}' has an empty command");
    };

    let workdir: PathBuf = match &task.workdir {
        Some(dir) => estate.path(&scope.expand(dir)?),
        None => estate.root.clone(),
    };
    let env = scope.expand_map(&task.env)?;

    if !crate::process::run(program, args, &workdir, &env)? {
        bail!("task '{name}' failed");
    }

    if let Some(done) = &task.done {
        ui::ok(name, scope.expand(done)?);
    }

    Ok(())
}
