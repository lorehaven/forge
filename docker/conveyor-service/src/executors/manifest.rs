//! Turning a job into cluster objects. Kept client-free so it's unit-testable without a cluster.
//! One `batch/v1` Job per conveyor job; an init container fetches the commit into an `emptyDir`.

use crate::executors::engine::{JobCredential, JobSpec};
use crate::workspace::checkout::basic_auth_header;
use k8s_openapi::api::batch::v1::{Job, JobSpec as K8sJobSpec};
use k8s_openapi::api::core::v1::{
    Container, EmptyDirVolumeSource, EnvFromSource, EnvVar, PodSpec, PodTemplateSpec, Secret,
    SecretEnvSource, SecurityContext, Volume, VolumeMount,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use std::collections::BTreeMap;

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

/// Everything a job needs in the cluster.
pub struct Manifest {
    pub name: String,
    pub job: Job,
    /// Present only when the job was given secrets; created before the job, deleted with it.
    pub secret: Option<Secret>,
}

/// DNS-1123: lowercase alphanumerics/dashes, ≤63 chars - doesn't assume job ids are uuids.
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
pub fn script(commands: &[Vec<String>]) -> String {
    let mut script = String::new();

    for (ordinal, argv) in commands.iter().enumerate() {
        let rendered = argv
            .iter()
            .map(|argument| shell_quote(argument))
            .collect::<Vec<_>>()
            .join(" ");

        script.push_str(&format!("printf '%s%d\\n' '{STEP_MARKER}' {ordinal} >&2\n"));
        script.push_str(&format!("{rendered}\n"));
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
pub fn build(
    spec: &JobSpec,
    commands: &[Vec<String>],
    namespace: &str,
    settings: &Settings,
) -> Manifest {
    let name = object_name(&spec.id);
    let labels = labels(spec, &name);

    // Not inline in the pod spec, which anyone who can read pods can read.
    let secret = (!spec.env.is_empty()).then(|| Secret {
        metadata: ObjectMeta {
            name: Some(name.clone()),
            namespace: Some(namespace.to_string()),
            labels: Some(labels.clone()),
            ..ObjectMeta::default()
        },
        string_data: Some(spec.env.clone().into_iter().collect()),
        ..Secret::default()
    });

    let volume_mount = VolumeMount {
        name: WORKSPACE_VOLUME.to_string(),
        mount_path: WORKSPACE_PATH.to_string(),
        ..VolumeMount::default()
    };

    let init_containers = spec.source.as_ref().map(|source| {
        vec![Container {
            name: "checkout".to_string(),
            image: Some(settings.git_image.clone()),
            command: Some(vec!["sh".to_string(), "-c".to_string()]),
            args: Some(vec![checkout_script(
                &source.clone_url,
                &source.git_ref,
                &source.sha,
            )]),
            working_dir: Some(WORKSPACE_PATH.to_string()),
            volume_mounts: Some(vec![volume_mount.clone()]),
            // Only the checkout container sees this; the step container never gets conveyor's own git credential.
            env: credential_env_vars(source.credential.as_ref()),
            security_context: Some(hardened()),
            ..Container::default()
        }]
    });

    let work = Container {
        name: "steps".to_string(),
        image: Some(
            spec.image
                .clone()
                .unwrap_or_else(|| settings.default_image.clone()),
        ),
        command: Some(vec!["sh".to_string(), "-c".to_string()]),
        args: Some(vec![script(commands)]),
        working_dir: Some(WORKSPACE_PATH.to_string()),
        volume_mounts: Some(vec![volume_mount]),
        env_from: secret.as_ref().map(|_| {
            vec![EnvFromSource {
                secret_ref: Some(SecretEnvSource {
                    name: name.clone(),
                    optional: Some(false),
                }),
                ..EnvFromSource::default()
            }]
        }),
        security_context: Some(hardened()),
        ..Container::default()
    };

    let job = Job {
        metadata: ObjectMeta {
            name: Some(name.clone()),
            namespace: Some(namespace.to_string()),
            labels: Some(labels.clone()),
            ..ObjectMeta::default()
        },
        spec: Some(K8sJobSpec {
            // Conveyor owns retries via the queue; a silent second attempt would double-count as one run.
            backoff_limit: Some(0),
            active_deadline_seconds: Some(spec.timeout.as_secs().max(1) as i64),
            // Backstop for a conveyor that died before `forget` could delete this.
            ttl_seconds_after_finished: Some(settings.ttl_seconds),
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: Some(labels),
                    ..ObjectMeta::default()
                }),
                spec: Some(PodSpec {
                    restart_policy: Some("Never".to_string()),
                    init_containers,
                    containers: vec![work],
                    volumes: Some(vec![Volume {
                        name: WORKSPACE_VOLUME.to_string(),
                        empty_dir: Some(EmptyDirVolumeSource::default()),
                        ..Volume::default()
                    }]),
                    service_account_name: settings.service_account.clone(),
                    ..PodSpec::default()
                }),
            },
            ..K8sJobSpec::default()
        }),
        ..Job::default()
    };

    Manifest { name, job, secret }
}

/// Hands the checkout container its credential via `GIT_CONFIG_*`, not embedded in `checkout_script`'s argv text.
fn credential_env_vars(credential: Option<&JobCredential>) -> Option<Vec<EnvVar>> {
    let credential = credential?;
    let header = basic_auth_header(&credential.username, &credential.token);

    Some(vec![
        EnvVar {
            name: "GIT_CONFIG_COUNT".to_string(),
            value: Some("1".to_string()),
            ..EnvVar::default()
        },
        EnvVar {
            name: "GIT_CONFIG_KEY_0".to_string(),
            value: Some("http.extraheader".to_string()),
            ..EnvVar::default()
        },
        EnvVar {
            name: "GIT_CONFIG_VALUE_0".to_string(),
            value: Some(header),
            ..EnvVar::default()
        },
    ])
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
fn hardened() -> SecurityContext {
    SecurityContext {
        allow_privilege_escalation: Some(false),
        ..SecurityContext::default()
    }
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
    pub fn from_env() -> Self {
        Self {
            git_image: envmnt::get_or("CONVEYOR_K8S_GIT_IMAGE", DEFAULT_GIT_IMAGE),
            default_image: envmnt::get_or("CONVEYOR_K8S_DEFAULT_IMAGE", DEFAULT_IMAGE),
            service_account: {
                let name = envmnt::get_or("CONVEYOR_K8S_SERVICE_ACCOUNT", "");
                (!name.trim().is_empty()).then(|| name.trim().to_string())
            },
            ttl_seconds: envmnt::get_or("CONVEYOR_K8S_TTL_SECONDS", "3600")
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
