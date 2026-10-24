//! Turning a job into cluster objects. Kept client-free so it's unit-testable without a cluster.
//! One `batch/v1` Job per conveyor job; an init container fetches the commit into an `emptyDir`.

use crate::executors::engine::{JobCredential, JobSpec};
use crate::workspace::checkout::basic_auth_header;
use quench_serde::{Value, json};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Where the checkout lands inside both containers.
pub const WORKSPACE_PATH: &str = "/workspace";

/// The volume the two containers share.
const WORKSPACE_VOLUME: &str = "workspace";

/// Alpine's git image is small and does exactly one thing.
const DEFAULT_GIT_IMAGE: &str = "alpine/git:latest";

/// What a job runs in when its pipeline names no image.
const DEFAULT_IMAGE: &str = "alpine:3.22";

/// Printed to stderr (not stdout, or a step piping its own stdout would swallow it) before each step.
pub const STEP_MARKER: &str = "##conveyor-step:";

/// Starts a line of a file the job hands back.
///
/// The line is `<marker><path>\t<base64>`, as many lines as the file needs. Stdout, since stderr is where the
/// step markers are; consumed by the executor, never shown in the log.
pub const FILE_MARKER: &str = "##conveyor-file:";

/// Bigger files are not handed back: they travel through the log stream, which is not a file transfer.
pub const MAX_EXPORT_BYTES: usize = 24 * 1024 * 1024;

/// Characters of base64 per line, so no log line is unreasonably long.
const EXPORT_LINE: usize = 4096;

/// Everything a job needs in the cluster.
#[derive(Debug)]
pub struct Manifest {
    pub name: String,
    pub job: Value,
    /// Present only when the job was given secrets; created before the job, deleted with it.
    pub secret: Option<Value>,
}

/// DNS-1123: lowercase alphanumerics/dashes, ≤63 chars - doesn't assume job ids are uuids.
#[must_use]
pub fn object_name(job_id: &str) -> String {
    let cleaned: String = job_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();

    let trimmed = cleaned.trim_matches('-');
    let body: String = trimmed.chars().take(54).collect();
    let body = body.trim_end_matches('-');

    if body.is_empty() {
        return "conveyor-job".to_string();
    }
    format!("conveyor-{body}")
}

/// `managed-by` is what a cleanup selects a stray job on.
#[must_use]
pub fn labels(spec: &JobSpec, name: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "app.kubernetes.io/managed-by".to_string(),
            "conveyor".to_string(),
        ),
        ("app.kubernetes.io/name".to_string(), name.to_string()),
        ("conveyor.forge/job-id".to_string(), spec.id.clone()),
    ])
}

/// `set -e` is deliberately absent - each step's failure is handled explicitly so its own exit code comes back.
#[must_use]
pub fn script(commands: &[Vec<String>]) -> String {
    script_with_exports(commands, &[])
}

/// [`script`] that also prints the `exports` that exist as [`FILE_MARKER`] lines, however it ends.
///
/// A failed step included. The pod's checkout is not conveyor's, so this is how a report or an artifact leaves it.
#[must_use]
pub fn script_with_exports(commands: &[Vec<String>], exports: &[String]) -> String {
    let mut script = String::new();

    if !exports.is_empty() {
        let files = exports
            .iter()
            .map(|path| shell_quote(path))
            .collect::<Vec<_>>()
            .join(" ");
        // `trap ... EXIT` leaves the exit status as it was.
        let _ = write!(
            script,
            "__conveyor_export() {{\n\
             for __f in {files}; do\n\
             if [ -f \"$__f\" ] && [ \"$(wc -c < \"$__f\")\" -le {MAX_EXPORT_BYTES} ]; then\n\
             base64 \"$__f\" | tr -d '\\n' | fold -w {EXPORT_LINE} | while IFS= read -r __c || [ -n \"$__c\" ]; do\n\
             printf '%s%s\\t%s\\n' '{FILE_MARKER}' \"$__f\" \"$__c\"\n\
             done\n\
             fi\n\
             done\n\
             }}\n\
             trap __conveyor_export EXIT\n"
        );
    }

    for (ordinal, argv) in commands.iter().enumerate() {
        let rendered = argv
            .iter()
            .map(|argument| shell_quote(argument))
            .collect::<Vec<_>>()
            .join(" ");

        let _ = writeln!(script, "printf '%s%d\\n' '{STEP_MARKER}' {ordinal} >&2");
        let _ = writeln!(script, "{rendered}");
        script.push_str("__status=$?\n");
        script.push_str("if [ $__status -ne 0 ]; then exit $__status; fi\n");
    }

    script
}

/// Wraps a value so a shell reads it as one argument, whatever is in it.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// Builds the objects for one job.
#[must_use]
pub fn build(
    spec: &JobSpec,
    commands: &[Vec<String>],
    namespace: &str,
    settings: &Settings,
) -> Manifest {
    let name = object_name(&spec.id);
    let labels = labels(spec, &name);

    // Not inline in the pod spec, which anyone who can read pods can read.
    let secret = (!spec.env.is_empty()).then(|| {
        json!({
            "apiVersion": "v1",
            "kind": "Secret",
            "metadata": { "name": name, "namespace": namespace, "labels": labels },
            "stringData": spec.env,
        })
    });

    let volume_mount = json!({ "name": WORKSPACE_VOLUME, "mountPath": WORKSPACE_PATH });

    let mut pod = json!({
        "restartPolicy": "Never",
        "containers": [{
            "name": "steps",
            "image": spec.image.as_deref().unwrap_or(&settings.default_image),
            "command": ["sh", "-c"],
            "args": [script_with_exports(commands, &spec.exports)],
            "workingDir": WORKSPACE_PATH,
            "volumeMounts": [volume_mount],
            "securityContext": hardened(),
        }],
        "volumes": [{ "name": WORKSPACE_VOLUME, "emptyDir": {} }],
    });

    if let Some(source) = &spec.source {
        let mut checkout = json!({
            "name": "checkout",
            "image": settings.git_image,
            "command": ["sh", "-c"],
            "args": [checkout_script(&source.clone_url, &source.git_ref, &source.sha)],
            "workingDir": WORKSPACE_PATH,
            "volumeMounts": [volume_mount],
            "securityContext": hardened(),
        });
        // Only the checkout container sees this; the step container never gets conveyor's own git credential.
        if let Some(env) = credential_env_vars(source.credential.as_ref()) {
            checkout["env"] = env;
        }
        pod["initContainers"] = json!([checkout]);
    }

    if secret.is_some() {
        pod["containers"][0]["envFrom"] =
            json!([{ "secretRef": { "name": name, "optional": false } }]);
    }

    if let Some(account) = &settings.service_account {
        pod["serviceAccountName"] = json!(account);
    }

    let job = json!({
        "apiVersion": "batch/v1",
        "kind": "Job",
        "metadata": { "name": name, "namespace": namespace, "labels": labels },
        "spec": {
            // Conveyor owns retries via the queue; a silent second attempt would double-count as one run.
            "backoffLimit": 0,
            "activeDeadlineSeconds": spec.timeout.as_secs().max(1),
            // Backstop for a conveyor that died before `forget` could delete this.
            "ttlSecondsAfterFinished": settings.ttl_seconds,
            "template": { "metadata": { "labels": labels }, "spec": pod },
        },
    });

    Manifest { name, job, secret }
}

/// Hands the checkout container its credential via `GIT_CONFIG_*`, not embedded in `checkout_script`'s argv text.
fn credential_env_vars(credential: Option<&JobCredential>) -> Option<Value> {
    let credential = credential?;
    let header = basic_auth_header(&credential.username, &credential.token);

    Some(json!([
        { "name": "GIT_CONFIG_COUNT", "value": "1" },
        { "name": "GIT_CONFIG_KEY_0", "value": "http.extraheader" },
        { "name": "GIT_CONFIG_VALUE_0", "value": header },
    ]))
}

/// Mirrors `workspace::checkout`: try the commit directly, fall back to fetching the ref in full.
fn checkout_script(clone_url: &str, git_ref: &str, sha: &str) -> String {
    format!(
        "set -e\n\
         export GIT_TERMINAL_PROMPT=0\n\
         git init --quiet .\n\
         git remote add origin {url}\n\
         git fetch --quiet --no-tags --depth 1 origin {sha} \
           || git fetch --quiet --no-tags origin {git_ref}\n\
         git checkout --quiet --detach {sha}\n",
        url = shell_quote(clone_url),
        sha = shell_quote(sha),
        git_ref = shell_quote(git_ref),
    )
}

/// Not a full lockdown (a build legitimately writes files/spawns processes), just no privilege escalation.
fn hardened() -> Value {
    json!({ "allowPrivilegeEscalation": false })
}

/// How this deployment runs cluster jobs.
#[derive(Clone, Debug)]
pub struct Settings {
    pub git_image: String,
    pub default_image: String,
    pub service_account: Option<String>,
    /// How long a finished Job lingers if conveyor never cleans it up.
    pub ttl_seconds: i32,
}

impl Settings {
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            git_image: quench_core::env::get_or("CONVEYOR_K8S_GIT_IMAGE", DEFAULT_GIT_IMAGE),
            default_image: quench_core::env::get_or("CONVEYOR_K8S_DEFAULT_IMAGE", DEFAULT_IMAGE),
            service_account: {
                let name = quench_core::env::get_or("CONVEYOR_K8S_SERVICE_ACCOUNT", "");
                (!name.trim().is_empty()).then(|| name.trim().to_string())
            },
            ttl_seconds: quench_core::env::get_or("CONVEYOR_K8S_TTL_SECONDS", "3600")
                .parse()
                .unwrap_or(3600),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            git_image: DEFAULT_GIT_IMAGE.to_string(),
            default_image: DEFAULT_IMAGE.to_string(),
            service_account: None,
            ttl_seconds: 3600,
        }
    }
}
