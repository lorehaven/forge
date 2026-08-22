//! What each command does.

use crate::cli::*;
use crate::client::Client;
use anyhow::{Context, Result, bail};
use futures_util::StreamExt;
use quench_cli::prelude::{Tone, print_status};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::time::Duration;

/// How often `--wait` asks whether a run has finished.
const POLL: Duration = Duration::from_millis(750);

// ---------------------------------------------------------------------------
// Repositories
// ---------------------------------------------------------------------------

pub async fn repo(client: &Client, command: &RepoCommands) -> Result<()> {
    match command {
        RepoCommands::Add(args) => {
            let (owner, name) = split_slug(&args.slug)?;
            let created: Value = client
                .post(
                    "/repos",
                    &json!({
                        "provider": args.provider,
                        "owner": owner,
                        "name": name,
                        "clone_url": args.clone_url,
                        "default_branch": args.default_branch,
                        "project_id": resolve_project(&args.project),
                    }),
                )
                .await?;

            print_status(
                Tone::Success,
                "registered",
                &format!("{}/{} ({})", owner, name, string(&created, "id")),
            );
            Ok(())
        }

        RepoCommands::List => {
            let repos: Vec<Value> = client.get("/repos").await?;
            if repos.is_empty() {
                print_status(Tone::Info, "repos", "none registered");
                return Ok(());
            }
            for repo in &repos {
                let state = if repo["enabled"].as_bool().unwrap_or(false) {
                    "enabled"
                } else {
                    "disabled"
                };
                println!(
                    "{:<40}  {:<9}  {:<8}  {}",
                    format!("{}/{}", string(repo, "owner"), string(repo, "name")),
                    string(repo, "provider"),
                    state,
                    string(repo, "id")
                );
            }
            Ok(())
        }

        RepoCommands::Enable(args) => set_enabled(client, &args.repo, true).await,
        RepoCommands::Disable(args) => set_enabled(client, &args.repo, false).await,

        RepoCommands::Move(args) => {
            let id = resolve_repo(client, &args.repo).await?;
            let _: Value = client
                .patch(
                    &format!("/repos/{id}"),
                    &json!({ "project_id": resolve_project(&args.project) }),
                )
                .await?;
            print_status(Tone::Success, "moved", &args.repo);
            Ok(())
        }

        RepoCommands::SetBranch(args) => {
            let id = resolve_repo(client, &args.repo).await?;
            let _: Value = client
                .patch(
                    &format!("/repos/{id}"),
                    &json!({ "default_branch": args.branch }),
                )
                .await?;
            print_status(Tone::Success, "default branch", &args.branch);
            Ok(())
        }

        RepoCommands::Remove(args) => {
            let id = resolve_repo(client, &args.repo).await?;
            client
                .send_empty(reqwest::Method::DELETE, &format!("/repos/{id}"))
                .await?;
            print_status(Tone::Success, "removed", &args.repo);
            Ok(())
        }
    }
}

pub async fn set_enabled(client: &Client, repo: &str, enabled: bool) -> Result<()> {
    let id = resolve_repo(client, repo).await?;
    let _: Value = client
        .post(
            &format!("/repos/{id}/enabled"),
            &json!({ "enabled": enabled }),
        )
        .await?;

    print_status(
        Tone::Success,
        if enabled { "enabled" } else { "disabled" },
        repo,
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Projects
// ---------------------------------------------------------------------------

pub async fn project(client: &Client, command: &ProjectCommands) -> Result<()> {
    match command {
        ProjectCommands::Add(args) => {
            let created: Value = client
                .post(
                    "/projects",
                    &json!({ "name": args.name, "parent_id": args.parent }),
                )
                .await?;
            print_status(
                Tone::Success,
                "created",
                &format!("{} ({})", args.name, string(&created, "id")),
            );
            Ok(())
        }

        ProjectCommands::List(args) => {
            let mut path = "/projects".to_string();
            if let Some(parent) = &args.parent {
                path.push_str(&format!("?parent_id={parent}"));
            }
            let projects: Vec<Value> = client.get(&path).await?;
            if projects.is_empty() {
                print_status(Tone::Info, "projects", "none");
                return Ok(());
            }
            for project in &projects {
                println!("{:<30}  {}", string(project, "name"), string(project, "id"));
            }
            Ok(())
        }

        ProjectCommands::Show(args) => {
            let project: Value = client.get(&format!("/projects/{}", args.project)).await?;
            print_status(Tone::Info, "project", &string(&project, "path"));
            println!("id:      {}", string(&project, "id"));
            match project.get("parent_id").and_then(Value::as_str) {
                Some(parent_id) => println!("parent:  {parent_id}"),
                None => println!("parent:  (root)"),
            }
            Ok(())
        }

        ProjectCommands::Rename(args) => {
            let _: Value = client
                .patch(
                    &format!("/projects/{}", args.project),
                    &json!({ "name": args.name }),
                )
                .await?;
            print_status(Tone::Success, "renamed", &args.name);
            Ok(())
        }

        ProjectCommands::Move(args) => {
            if !args.to_root && args.parent.is_none() {
                bail!("pass --parent or --to-root");
            }
            let body = if args.to_root {
                json!({ "to_root": true })
            } else {
                json!({ "parent_id": args.parent })
            };
            let _: Value = client
                .patch(&format!("/projects/{}", args.project), &body)
                .await?;
            print_status(Tone::Success, "moved", &args.project);
            Ok(())
        }

        ProjectCommands::Remove(args) => {
            client
                .send_empty(
                    reqwest::Method::DELETE,
                    &format!("/projects/{}", args.project),
                )
                .await?;
            print_status(Tone::Success, "removed", &args.project);
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Runs
// ---------------------------------------------------------------------------

pub async fn run(client: &Client, args: &RunArgs) -> Result<()> {
    let id = resolve_repo(client, &args.repo).await?;

    let mut body = json!({});
    if let Some(git_ref) = &args.git_ref {
        body["git_ref"] = json!(git_ref);
    }
    if let Some(sha) = &args.sha {
        body["sha"] = json!(sha);
    }

    let started: Value = client.post(&format!("/repos/{id}/runs"), &body).await?;
    let run_id = string(&started, "id");

    print_status(
        Tone::Info,
        "queued",
        &format!("{} at {}", string(&started, "git_ref"), short(&started)),
    );
    println!("{run_id}");

    if args.wait {
        return wait_for(client, &run_id).await;
    }
    Ok(())
}

/// Polls until the run rests, then exits with its verdict.
///
/// A non-zero exit for a failed run is the point: it makes `conveyor run --wait`
/// usable as the last line of a script.
pub async fn wait_for(client: &Client, run_id: &str) -> Result<()> {
    let mut last = String::new();

    loop {
        let run: Value = client.get(&format!("/runs/{run_id}")).await?;
        let status = string(&run, "status");

        if status != last {
            print_status(tone_for(&status), "run", &status);
            last = status.clone();
        }

        match status.as_str() {
            "success" | "skipped" => return Ok(()),
            "failed" | "cancelled" => {
                let reason = run["error"].as_str().unwrap_or("").to_string();
                bail!(if reason.is_empty() {
                    format!("run {status}")
                } else {
                    format!("run {status}: {reason}")
                });
            }
            _ => tokio::time::sleep(POLL).await,
        }
    }
}

pub async fn runs(client: &Client, args: &RunsArgs) -> Result<()> {
    let mut path = format!("/runs?limit={}", args.limit);
    if let Some(repo) = &args.repo {
        path.push_str(&format!("&repo_id={}", resolve_repo(client, repo).await?));
    }

    let runs: Vec<Value> = client.get(&path).await?;
    if runs.is_empty() {
        print_status(Tone::Info, "runs", "none yet");
        return Ok(());
    }

    for run in &runs {
        println!(
            "{:<10}  {:<28}  {:<10}  {}",
            string(run, "status"),
            string(run, "git_ref"),
            short(run),
            string(run, "id")
        );
    }
    Ok(())
}

pub async fn show(client: &Client, args: &ShowArgs) -> Result<()> {
    let run: Value = client.get(&format!("/runs/{}", args.run_id)).await?;

    print_status(
        tone_for(&string(&run, "status")),
        &string(&run, "status"),
        &format!("{} at {}", string(&run, "git_ref"), short(&run)),
    );

    if let Some(error) = run["error"].as_str().filter(|e| !e.is_empty()) {
        println!("  {error}");
    }

    println!("\njobs:");
    for job in run["jobs"].as_array().unwrap_or(&vec![]) {
        println!(
            "  {:<10}  {}/{:<24}  {}",
            string(job, "status"),
            string(job, "stage"),
            string(job, "name"),
            job["error"].as_str().unwrap_or("")
        );
        println!("             {}", string(job, "id"));
    }

    let artifacts = run["artifacts"].as_array().cloned().unwrap_or_default();
    if !artifacts.is_empty() {
        println!("\nartifacts:");
        for artifact in &artifacts {
            println!(
                "  {:<28}  {}",
                string(artifact, "name"),
                string(artifact, "uri")
            );
        }
    }
    Ok(())
}

pub async fn cancel(client: &Client, args: &CancelArgs) -> Result<()> {
    client
        .send_empty(
            reqwest::Method::POST,
            &format!("/runs/{}/cancel", args.run_id),
        )
        .await?;
    print_status(Tone::Info, "cancelling", &args.run_id);
    Ok(())
}

// ---------------------------------------------------------------------------
// Logs
// ---------------------------------------------------------------------------

pub async fn logs(client: &Client, args: &LogsArgs) -> Result<()> {
    // A run id is the useful thing to have to hand, so accept either and work
    // out which this is.
    let jobs = match client.get::<Value>(&format!("/runs/{}", args.id)).await {
        Ok(run) => run["jobs"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|job| {
                (
                    string(job, "id"),
                    format!("{}/{}", string(job, "stage"), string(job, "name")),
                )
            })
            .collect(),
        // Not a run; treat it as a job id.
        Err(_) => vec![(args.id.clone(), String::new())],
    };

    for (job_id, name) in jobs {
        if !name.is_empty() {
            print_status(Tone::Info, "job", &name);
        }
        if args.follow {
            follow(client, &job_id).await?;
        } else {
            print_stored(client, &job_id).await?;
        }
    }
    Ok(())
}

pub async fn print_stored(client: &Client, job_id: &str) -> Result<()> {
    let chunks: Vec<Value> = client.get(&format!("/jobs/{job_id}/logs")).await?;
    for chunk in &chunks {
        write_line(&string(chunk, "stream"), &string(chunk, "line"));
    }
    Ok(())
}

/// Reads the server-sent event stream, printing lines as they arrive.
pub async fn follow(client: &Client, job_id: &str) -> Result<()> {
    let response = client.stream(&format!("/jobs/{job_id}/stream")).await?;
    let mut body = response.bytes_stream();

    // SSE frames are separated by a blank line and can arrive split across
    // chunks, so they are reassembled here rather than assumed whole.
    let mut buffer = String::new();
    let mut event = String::new();

    while let Some(chunk) = body.next().await {
        buffer.push_str(&String::from_utf8_lossy(
            &chunk.context("reading the stream")?,
        ));

        while let Some(end) = buffer.find("\n\n") {
            let frame = buffer[..end].to_string();
            buffer.drain(..end + 2);

            let mut data = String::new();
            for line in frame.lines() {
                if let Some(rest) = line.strip_prefix("event: ") {
                    event = rest.to_string();
                } else if let Some(rest) = line.strip_prefix("data: ") {
                    data = rest.to_string();
                }
            }

            if event == "done" {
                return Ok(());
            }
            if !data.is_empty() {
                write_line(&event, &data);
            }
        }
    }
    Ok(())
}

pub fn write_line(stream: &str, line: &str) {
    // stderr to stderr, so `conveyor logs > build.txt` keeps the two apart the
    // way the build itself did.
    if stream == "stderr" {
        let _ = writeln!(std::io::stderr(), "{line}");
    } else {
        println!("{line}");
    }
}

// ---------------------------------------------------------------------------
// Secrets
// ---------------------------------------------------------------------------

pub async fn secret(client: &Client, command: &SecretCommands) -> Result<()> {
    match command {
        SecretCommands::Set(args) => {
            let value = match &args.value {
                Some(value) => value.clone(),
                // Reading from stdin keeps the value out of shell history and
                // out of the process list, where anyone on the machine can see
                // it with `ps`.
                None => {
                    let mut input = String::new();
                    std::io::stdin()
                        .read_to_string(&mut input)
                        .context("reading the value from stdin")?;
                    input.trim_end_matches('\n').to_string()
                }
            };

            let path = secret_path(client, args.repo.as_deref(), Some(&args.name)).await?;
            let _: Value = client.put(&path, &json!({ "value": value })).await?;
            print_status(Tone::Success, "set", &args.name);
            Ok(())
        }

        SecretCommands::List(args) => {
            let path = secret_path(client, args.repo.as_deref(), None).await?;
            let secrets: Vec<Value> = client.get(&path).await?;
            if secrets.is_empty() {
                print_status(Tone::Info, "secrets", "none set");
                return Ok(());
            }
            for secret in &secrets {
                println!(
                    "{:<30}  set by {}",
                    string(secret, "name"),
                    string(secret, "created_by")
                );
            }
            Ok(())
        }

        SecretCommands::Remove(args) => {
            let path = secret_path(client, args.repo.as_deref(), Some(&args.name)).await?;
            client.send_empty(reqwest::Method::DELETE, &path).await?;
            print_status(Tone::Success, "removed", &args.name);
            Ok(())
        }
    }
}

pub async fn secret_path(
    client: &Client,
    repo: Option<&str>,
    name: Option<&str>,
) -> Result<String> {
    let base = match repo {
        Some(repo) => format!("/repos/{}/secrets", resolve_repo(client, repo).await?),
        None => "/secrets".to_string(),
    };
    Ok(match name {
        Some(name) => format!("{base}/{name}"),
        None => base,
    })
}

// ---------------------------------------------------------------------------
// Credentials
// ---------------------------------------------------------------------------

pub async fn credential(client: &Client, command: &CredentialCommands) -> Result<()> {
    match command {
        CredentialCommands::Set(args) => {
            let token = match &args.token {
                Some(token) => token.clone(),
                // Same reasoning as `secret set`: keeps it out of shell
                // history and out of the process list.
                None => {
                    let mut input = String::new();
                    std::io::stdin()
                        .read_to_string(&mut input)
                        .context("reading the token from stdin")?;
                    input.trim_end_matches('\n').to_string()
                }
            };

            let path =
                credential_path(client, args.repo.as_deref(), args.project.as_deref()).await?;
            let credential: Value = client
                .put(
                    &path,
                    &json!({ "name": args.name, "username": args.git_username, "token": token }),
                )
                .await?;
            print_status(
                Tone::Success,
                "set",
                &format!("{} ({})", args.name, string(&credential, "preview")),
            );
            Ok(())
        }

        CredentialCommands::Show(args) => {
            let path =
                credential_path(client, args.repo.as_deref(), args.project.as_deref()).await?;
            let credential: Value = client.get(&path).await?;
            if credential.is_null() {
                print_status(Tone::Info, "credential", "none set");
                return Ok(());
            }
            println!(
                "{:<20}  {} as {}, set by {}",
                string(&credential, "name"),
                string(&credential, "preview"),
                string(&credential, "username"),
                string(&credential, "created_by"),
            );
            Ok(())
        }

        CredentialCommands::Remove(args) => {
            let path =
                credential_path(client, args.repo.as_deref(), args.project.as_deref()).await?;
            client.send_empty(reqwest::Method::DELETE, &path).await?;
            print_status(Tone::Success, "removed", "the credential");
            Ok(())
        }
    }
}

pub async fn credential_path(
    client: &Client,
    repo: Option<&str>,
    project: Option<&str>,
) -> Result<String> {
    match (repo, project) {
        (Some(_), Some(_)) => bail!("pass --repo or --project, not both"),
        (Some(repo), None) => Ok(format!(
            "/repos/{}/credentials",
            resolve_repo(client, repo).await?
        )),
        (None, Some(project)) => Ok(format!(
            "/projects/{}/credentials",
            resolve_project(project)
        )),
        (None, None) => bail!("a credential needs a scope: pass --repo or --project"),
    }
}

/// Unlike `resolve_repo`, no name-style lookup: a project's name is only
/// unique among its own siblings (`conveyor project list` is how you find an
/// id from a name), not across the whole tree the way a repository's
/// `owner/name` is, so there is no single unambiguous string to resolve
/// against here. A project reference is its id, taken as given.
pub fn resolve_project(reference: &str) -> String {
    reference.to_string()
}

// ---------------------------------------------------------------------------
// Validate
// ---------------------------------------------------------------------------

/// Checks a pipeline without a running conveyor.
///
/// The same parser the service uses, linked in - so what this accepts is
/// exactly what a run will accept, rather than a second implementation that
/// agrees with it most of the time.
pub fn validate(args: &ValidateArgs) -> Result<()> {
    let source = std::fs::read_to_string(&args.path)
        .with_context(|| format!("could not read {}", args.path))?;

    match conveyor_pipeline::parse(&source) {
        Ok(spec) => {
            print_status(
                Tone::Success,
                "valid",
                &format!(
                    "{} stage(s), {} job(s)",
                    spec.stages.len(),
                    spec.job_count()
                ),
            );
            for stage in spec.stages_in_order() {
                let needs = if stage.needs.is_empty() {
                    String::new()
                } else {
                    format!("  needs {}", stage.needs.join(", "))
                };
                println!("  {}{needs}", stage.name);
                for job in &stage.jobs {
                    println!("    {} ({} step(s))", job.name, job.steps.len());
                }
            }
            Ok(())
        }
        Err(error) => {
            print_status(Tone::Error, "invalid", &args.path);
            bail!("{error}");
        }
    }
}

// ---------------------------------------------------------------------------
// Shared
// ---------------------------------------------------------------------------

/// Accepts `owner/name` or an id, because both are things people have to hand.
pub async fn resolve_repo(client: &Client, reference: &str) -> Result<String> {
    if !reference.contains('/') {
        return Ok(reference.to_string());
    }

    let (owner, name) = split_slug(reference)?;
    let repos: Vec<Value> = client.get("/repos").await?;

    repos
        .iter()
        .find(|repo| string(repo, "owner") == owner && string(repo, "name") == name)
        .map(|repo| string(repo, "id"))
        .with_context(|| format!("{reference} is not registered with conveyor"))
}

pub fn split_slug(slug: &str) -> Result<(String, String)> {
    slug.split_once('/')
        .map(|(owner, name)| (owner.to_string(), name.to_string()))
        .with_context(|| format!("expected owner/name, got `{slug}`"))
}

pub fn string(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_string()
}

pub fn short(run: &Value) -> String {
    let sha = string(run, "sha");
    sha.chars().take(7).collect()
}

pub const fn tone_for(status: &str) -> Tone {
    match status.as_bytes() {
        b"success" => Tone::Success,
        b"failed" | b"cancelled" => Tone::Error,
        _ => Tone::Info,
    }
}
