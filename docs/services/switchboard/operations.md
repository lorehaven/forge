---
title: Operations
parent: Switchboard
grand_parent: Services
nav_order: 4
---

## Run locally

`foreman start switchboard` (`foreman.toml`): builds the service and the browser module, runs foundry (`--install switchboard`), waits for Gatehouse, and serves HTTPS on 7443 with `BASE_PATH=/switchboard`, using the shared development certificate (`SERVER_CERT_PATH`, `SERVER_KEY_PATH`). The engine is `native` unless `VLLM_MANAGEMENT_MODE` is set, so launching needs `vllm` on `PATH`. `foreman stop` first runs the `pre_stop` hook (60 s timeout, 3 s settle) that asks Switchboard for its instances and deletes them; foreman warns about `vllm serve` processes that outlive a stop and does not kill them. Never start vLLM for a documentation or test run; use `VLLM_MANAGEMENT_MODE=mock`. `service/run-debug.sh` runs the binary with debug logging. See [Running the estate locally](../../guides/running-locally.md).

## Cluster

Image `switchboard` (`.anvil/docker.toml`: `RUN_AS=999:999`, `RESOURCE_DIR=web`, `WASM_PACKAGE=switchboard-web`; the service configuration comes from the environment). Needs: Postgres with the `switchboard` module installed by foundry, Gatehouse, a service account that may create pods, services and list/delete them in the target namespace (Kubernetes mode), the host's `/sys/class/drm` visible (a privileged pod sees it), the model roots mounted at the same paths as on the nodes (the pods mount the same host paths), and `/opt/vllm_architectures.json` (or `VLLM_ARCHITECTURES_FILE`). Readiness: `/health/ready` after Gatehouse answers and the first model scan completes. Deployment manifests are not in this repository.

## Failure modes

| Symptom | Cause |
|---|---|
| Launch returns `500` text `api_error_vllm_launch_failed` | native: `vllm` missing, no free port, or the process exited within 1.2 s (the log tail is in the service log and `last_error`); kubernetes: pod or service creation refused (name exists, quota, RBAC). The reason is only in the service log. |
| Pod stays `starting` | the readiness probe `GET /health` fails (model still loading, or crash loop with `restartPolicy: Never` gives `Failed`). |
| Instance becomes `Failed` and vanishes | the reaper removed it within 30 s. |
| Pods `completed` after a reboot, come back one at a time | the restorer; gives up after 5 consecutive restarts and leaves `completed`. |
| GPU shows `n/a` in the client | no `card*/device/mem_info_vram_total` (NVIDIA, or `/sys` hidden). |
| Models list is empty | roots do not exist or hold no `config.json`/`.gguf`; wait for the 60 s sync; check `HF_ROOTS`/`GGUF_ROOTS` (colon-separated). |
| `vllm_supported` false for everything | `VLLM_ARCHITECTURES_FILE` missing: only the built-in list of 15 architectures applies. |
| `403` on launch/stop/delete | missing `switchboard:launch`/`stop`/`delete-model`. |
| Instances not removed when the service stops | native instances are separate processes (own process group); only the pre_stop hook or `DELETE` stops them. |

## Logs

`quench_log`; engine messages: `Initializing vLLM management in <mode> mode`, `Enabled native tool calling for model ...`, `Launching model <m> for task <t> (...)`, `vLLM reaper: ...`, `vLLM restorer: ...`, `Registered N vLLM architectures from file: ...`, `Removing N stale models from cache.`, `Discovered or refreshed N HF models.`. vLLM's own output for native launches is in the file named by `log_path`.

## Tests

Unit tests under `service/tests/unit/` (one file per module: `routers_vllm_*_tests.rs` including a mocked Kubernetes engine, `routers_models_*_tests.rs`, `routers_ui_pages_tests.rs`, `client_tests.rs`), `core/tests/unit/{fit,model,draft}_tests.rs`, `web/tests/unit.rs`, `app/tests/screens.rs`. Run `cargo test -p switchboard-core -p switchboard-app -p switchboard-web -p switchboard`.
