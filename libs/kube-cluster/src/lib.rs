//! A Kubernetes cluster, spoken to directly: apply, diff, delete, list, scale, restart and roll back objects, and wait for a
//! rollout or a Job - what `riveter` and Gantry's runner used to ask the `kubectl` program to do.
//!
//! Built on [`quench_k8s`]: a kubeconfig (or the pod's service account) is all it needs, and no program but the caller's own
//! has to be installed. [`cluster::Cluster`] is the connection; the rest of the crate is the judgement of a rollout or a Job from
//! the object alone, and a unified diff.

pub mod cluster;
pub mod diff;
