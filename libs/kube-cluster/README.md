# kube-cluster
A Kubernetes cluster spoken to directly, for the tools that used to run `kubectl`.

## What it is
`Cluster::connect(context)` reads the kubeconfig (`KUBECONFIG`, else `~/.kube/config`, or the pod's service account) and gives synchronous operations over [`quench-k8s`](https://crates.io/crates/quench-k8s): server-side `apply`, `diff` against the live objects, `delete`, `list_labelled`, `scale`, `restart`, `undo` (a Deployment's previous revision), `rollout_status` and `job_state`.

## Use cases
- A deploy tool that applies manifests without a `kubectl` binary (`riveter`).
- A runner Job that scales, restarts, rolls back and waits (Gantry's runner).
