---
title: Instances
parent: Switchboard
grand_parent: Services
nav_order: 8
---

An instance is one vLLM server, described by the `VllmInstance` object (see [API](api.md)).

## Ids and namespaces

| Engine | `id` | `namespace` | `host` | `log_path` |
|---|---|---|---|---|
| native | `pid-<pid>` | `native` | `--host` of the process | set for instances this process launched |
| kubernetes | `pod:<namespace>:<pod name>` | the pod's namespace | `<pod>.<namespace>.svc.cluster.local` | none |
| mock | `mock-<epoch millis>` | request namespace or `default` | request host | none |

The namespace reported by `GET /api/v1/vllm/capabilities` is the one the launch form pre-fills: `native`, `mock`, or the Kubernetes namespace.

## Status values

| Status | Native | Kubernetes | Mock |
|---|---|---|---|
| `starting` | launched by this process, log has no `Started server process [pid]` yet | pod phase `Pending`, or `Running` but not Ready | |
| `running` | log shows the server started, or any `vllm serve` process this run did not launch | `Running` and condition Ready is True (the `/health` probe passes) | always |
| `terminating` | stopped by this service, listed for 15 s | the pod has a deletion timestamp | |
| `completed` | | phase `Succeeded` (restorable) | |
| `interrupted` | | phase `Failed` with reason `Shutdown`, `Terminated` or `UnexpectedAdmissionError` (restorable) | |
| `Failed` | | phase `Failed` for any other reason (raw phase text; the reaper removes it) | |
| other | | any other phase text as is; `unknown` without a phase | |

State machine (Kubernetes): `starting` to `running` when Ready; `running` to `completed` or `interrupted` after a reboot, then the restorer recreates the pod (back to `starting`, at most 5 times in a row); a crash goes to `Failed` and is removed by the reaper; a stop goes to `terminating` and then the pod disappears. The native engine has `starting`, `running`, `terminating` and then no entry. A native launch that dies within 1.2 s is reported as an error and never listed.

The client colours `failed`, `Failed`, `completed`, `interrupted` as failures, `starting` and `terminating` as warnings, everything else as ok; the stop button is shown unless the status is `failed` or `terminating` (so it appears for `Failed`, whose literal `failed` comparison is case-sensitive).
