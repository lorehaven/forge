//! The `kubectl` commands the runner plans, carried out against the cluster directly.
//!
//! The planner (`commands.rs`) still writes each step as the command a person would type - that is what the Job's log shows and
//! what the tests read - but the runner image has no `kubectl`: `ProcessExec` hands those commands to this module, which reads
//! the few shapes the planner produces and does the same through [`kube_cluster`], with the Job's service account.
//!
//! The shapes: `apply -f FILE`, `scale KIND/NAME --replicas=N`, `delete KIND/NAME [--ignore-not-found] [--timeout=Ns]`,
//! `delete pods -l SELECTOR`, `get KINDS -o json`, `get pods -l SELECTOR -o name`, and `rollout status|restart|undo KIND/NAME`,
//! each with `-n NAMESPACE` or `-A`. Anything else is refused rather than guessed at.

use kube_cluster::cluster::Cluster;
use quench_serde::Value;
use std::time::Duration;

/// A command line read.
#[derive(Debug, Default, PartialEq, Eq)]
struct Parsed {
    namespace: Option<String>,
    all: bool,
    selector: Option<String>,
    output: Option<String>,
    file: Option<String>,
    flags: Vec<String>,
    words: Vec<String>,
}

fn parse(args: &[String]) -> Result<Parsed, String> {
    let mut parsed = Parsed::default();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let mut value = |name: &str| {
            rest.next()
                .cloned()
                .ok_or_else(|| format!("`{name}` needs a value"))
        };
        match arg.as_str() {
            "-n" | "--namespace" => parsed.namespace = Some(value("-n")?),
            "-A" | "--all-namespaces" => parsed.all = true,
            "-l" | "--selector" => parsed.selector = Some(value("-l")?),
            "-o" | "--output" => parsed.output = Some(value("-o")?),
            "-f" | "--filename" => parsed.file = Some(value("-f")?),
            flag if flag.starts_with("--") => parsed.flags.push(flag.to_string()),
            word => parsed.words.push(word.to_string()),
        }
    }
    Ok(parsed)
}

impl Parsed {
    /// The value of `--name=value`.
    fn flag(&self, name: &str) -> Option<&str> {
        self.flags
            .iter()
            .find_map(|f| f.strip_prefix(name).and_then(|rest| rest.strip_prefix('=')))
    }

    fn has(&self, name: &str) -> bool {
        self.flags.iter().any(|f| f == name)
    }

    fn word(&self, at: usize, what: &str) -> Result<&str, String> {
        self.words
            .get(at)
            .map(String::as_str)
            .ok_or_else(|| format!("{what} is missing"))
    }
}

/// `30s`, `2m` or `45` as a duration.
fn duration(text: &str) -> Result<Duration, String> {
    let (number, unit) = text
        .find(|c: char| !c.is_ascii_digit())
        .map_or((text, "s"), |at| text.split_at(at));
    let value: u64 = number
        .parse()
        .map_err(|_| format!("`{text}` is not a duration"))?;
    match unit {
        "s" | "" => Ok(Duration::from_secs(value)),
        "m" => Ok(Duration::from_secs(value * 60)),
        "h" => Ok(Duration::from_secs(value * 3600)),
        other => Err(format!("`{text}`: unknown unit `{other}`")),
    }
}

fn kind_and_name(spec: &str) -> Result<(&str, &str), String> {
    spec.split_once('/')
        .ok_or_else(|| format!("`{spec}` is not kind/name"))
}

/// What a command printed, and how it ended.
#[allow(
    clippy::too_many_lines,
    reason = "one arm per command the planner writes"
)]
fn execute(args: &[String]) -> Result<String, String> {
    let parsed = parse(args)?;
    let verb = parsed.word(0, "the command")?;
    let cluster = Cluster::connect(None).map_err(|e| e.to_string())?;
    let namespace = parsed.namespace.as_deref();
    let scope = if parsed.all { None } else { namespace };
    let at = |cluster: &Cluster| {
        namespace.map_or_else(|| cluster.default_namespace().to_string(), str::to_string)
    };

    match verb {
        "apply" => {
            let file = parsed.file.as_deref().ok_or("apply needs -f FILE")?;
            let docs = kube_cluster::cluster::read_manifest(file).map_err(|e| e.to_string())?;
            cluster.apply(&docs).map_err(|e| e.to_string())?;
            Ok(String::new())
        }
        "scale" => {
            let spec = parsed.word(1, "the workload")?;
            let (kind, name) = kind_and_name(spec)?;
            let replicas: u32 = parsed
                .flag("--replicas")
                .ok_or("scale needs --replicas=N")?
                .parse()
                .map_err(|_| "--replicas is not a number".to_string())?;
            cluster
                .scale(kind, name, &at(&cluster), replicas)
                .map_err(|e| e.to_string())?;
            Ok(format!("{spec} scaled\n"))
        }
        "delete" => {
            let target = parsed.word(1, "what to delete")?;
            if let Some(selector) = &parsed.selector {
                let count = cluster
                    .delete_matching(target.trim_end_matches('s'), selector, scope)
                    .map_err(|e| e.to_string())?;
                return Ok(format!("{count} {target} deleted\n"));
            }
            let timeout = parsed
                .flag("--timeout")
                .map_or(Ok(Duration::from_secs(30)), duration)?;
            let gone = cluster
                .delete_resource(target, namespace, parsed.has("--ignore-not-found"), timeout)
                .map_err(|e| e.to_string())?;
            Ok(if gone {
                format!("{target} deleted\n")
            } else {
                String::new()
            })
        }
        "get" => {
            let kinds: Vec<&str> = parsed.word(1, "what to get")?.split(',').collect();
            let selector = parsed.selector.as_deref().unwrap_or("");
            let objects = cluster
                .objects(&kinds, selector, scope)
                .map_err(|e| e.to_string())?;
            match parsed.output.as_deref() {
                Some("name") => Ok(objects.iter().fold(String::new(), |text, o| {
                    text + &format!(
                        "{}/{}\n",
                        o["kind"].as_str().unwrap_or("object").to_ascii_lowercase(),
                        o["metadata"]["name"].as_str().unwrap_or("?")
                    )
                })),
                Some("json") => {
                    let mut list = quench_serde::Map::new();
                    list.insert("apiVersion", Value::from("v1"));
                    list.insert("kind", Value::from("List"));
                    list.insert("items", Value::Array(objects));
                    Ok(quench_serde::json::to_string(&Value::Object(list)))
                }
                other => Err(format!("get -o {other:?} is not supported")),
            }
        }
        "rollout" => {
            let action = parsed.word(1, "the rollout action")?;
            let spec = parsed.word(2, "the workload")?;
            let (kind, name) = kind_and_name(spec)?;
            let ns = at(&cluster);
            match action {
                "status" => {
                    let timeout = parsed
                        .flag("--timeout")
                        .map_or(Ok(Duration::from_secs(300)), duration)?;
                    cluster
                        .rollout_status(kind, name, Some(&ns), timeout)
                        .map_err(|e| e.to_string())?;
                    Ok(String::new())
                }
                "restart" => {
                    cluster
                        .restart(kind, name, &ns)
                        .map_err(|e| e.to_string())?;
                    Ok(format!("{spec} restarted\n"))
                }
                "undo" => cluster
                    .undo(kind, name, &ns)
                    .map(|said| format!("{said}\n"))
                    .map_err(|e| e.to_string()),
                other => Err(format!("rollout {other} is not supported")),
            }
        }
        other => Err(format!("`{other}` is not a command the runner knows")),
    }
}

/// Runs a command, printing what it says; returns the exit code (0 or 1) and what it printed.
#[must_use]
pub fn run(args: &[String]) -> (i32, String) {
    match execute(args) {
        Ok(text) => {
            print!("{text}");
            (0, text)
        }
        Err(why) => {
            eprintln!("error: {why}");
            (1, String::new())
        }
    }
}

/// Runs a command whose answer is read: the exit code and what it printed, which is not echoed.
#[must_use]
pub fn capture(args: &[String]) -> (i32, String) {
    match execute(args) {
        Ok(text) => (0, text),
        Err(why) => {
            eprintln!("error: {why}");
            (1, String::new())
        }
    }
}
