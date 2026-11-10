//! The cluster, spoken to directly.
//!
//! What riveter used to ask `kubectl` to do: apply a manifest, diff it against the cluster, delete it, wait for a rollout or a
//! Job, list what an environment owns, read the CRDs. A manifest is read object by object and each object is sent through
//! [`quench_k8s`], so no program but riveter is needed to touch a cluster.
//!
//! Objects are applied **server side** (`PATCH` with `application/apply-patch+yaml`, field manager `riveter`, conflicts forced).
//! The server keeps track of which fields riveter set, so a field dropped from an overlay is removed on the next apply, which
//! is what `kubectl apply`'s last-applied annotation did.

use crate::diff;
use quench_core::{Result, bail, ensure, err};
use quench_k8s::{
    Api, ApiResource, Client, Config, DeleteParams, Discovery, Error as KubeError, ListParams,
    Patch, PatchParams,
};
use quench_serde::{Value, json};
use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::future::Future;
use std::rc::Rc;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

/// Who the cluster records as the owner of what riveter applied.
pub const FIELD_MANAGER: &str = "riveter";

/// How often a wait looks at the cluster again.
const POLL: Duration = Duration::from_secs(1);

// ── Manifests ───────────────────────────────────────────────────────────────────────────────────────────────────────

/// The objects of a manifest file, in file order.
///
/// # Errors
/// If the file cannot be read or is not YAML.
pub fn read_manifest(path: &str) -> Result<Vec<Value>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| err!("could not read the manifest {path}: {e}"))?;
    documents(&text)
}

/// The objects in some YAML: documents that hold nothing are left out and a `kind: List` is opened up, as `kubectl -f` does.
///
/// # Errors
/// If the text is not YAML.
pub fn documents(text: &str) -> Result<Vec<Value>> {
    let parsed: Vec<Value> = quench_serde::yaml::from_str_all(text)
        .map_err(|e| err!("the manifest is not valid YAML: {e}"))?;
    let mut out = Vec::new();
    for doc in parsed {
        let items = doc
            .get("items")
            .and_then(Value::as_array)
            .filter(|_| doc.get("kind").and_then(Value::as_str) == Some("List"));
        match items {
            Some(items) => out.extend(items.iter().cloned()),
            None if doc.as_object().is_some_and(|o| o.iter().len() > 0) => out.push(doc),
            None => {}
        }
    }
    Ok(out)
}

/// `apiVersion`, `kind` and `metadata.name` of an object, which every object has to have.
fn identity(doc: &Value) -> Result<(&str, &str, &str)> {
    let text = |path: &[&str]| {
        let mut at = doc;
        for key in path {
            at = at.get(*key)?;
        }
        at.as_str().filter(|s| !s.is_empty())
    };
    match (
        text(&["apiVersion"]),
        text(&["kind"]),
        text(&["metadata", "name"]),
    ) {
        (Some(api_version), Some(kind), Some(name)) => Ok((api_version, kind, name)),
        _ => bail!(
            "an object of the manifest has no apiVersion, kind or metadata.name: {}",
            doc.to_string().chars().take(120).collect::<String>()
        ),
    }
}

/// The group of an `apiVersion` (`apps` of `apps/v1`, empty for `v1`).
fn group_of(api_version: &str) -> &str {
    api_version.split_once('/').map_or("", |(group, _)| group)
}

/// How kubectl names an object when it reports on it: `deployment.apps/web`, `service/web`.
#[must_use]
pub fn label(doc: &Value) -> String {
    let (api_version, kind, name) = identity(doc).unwrap_or(("", "object", "?"));
    let kind = kind.to_lowercase();
    match group_of(api_version) {
        "" => format!("{kind}/{name}"),
        group => format!("{kind}.{group}/{name}"),
    }
}

/// What applying an object did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    Created,
    Configured,
    Unchanged,
}

impl std::fmt::Display for Verb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Created => "created",
            Self::Configured => "configured",
            Self::Unchanged => "unchanged",
        })
    }
}

/// A dry run that never reaches the cluster: what applying the objects would send, checked only as far as the objects
/// themselves go. Prints a line per object and returns how many.
///
/// # Errors
/// An object with no `apiVersion`, `kind` or `metadata.name`.
pub fn dry_run(docs: &[Value]) -> Result<usize> {
    for doc in docs {
        identity(doc)?;
        println!("{} configured (dry run)", label(doc));
    }
    Ok(docs.len())
}

// ── The connection ──────────────────────────────────────────────────────────────────────────────────────────────────

/// A connection to one cluster, driven from synchronous code.
pub struct Cluster {
    runtime: Runtime,
    client: Client,
    /// The resources of a group and version, as that group's discovery document lists them.
    versions: RefCell<HashMap<String, Rc<Vec<ApiResource>>>>,
    /// Everything the cluster serves, read when something has to be found by its name alone.
    everything: OnceCell<Discovery>,
}

impl std::fmt::Debug for Cluster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cluster").finish_non_exhaustive()
    }
}

fn kube(error: &KubeError) -> quench_core::error::Error {
    match error {
        KubeError::Api(status) => err!("{}: {}", status.reason, status.message),
        other => err!("{other}"),
    }
}

impl Cluster {
    /// Connects the way `kubectl` would: the kubeconfig `KUBECONFIG` names, or `~/.kube/config`, with `context` or else its
    /// current one; inside a pod, the pod's service account.
    ///
    /// # Errors
    /// No usable configuration, or one that needs a credential plugin.
    pub fn connect(context: Option<&str>) -> Result<Self> {
        let config = Config::infer_with_context(context)
            .map_err(|e| err!("could not find the cluster: {e}"))?;
        let client =
            Client::new(config).map_err(|e| err!("could not set up the connection: {e}"))?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| err!("could not start the runtime: {e}"))?;
        Ok(Self {
            runtime,
            client,
            versions: RefCell::default(),
            everything: OnceCell::new(),
        })
    }

    fn run<T>(&self, work: impl Future<Output = T>) -> T {
        self.runtime.block_on(work)
    }

    fn api(&self, resource: &ApiResource, namespace: Option<&str>) -> Api {
        match (resource.namespaced, namespace) {
            (true, Some(namespace)) => self.client.namespaced(resource, namespace),
            _ => self.client.api(resource),
        }
    }

    // ── What an object is, to the cluster ───────────────────────────────────────────────────────────────────────────

    /// The resources a group and version serve; `refresh` reads them again (a CRD applied a moment ago adds one).
    fn resources_of(&self, api_version: &str, refresh: bool) -> Result<Rc<Vec<ApiResource>>> {
        if !refresh && let Some(found) = self.versions.borrow().get(api_version) {
            return Ok(Rc::clone(found));
        }
        let (group, version) = api_version.split_once('/').unwrap_or(("", api_version));
        let path = if group.is_empty() {
            format!("/api/{version}")
        } else {
            format!("/apis/{group}/{version}")
        };
        let list = self
            .run(self.client.get_json(&path))
            .map_err(|e| err!("the cluster does not serve {api_version}: {}", kube(&e)))?;
        let found: Vec<ApiResource> = list
            .get("resources")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|r| {
                let text = |key: &str| r.get(key).and_then(Value::as_str);
                let (plural, kind) = (text("name")?, text("kind")?);
                (!plural.contains('/')).then(|| {
                    ApiResource::new(
                        group,
                        version,
                        kind,
                        plural,
                        r.get("namespaced").and_then(Value::as_bool) == Some(true),
                    )
                })
            })
            .collect();
        let found = Rc::new(found);
        self.versions
            .borrow_mut()
            .insert(api_version.to_string(), Rc::clone(&found));
        Ok(found)
    }

    /// The resource an object is an instance of.
    fn resource_of(&self, api_version: &str, kind: &str) -> Result<ApiResource> {
        for refresh in [false, true] {
            if let Some(found) = self
                .resources_of(api_version, refresh)?
                .iter()
                .find(|r| r.kind == kind)
            {
                return Ok(found.clone());
            }
        }
        bail!("the cluster has no resource of kind {kind} in {api_version}")
    }

    /// Where an object lives: its resource, its name and, for a namespaced one, its namespace (the manifest's own, else the
    /// kubeconfig context's).
    fn locate(&self, doc: &Value) -> Result<(ApiResource, String, Option<String>)> {
        let (api_version, kind, name) = identity(doc)?;
        let resource = self.resource_of(api_version, kind)?;
        let namespace = resource.namespaced.then(|| {
            doc.get("metadata")
                .and_then(|m| m.get("namespace"))
                .and_then(Value::as_str)
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| self.client.default_namespace())
                .to_string()
        });
        Ok((resource, name.to_string(), namespace))
    }

    // ── Apply ────────────────────────────────────────────────────────────────────────────────────────────────────

    fn apply_one(&self, doc: &Value) -> Result<Verb> {
        let (resource, name, namespace) = self.locate(doc)?;
        let api = self.api(&resource, namespace.as_deref());
        let before = self.run(api.get_opt(&name)).map_err(|e| kube(&e))?;
        let params = PatchParams {
            dry_run: false,
            field_manager: Some(FIELD_MANAGER.to_string()),
            force: true,
        };
        let after = self
            .run(api.patch(
                &name,
                &Patch::Apply(with_namespace(doc, namespace.as_deref())),
                &params,
            ))
            .map_err(|e| kube(&e))?;
        Ok(match before {
            None => Verb::Created,
            Some(before) if resource_version(&before) == resource_version(&after) => {
                Verb::Unchanged
            }
            Some(_) => Verb::Configured,
        })
    }

    /// Applies each object, one after the other, and says what became of it. An object that fails does not stop the ones after
    /// it, as with `kubectl apply`; the failures are reported together at the end.
    ///
    /// # Errors
    /// If any object failed.
    pub fn apply(&self, docs: &[Value]) -> Result<usize> {
        self.apply_all(docs, true)
    }

    /// As [`apply`](Self::apply), saying nothing about what it did (for a Secret, whose name and hash are nobody's business).
    ///
    /// # Errors
    /// If any object failed.
    pub fn apply_quietly(&self, docs: &[Value]) -> Result<usize> {
        self.apply_all(docs, false)
    }

    fn apply_all(&self, docs: &[Value], report: bool) -> Result<usize> {
        let mut failed = Vec::new();
        for doc in docs {
            match self.apply_one(doc) {
                Ok(verb) if report => println!("{} {verb}", label(doc)),
                Ok(_) => {}
                Err(error) => {
                    // Said as it happens when the apply is reported; a quiet one has only the error to say it in.
                    if report {
                        eprintln!("error: {}: {error}", label(doc));
                        failed.push(label(doc));
                    } else {
                        failed.push(format!("{} ({error})", label(doc)));
                    }
                }
            }
        }
        ensure!(
            failed.is_empty(),
            "could not apply {} of {} object(s): {}",
            failed.len(),
            docs.len(),
            failed.join(", ")
        );
        Ok(docs.len())
    }

    // ── Diff ─────────────────────────────────────────────────────────────────────────────────────────────────────

    /// Prints what applying `docs` would change, as a unified diff of each object's YAML, and returns whether anything
    /// would. The merged object is the server's own answer to a dry-run apply, so defaults it fills in and fields other
    /// managers own are accounted for the way an apply would account for them.
    ///
    /// # Errors
    /// If the cluster could not be asked about an object.
    pub fn diff(&self, docs: &[Value]) -> Result<bool> {
        let mut differs = false;
        for doc in docs {
            let (resource, name, namespace) = self.locate(doc)?;
            let api = self.api(&resource, namespace.as_deref());
            let live = self.run(api.get_opt(&name)).map_err(|e| kube(&e))?;
            let params = PatchParams {
                dry_run: true,
                field_manager: Some(FIELD_MANAGER.to_string()),
                force: true,
            };
            let merged = self
                .run(api.patch(
                    &name,
                    &Patch::Apply(with_namespace(doc, namespace.as_deref())),
                    &params,
                ))
                .map_err(|e| err!("{}: {}", label(doc), kube(&e)))?;

            let old = live.as_ref().map(comparable).unwrap_or_default();
            let new = comparable(&merged);
            let id = identifier(&resource, &name, namespace.as_deref());
            if let Some(text) =
                diff::unified(&old, &new, &format!("live/{id}"), &format!("merged/{id}"))
            {
                differs = true;
                println!("diff -u -N live/{id} merged/{id}");
                print!("{text}");
            }
        }
        Ok(differs)
    }

    // ── Delete ───────────────────────────────────────────────────────────────────────────────────────────────────

    /// Deletes each object of `docs`. One that is not there is an error, as with `kubectl delete -f`, and the rest are still
    /// deleted.
    ///
    /// # Errors
    /// If any object could not be deleted.
    pub fn delete(&self, docs: &[Value]) -> Result<usize> {
        let mut failed = Vec::new();
        for doc in docs {
            let outcome = self.locate(doc).and_then(|(resource, name, namespace)| {
                self.run(
                    self.api(&resource, namespace.as_deref())
                        .delete(&name, &DeleteParams::background()),
                )
                .map(|_| name)
                .map_err(|e| kube(&e))
            });
            match outcome {
                Ok(name) => println!(
                    "{} \"{name}\" deleted",
                    label(doc).split('/').next().unwrap_or("object")
                ),
                Err(error) => {
                    eprintln!("error: {}: {error}", label(doc));
                    failed.push(label(doc));
                }
            }
        }
        ensure!(
            failed.is_empty(),
            "could not delete {} of {} object(s): {}",
            failed.len(),
            docs.len(),
            failed.join(", ")
        );
        Ok(docs.len())
    }

    /// Deletes one named object, found by the lowercase kind riveter lists (`deployment`).
    ///
    /// # Errors
    /// An unknown kind, or the cluster refusing.
    pub fn delete_named(&self, kind: &str, name: &str, namespace: Option<&str>) -> Result<()> {
        let resource = self
            .kind_named(kind)
            .ok_or_else(|| err!("the cluster does not serve {kind}"))?;
        let namespace = namespace.unwrap_or_else(|| self.client.default_namespace());
        self.run(
            self.api(&resource, Some(namespace))
                .delete(name, &DeleteParams::background()),
        )
        .map(|_| ())
        .map_err(|e| kube(&e))
    }

    // ── Looking ──────────────────────────────────────────────────────────────────────────────────────────────────

    /// Whether the namespace exists; `None` when the cluster could not be asked, which is not the same as "no".
    #[must_use]
    pub fn namespace_exists(&self, name: &str) -> Option<bool> {
        self.run(self.client.api(&ApiResource::namespace()).get_opt(name))
            .ok()
            .map(|found| found.is_some())
    }

    /// The resource a lowercase kind or plural names (`deployment`, `ingresses`), in the groups the cluster serves.
    fn kind_named(&self, name: &str) -> Option<ApiResource> {
        let everything = if let Some(found) = self.everything.get() {
            found
        } else {
            let found = self.run(self.client.discover()).ok()?;
            self.everything.get_or_init(|| found)
        };
        let name = name.to_lowercase();
        everything
            .groups()
            .iter()
            .flat_map(|group| group.recommended_resources().iter())
            .map(|(resource, _)| resource)
            .find(|r| r.kind.to_lowercase() == name || r.plural == name)
            .cloned()
    }

    /// What of these kinds carries `selector` (`a=b,c=d`) in `namespace`. A kind the cluster does not serve (a CRD that is not
    /// installed) is left out rather than failing the lot.
    ///
    /// # Errors
    /// If the cluster refuses to list a kind it does serve.
    pub fn list_labelled(
        &self,
        kinds: &[&str],
        selector: &str,
        namespace: Option<&str>,
    ) -> Result<Vec<Listed>> {
        let mut found = Vec::new();
        for kind in kinds {
            let Some(resource) = self.kind_named(kind) else {
                continue;
            };
            let namespace = namespace.unwrap_or_else(|| self.client.default_namespace());
            let list = self
                .run(
                    self.api(&resource, Some(namespace))
                        .list(&ListParams::default().labels(selector)),
                )
                .map_err(|e| err!("could not list {}: {}", resource.plural, kube(&e)))?;
            for item in &list.items {
                let Some(name) = item
                    .get("metadata")
                    .and_then(|m| m.get("name"))
                    .and_then(Value::as_str)
                else {
                    continue;
                };
                found.push(Listed {
                    kind: resource.kind.clone(),
                    name: name.to_string(),
                    owner_kinds: item
                        .get("metadata")
                        .and_then(|m| m.get("ownerReferences"))
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|o| o.get("kind").and_then(Value::as_str))
                        .map(str::to_string)
                        .collect(),
                    selects: item
                        .get("spec")
                        .and_then(|s| s.get("selector"))
                        .is_some_and(|s| !s.is_null()),
                });
            }
        }
        Ok(found)
    }

    /// The CRDs named (`certificates.cert-manager.io`), or every one if `names` is empty. A name the cluster does not have is
    /// left out.
    ///
    /// # Errors
    /// If the cluster refuses to list or read them.
    pub fn crds(&self, names: &[String]) -> Result<Vec<Value>> {
        let api = self.client.api(&ApiResource::new(
            "apiextensions.k8s.io",
            "v1",
            "CustomResourceDefinition",
            "customresourcedefinitions",
            false,
        ));
        if names.is_empty() {
            return self
                .run(api.list(&ListParams::default()))
                .map(|list| list.items)
                .map_err(|e| err!("could not list the CRDs: {}", kube(&e)));
        }
        let mut found = Vec::new();
        for name in names {
            match self.run(api.get_opt(name)) {
                Ok(Some(crd)) => found.push(crd),
                Ok(None) => {}
                Err(e) => bail!("could not read the CRD {name}: {}", kube(&e)),
            }
        }
        Ok(found)
    }

    /// The namespace an object without one goes to: the kubeconfig context's, or the pod's own.
    #[must_use]
    pub fn default_namespace(&self) -> &str {
        self.client.default_namespace()
    }

    // ── Changing what runs ───────────────────────────────────────────────────────────────────────────────────────

    /// The resource a `kind[.group]` names (`deployment`, `clusterrole.rbac.authorization.k8s.io`), as `kubectl` takes it.
    fn resource_named(&self, spec: &str) -> Option<ApiResource> {
        let Some((kind, group)) = spec.split_once('.') else {
            return self.kind_named(spec);
        };
        self.kind_named(kind)
            .filter(|r| r.group == group)
            .or_else(|| self.kind_named(spec))
    }

    /// Sets a workload's replica count.
    ///
    /// # Errors
    /// An unknown kind, or the cluster refusing.
    pub fn scale(&self, kind: &str, name: &str, namespace: &str, replicas: u32) -> Result<()> {
        let resource = self
            .resource_named(kind)
            .ok_or_else(|| err!("the cluster does not serve {kind}"))?;
        let patch = json!({"spec": {"replicas": replicas}});
        self.run(self.api(&resource, Some(namespace)).patch(
            name,
            &Patch::Merge(patch),
            &PatchParams::default(),
        ))
        .map(|_| ())
        .map_err(|e| kube(&e))
    }

    /// Restarts a workload's pods the way `kubectl rollout restart` does: a changed annotation on the pod template, which the
    /// controller answers with a rolling update.
    ///
    /// # Errors
    /// An unknown kind, or the cluster refusing.
    pub fn restart(&self, kind: &str, name: &str, namespace: &str) -> Result<()> {
        let resource = self
            .resource_named(kind)
            .ok_or_else(|| err!("the cluster does not serve {kind}"))?;
        let now = quench_core::time::Time::now().to_rfc3339();
        let patch = json!({"spec": {"template": {"metadata": {"annotations": {
            "kubectl.kubernetes.io/restartedAt": now
        }}}}});
        self.run(self.api(&resource, Some(namespace)).patch(
            name,
            &Patch::Merge(patch),
            &PatchParams::default(),
        ))
        .map(|_| ())
        .map_err(|e| kube(&e))
    }

    /// Puts a Deployment back to its previous revision: the pod template of the newest `ReplicaSet` older than the current one.
    ///
    /// # Errors
    /// Not a Deployment, no earlier revision, or the cluster refusing.
    pub fn undo(&self, kind: &str, name: &str, namespace: &str) -> Result<String> {
        ensure!(
            kind.eq_ignore_ascii_case("deployment") || kind.eq_ignore_ascii_case("deployments"),
            "rolling back is supported for Deployments, not {kind}"
        );
        let deployments = self.api(&ApiResource::deployment(), Some(namespace));
        let deployment = self
            .run(deployments.get(name))
            .map_err(|e| err!("could not read deployment/{name}: {}", kube(&e)))?;
        let revision_of = |object: &Value| {
            object
                .get("metadata")
                .and_then(|m| m.get("annotations"))
                .and_then(|a| a.get("deployment.kubernetes.io/revision"))
                .and_then(Value::as_str)
                .and_then(|r| r.parse::<i64>().ok())
        };
        let current = revision_of(&deployment).unwrap_or(0);
        let uid = text(&deployment, &["metadata", "uid"]).to_string();
        let sets = self
            .run(
                self.api(
                    &ApiResource::new("apps", "v1", "ReplicaSet", "replicasets", true),
                    Some(namespace),
                )
                .list(&ListParams::default()),
            )
            .map_err(|e| err!("could not list the ReplicaSets: {}", kube(&e)))?;
        let previous = sets
            .items
            .iter()
            .filter(|set| {
                set.get("metadata")
                    .and_then(|m| m.get("ownerReferences"))
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .any(|owner| {
                        text(owner, &["kind"]) == "Deployment"
                            && (text(owner, &["uid"]) == uid || text(owner, &["name"]) == name)
                    })
            })
            .filter_map(|set| revision_of(set).map(|revision| (revision, set)))
            .filter(|(revision, _)| *revision < current)
            .max_by_key(|(revision, _)| *revision);
        let Some((revision, set)) = previous else {
            bail!("deployment/{name} has no earlier revision to roll back to");
        };
        let mut template = set
            .get("spec")
            .and_then(|s| s.get("template"))
            .cloned()
            .ok_or_else(|| err!("the ReplicaSet of revision {revision} has no pod template"))?;
        // The hash label belongs to the ReplicaSet, not to what the Deployment asks for.
        if let Some(labels) = template
            .get_mut("metadata")
            .and_then(|m| m.get_mut("labels"))
            .and_then(Value::as_object_mut)
        {
            labels.remove("pod-template-hash");
        }
        let patch = json!([{"op": "replace", "path": "/spec/template", "value": template}]);
        self.run(deployments.patch(name, &Patch::Json(patch), &PatchParams::default()))
            .map_err(|e| kube(&e))?;
        Ok(format!(
            "deployment/{name} rolled back to revision {revision}"
        ))
    }

    /// Deletes one object named as `kind[.group]/name` and waits until it is gone, up to `timeout`. A missing one is an error
    /// unless `ignore_missing`.
    ///
    /// # Errors
    /// An unknown kind, the cluster refusing, or the object still there at the end of `timeout`.
    pub fn delete_resource(
        &self,
        spec: &str,
        namespace: Option<&str>,
        ignore_missing: bool,
        timeout: Duration,
    ) -> Result<bool> {
        let (kind, name) = spec
            .split_once('/')
            .ok_or_else(|| err!("`{spec}` is not kind/name"))?;
        let resource = self
            .resource_named(kind)
            .ok_or_else(|| err!("the cluster does not serve {kind}"))?;
        let namespace = namespace.unwrap_or_else(|| self.client.default_namespace());
        let api = self.api(&resource, Some(namespace));
        match self.run(api.delete(name, &DeleteParams::background())) {
            Ok(_) => {}
            Err(KubeError::Api(status)) if status.code == 404 && ignore_missing => {
                return Ok(false);
            }
            Err(e) => return Err(kube(&e)),
        }
        let deadline = Instant::now() + timeout;
        loop {
            match self.run(api.get_opt(name)) {
                Ok(None) => return Ok(true),
                Ok(Some(_)) => {}
                Err(e) => return Err(kube(&e)),
            }
            ensure!(
                Instant::now() < deadline,
                "timed out waiting for {spec} to be deleted"
            );
            std::thread::sleep(POLL.min(Duration::from_millis(500)));
        }
    }

    /// The objects of some kinds (`pods`, `deployment`) in a namespace, or in all when `namespace` is `None`, that match a label
    /// selector (empty for all).
    ///
    /// # Errors
    /// An unknown kind, or the cluster refusing.
    pub fn objects(
        &self,
        kinds: &[&str],
        selector: &str,
        namespace: Option<&str>,
    ) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        for kind in kinds {
            let resource = self
                .resource_named(kind)
                .ok_or_else(|| err!("the cluster does not serve {kind}"))?;
            let mut params = ListParams::default();
            if !selector.is_empty() {
                params = params.labels(selector);
            }
            let list = self
                .run(self.api(&resource, namespace).list(&params))
                .map_err(|e| err!("could not list {}: {}", resource.plural, kube(&e)))?;
            for mut item in list.items {
                // A list leaves the kind off its items; a caller that reads them as objects wants it.
                if let Some(map) = item.as_object_mut()
                    && !map.contains_key("kind")
                {
                    map.insert("kind", Value::String(resource.kind.clone()));
                }
                out.push(item);
            }
        }
        Ok(out)
    }

    /// Deletes the objects of one kind that match a label selector, without waiting; returns how many.
    ///
    /// # Errors
    /// An unknown kind, or the cluster refusing.
    pub fn delete_matching(
        &self,
        kind: &str,
        selector: &str,
        namespace: Option<&str>,
    ) -> Result<usize> {
        let resource = self
            .resource_named(kind)
            .ok_or_else(|| err!("the cluster does not serve {kind}"))?;
        let found = self.objects(&[kind], selector, namespace)?;
        for object in &found {
            let name = text(object, &["metadata", "name"]).to_string();
            let each = text(object, &["metadata", "namespace"]);
            let each = if each.is_empty() {
                namespace
            } else {
                Some(each)
            };
            self.run(
                self.api(&resource, each)
                    .delete(&name, &DeleteParams::background()),
            )
            .map_err(|e| err!("could not delete {kind}/{name}: {}", kube(&e)))?;
        }
        Ok(found.len())
    }

    // ── Waiting ──────────────────────────────────────────────────────────────────────────────────────────────────

    /// Waits for a `Deployment`, `StatefulSet` or `DaemonSet` to finish rolling out, saying how far it is each time that
    /// changes, as `kubectl rollout status` does.
    ///
    /// # Errors
    /// If it fails, or is not done within `timeout`; the message says where it had got to.
    pub fn rollout_status(
        &self,
        kind: &str,
        name: &str,
        namespace: Option<&str>,
        timeout: Duration,
    ) -> Result<()> {
        let resource = match kind.to_lowercase().as_str() {
            "deployment" => ApiResource::deployment(),
            "statefulset" => ApiResource::stateful_set(),
            "daemonset" => ApiResource::daemon_set(),
            other => bail!("`{other}` is not something that rolls out"),
        };
        let namespace = namespace.unwrap_or_else(|| self.client.default_namespace());
        let api = self.api(&resource, Some(namespace));
        let deadline = Instant::now() + timeout;
        let mut said = String::new();
        loop {
            let object = self
                .run(api.get(name))
                .map_err(|e| err!("could not read {kind}/{name}: {}", kube(&e)))?;
            match rollout_progress(&resource.kind, &object) {
                Progress::Done(message) => {
                    println!("{message}");
                    return Ok(());
                }
                Progress::Failed(why) => bail!("{why}"),
                Progress::Waiting(message) => {
                    if message != said {
                        println!("{message}");
                        said = message;
                    }
                }
            }
            ensure!(
                Instant::now() < deadline,
                "timed out waiting for {kind}/{name}: {said}"
            );
            std::thread::sleep(POLL.min(deadline.saturating_duration_since(Instant::now())));
        }
    }

    /// Where a Job is, as its conditions say.
    ///
    /// # Errors
    /// If the Job cannot be read.
    pub fn job_state(&self, name: &str, namespace: Option<&str>) -> Result<JobState> {
        let namespace = namespace.unwrap_or_else(|| self.client.default_namespace());
        let job = self
            .run(
                self.client
                    .namespaced(&ApiResource::job(), namespace)
                    .get(name),
            )
            .map_err(|e| kube(&e))?;
        Ok(job_state_of(&job))
    }
}

// ── What a listing found ────────────────────────────────────────────────────────────────────────────────────────────

/// One live object a listing found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    /// The kind as the cluster spells it (`Deployment`).
    pub kind: String,
    pub name: String,
    /// The kinds of the objects that own it.
    pub owner_kinds: Vec<String>,
    /// Whether it has a `spec.selector` (a Service that picks pods).
    pub selects: bool,
}

// ── Rollouts and Jobs, read from the object ─────────────────────────────────────────────────────────────────────────

/// How far a rollout is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    Done(String),
    Waiting(String),
    Failed(String),
}

fn number(object: &Value, path: &[&str]) -> i64 {
    let mut at = object;
    for key in path {
        match at.get(*key) {
            Some(next) => at = next,
            None => return 0,
        }
    }
    at.as_i64().unwrap_or(0)
}

fn text<'a>(object: &'a Value, path: &[&str]) -> &'a str {
    let mut at = object;
    for key in path {
        match at.get(*key) {
            Some(next) => at = next,
            None => return "",
        }
    }
    at.as_str().unwrap_or("")
}

/// Whether a workload rolls out by replacing its pods itself; anything else (`OnDelete`) has no rollout to wait for.
fn rolls_itself(object: &Value) -> bool {
    matches!(
        text(object, &["spec", "updateStrategy", "type"]),
        "" | "RollingUpdate"
    )
}

/// What `kubectl rollout status` would say of a workload now: the same checks, in the same order, with the same words.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "one arm per kind of workload, each a list of checks in the order kubectl makes them"
)]
pub fn rollout_progress(kind: &str, object: &Value) -> Progress {
    let name = text(object, &["metadata", "name"]);
    let generation = number(object, &["metadata", "generation"]);
    let observed = number(object, &["status", "observedGeneration"]);
    match kind {
        "Deployment" => {
            if generation > observed {
                return Progress::Waiting(
                    "Waiting for deployment spec update to be observed...".into(),
                );
            }
            let overdue = object
                .get("status")
                .and_then(|s| s.get("conditions"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|c| {
                    text(c, &["type"]) == "Progressing"
                        && text(c, &["reason"]) == "ProgressDeadlineExceeded"
                });
            if overdue {
                return Progress::Failed(format!(
                    "deployment \"{name}\" exceeded its progress deadline"
                ));
            }
            let wanted = object
                .get("spec")
                .and_then(|s| s.get("replicas"))
                .and_then(Value::as_i64);
            let (replicas, updated, available) = (
                number(object, &["status", "replicas"]),
                number(object, &["status", "updatedReplicas"]),
                number(object, &["status", "availableReplicas"]),
            );
            if let Some(wanted) = wanted.filter(|wanted| updated < *wanted) {
                return Progress::Waiting(format!(
                    "Waiting for deployment \"{name}\" rollout to finish: {updated} out of {wanted} new replicas have been updated..."
                ));
            }
            if replicas > updated {
                return Progress::Waiting(format!(
                    "Waiting for deployment \"{name}\" rollout to finish: {} old replicas are pending termination...",
                    replicas - updated
                ));
            }
            if available < updated {
                return Progress::Waiting(format!(
                    "Waiting for deployment \"{name}\" rollout to finish: {available} of {updated} updated replicas are available..."
                ));
            }
            Progress::Done(format!("deployment \"{name}\" successfully rolled out"))
        }
        "DaemonSet" => {
            if !rolls_itself(object) {
                return Progress::Failed(
                    "rollout status is only available for RollingUpdate strategy type".into(),
                );
            }
            if generation > observed {
                return Progress::Waiting(
                    "Waiting for daemon set spec update to be observed...".into(),
                );
            }
            let desired = number(object, &["status", "desiredNumberScheduled"]);
            let updated = number(object, &["status", "updatedNumberScheduled"]);
            let available = number(object, &["status", "numberAvailable"]);
            if updated < desired {
                return Progress::Waiting(format!(
                    "Waiting for daemon set \"{name}\" rollout to finish: {updated} out of {desired} new pods have been updated..."
                ));
            }
            if available < desired {
                return Progress::Waiting(format!(
                    "Waiting for daemon set \"{name}\" rollout to finish: {available} of {desired} updated pods are available..."
                ));
            }
            Progress::Done(format!("daemon set \"{name}\" successfully rolled out"))
        }
        "StatefulSet" => {
            if !rolls_itself(object) {
                return Progress::Failed(
                    "rollout status is only available for RollingUpdate strategy type".into(),
                );
            }
            if observed == 0 || generation > observed {
                return Progress::Waiting(
                    "Waiting for statefulset spec update to be observed...".into(),
                );
            }
            let wanted = object
                .get("spec")
                .and_then(|s| s.get("replicas"))
                .and_then(Value::as_i64);
            let (ready, updated) = (
                number(object, &["status", "readyReplicas"]),
                number(object, &["status", "updatedReplicas"]),
            );
            if let Some(wanted) = wanted.filter(|wanted| ready < *wanted) {
                return Progress::Waiting(format!(
                    "Waiting for {} pods to be ready...",
                    wanted - ready
                ));
            }
            let partition = object
                .get("spec")
                .and_then(|s| s.get("updateStrategy"))
                .and_then(|u| u.get("rollingUpdate"))
                .and_then(|r| r.get("partition"))
                .and_then(Value::as_i64);
            if let (Some(partition), Some(wanted)) = (partition.filter(|p| *p > 0), wanted)
                && updated < wanted - partition
            {
                return Progress::Waiting(format!(
                    "Waiting for partitioned roll out to finish: {updated} out of {} new pods have been updated...",
                    wanted - partition
                ));
            }
            if partition.is_some_and(|p| p > 0) {
                return Progress::Done(format!(
                    "partitioned roll out complete: {updated} new pods have been updated..."
                ));
            }
            let (update, current) = (
                text(object, &["status", "updateRevision"]),
                text(object, &["status", "currentRevision"]),
            );
            if update != current {
                return Progress::Waiting(format!(
                    "waiting for statefulset rolling update to complete {updated} pods at revision {update}..."
                ));
            }
            Progress::Done(format!(
                "statefulset rolling update complete {updated} pods at revision {update}..."
            ))
        }
        other => Progress::Failed(format!("`{other}` is not something that rolls out")),
    }
}

/// Where a Job is, as far as its conditions say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobState {
    /// Neither completed nor failed yet.
    Running,
    /// Finished successfully.
    Complete,
    /// Gave up, with the reason Kubernetes recorded.
    Failed(String),
}

/// The state a Job's `status.conditions` describe.
#[must_use]
pub fn job_state_of(job: &Value) -> JobState {
    let conditions = job
        .get("status")
        .and_then(|s| s.get("conditions"))
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let holds = |kind: &str| {
        conditions.iter().find(|c| {
            text(c, &["type"]) == kind && text(c, &["status"]).eq_ignore_ascii_case("true")
        })
    };
    if holds("Complete").is_some() {
        JobState::Complete
    } else if let Some(failed) = holds("Failed") {
        JobState::Failed(text(failed, &["message"]).to_string())
    } else {
        JobState::Running
    }
}

// ── Small helpers ───────────────────────────────────────────────────────────────────────────────────────────────────

fn resource_version(object: &Value) -> Option<&str> {
    object
        .get("metadata")
        .and_then(|m| m.get("resourceVersion"))
        .and_then(Value::as_str)
}

/// The object with its namespace written in, for a namespaced resource whose manifest left it to the context.
fn with_namespace(doc: &Value, namespace: Option<&str>) -> Value {
    let mut doc = doc.clone();
    if let (Some(namespace), Some(map)) = (
        namespace,
        doc.get_mut("metadata").and_then(Value::as_object_mut),
    ) && !map.contains_key("namespace")
    {
        map.insert("namespace", Value::String(namespace.to_string()));
    }
    doc
}

/// `apps.v1.Deployment.demo.web`, the name kubectl gives the files of a diff.
fn identifier(resource: &ApiResource, name: &str, namespace: Option<&str>) -> String {
    let group = if resource.group.is_empty() {
        String::new()
    } else {
        format!("{}.", resource.group)
    };
    let namespace = namespace.map(|n| format!("{n}.")).unwrap_or_default();
    format!(
        "{group}{}.{}.{namespace}{name}",
        resource.version, resource.kind
    )
}

/// An object as text for comparing: what the cluster adds to every object as it goes (`managedFields`, the version and
/// generation counters) left out, the keys sorted so that two objects with the same content read the same.
fn comparable(object: &Value) -> String {
    let mut object = object.clone();
    if let Some(metadata) = object.get_mut("metadata").and_then(Value::as_object_mut) {
        for key in ["managedFields", "resourceVersion", "generation"] {
            metadata.remove(key);
        }
    }
    quench_serde::yaml::to_string(&sorted(&object)).unwrap_or_default()
}

fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = quench_serde::Map::new();
            for key in keys {
                if let Some(inner) = map.get(key) {
                    out.insert(key.clone(), sorted(inner));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

// ── The kubeconfig's own say ────────────────────────────────────────────────────────────────────────────────────────

/// The context a command would use when none is named: the `current-context` of the kubeconfig. `None` when there is no
/// kubeconfig or it names none.
#[must_use]
pub fn current_context() -> Option<String> {
    let paths: Vec<std::path::PathBuf> =
        if let Some(value) = std::env::var_os("KUBECONFIG").filter(|v| !v.is_empty()) {
            std::env::split_paths(&value).collect()
        } else {
            let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
            vec![std::path::PathBuf::from(home).join(".kube/config")]
        };
    paths.iter().find_map(|path| {
        let text = std::fs::read_to_string(path).ok()?;
        let config: Value = quench_serde::yaml::from_str(&text).ok()?;
        config
            .get("current-context")
            .and_then(Value::as_str)
            .filter(|c| !c.is_empty())
            .map(str::to_string)
    })
}
