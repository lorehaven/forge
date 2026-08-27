# Switchboard Service

Switchboard is Forge's model-serving gateway and dashboard: it discovers local model files (HuggingFace-format and GGUF), estimates whether they'll fit in available GPU VRAM at various quantization/context combinations, and manages the lifecycle of vLLM server processes that actually serve them. Other services in the estate — most notably [Sage](./sage-service.md) — never talk to vLLM directly to launch or discover models; they go through Switchboard's `/api/v1/vllm/*` API, authenticating as an OAuth client-credentials client against Gatehouse. Switchboard also exposes an HTMX dashboard for browsing the local model catalog and (optionally) launching/stopping vLLM instances by hand.

## Features

- **Model discovery** — walks configured filesystem roots (`HF_ROOTS`, default `/mnt/dev/huggingface/hub`; `GGUF_ROOTS`, default `/mnt/dev/quantized`) looking for HF `config.json` directories and standalone `.gguf` files (skipping multi-part shard files). Discovered models are cached in Postgres (`switchboard.model_cache`) and re-synced every 60s (`sync.rs`) — stale entries whose files disappeared are pruned, new ones are added.
- **VRAM fit estimation** — for each discovered model, computes weight size, KV-cache size, and total VRAM need across every quantization level and context-length combination it could plausibly run at (`build_hf_estimates`/`build_gguf_estimates`), including a fixed overhead and fragmentation margin, so the dashboard can show "fits" / "does not fit" / "fits with N GB margin" against current free VRAM.
- **GGUF architecture inspection** — reads just the GGUF header/metadata (first 1 MB) to recover layer count, hidden size, and context length without loading the whole file; HF models get the same values from `config.json`, or a parameter-count formula if not directly stated.
- **vLLM architecture allow-list** — HF models are marked `vllm_supported` by checking their `architectures` field against a configurable list (`VLLM_ARCHITECTURES_FILE`, `switchboard.vllm_architectures` table).
- **vLLM instance lifecycle** — launch, list, and stop vLLM server processes through a pluggable `VllmEngine` trait with three backends selected by `VLLM_MANAGEMENT_MODE`:
  - **Native** (default) — spawns `vllm serve <model> ...` as a child process, tracks it by PID, infers live status (`starting`/`running`/`terminating`) by scanning `/proc` and tailing its log for the "Started server process" line, and picks a free port for concurrent launches. Handles the `--task`→`--runner`/`--convert` flag migration (vLLM removed `--task` in favor of `--runner`/`--convert`).
- **CPU launches** — a launch request's optional `device` field selects the execution device. Omitted / `"gpu"` / `"auto"` keeps the historical behaviour (nothing passed, vLLM auto-selects the accelerator). `"cpu"` adds `--device cpu`, drops `--gpu-memory-utilization` (meaningless on CPU), and sets `VLLM_CPU_KVCACHE_SPACE` (GiB, from the env var or a default of 4). Any other value is passed through verbatim as `--device <value>`. CPU inference cannot load FP8 checkpoints. GPU launches are entirely unaffected — the two run side by side.
  - **Native mode** relies on the `vllm` on `PATH` being a CPU-capable build.
  - **Kubernetes mode** runs a separate container image for CPU pods: `VLLM_CPU_IMAGE` (default `vllm/vllm-openai-cpu:latest-x86_64`, or a registry mirror of it). Unlike the GPU path — which runs vLLM from a host-mounted venv with `/dev/kfd`, `/dev/dri`, `/opt/rocm` and a privileged container — the CPU pod uses the image's own `vllm serve` entrypoint, mounts only the HF cache and model roots, drops the GPU device-plugin resource, and runs with just the `SYS_NICE` capability. `VLLM_CPU_OMP_THREADS_BIND`, if set, is passed through to pin OpenMP threads.
  - The launch modal exposes this as a GPU/CPU selector and, for CPU, disables the GPU-utilization field and skips the VRAM fit note.
  - **Kubernetes** — creates/lists/deletes `vllm`-labeled Pods (and Services) via the `kube` crate; falls back to Native if the in-cluster client can't be built.
  - **Mock** — for tests/local dev without real GPUs.
  - A background **reaper** (`reaper.rs`) removes instances stuck in `Failed` state every 30s, since Kubernetes pods with `restartPolicy: Never` never self-heal and would otherwise block relaunching the same model.
- **GPU status** — polls `rocm-smi --showmeminfo vram --json` once a second, broadcasts an HTML fragment over SSE (`/api/v1/gpu/status/sse`) for the dashboard's live VRAM gauge, and serves it as JSON at `/api/v1/gpu/status`.
- **Fine-grained permissions** — write actions are gated per-action (`launch`, `stop`, `delete-model`) via Gatehouse's permission catalog rather than a single coarse "write" grant, checked directly in each handler (`mod_impl::can`) rather than through blanket middleware, because a plain `RequireWrite` grant can't express "may launch but not delete."
- **Feature flags** — `FEATURE_MODELS_DASHBOARD_ENABLED` and `FEATURE_VLLM_MANAGEMENT_ENABLED` gate the corresponding UI sections independently of the underlying API.

## Architecture

- **`routers/models/`** — the model catalog: `discovery.rs` (filesystem scan + estimate building), `store.rs` (Postgres-backed cache, warm-up on boot), `sync.rs` (60s reconciliation loop), `list.rs` (filter/sort/paginate + HTMX grid + VRAM-estimate modal rendering), `delete.rs` (removes a model file from disk, path-validated against `HF_ROOTS`/`GGUF_ROOTS`), `running.rs` (admin-only view of currently loaded models), `types.rs` (`Model`, `ModelEstimate`, `Quant`, `Context` enums with the VRAM-estimation math).
- **`routers/vllm/`** — instance management: `engine.rs` (the `VllmEngine` trait + `VllmManagementMode`), `native.rs`, `kubernetes.rs`, `mock.rs` (the three engines), `launch.rs`/`stop.rs`/`list.rs`/`modals.rs` (HTTP handlers and HTMX fragments), `sse.rs` (live instance-grid updates), `reaper.rs`, `types.rs` (`VllmInstance`, `LaunchRequest`, task↔CLI-flag translation).
- **`routers/gpu/`** — VRAM polling and SSE broadcast.
- **`routers/ui/`** — HTMX dashboard: home, models dashboard, vLLM management pages, auth delegation to Gatehouse (same pattern as Sage — no local login).
- **`lib.rs`** — wires the above into `root_scope`/`base_path_scope`, installing broadcaster channels and JWT/session config as shared Actix app data.

## API routes / UI pages

Base path `/switchboard` in local dev; all API routes wrapped in `Auth` (JWT), write routes additionally checked per-action:

- **Models** (`/api/v1/models`): `POST /list` (JSON, filtered), `GET /grid` (HTMX), `GET /estimates-modal`, `GET /estimates-modal/empty`, `GET /delete-modal`, `GET /delete-modal/empty`, `POST /delete` and `POST /delete-form` (requires `delete-model`), `GET /running` (admin-only).
- **vLLM** (`/api/v1/vllm`): `GET /list` and `GET /instances` (alias), `GET /grid`, `GET /launch-modal`/`stop-modal` (+ empty variants), `POST /instances` (launch; requires `launch`) and its form variant, `DELETE /instances/{id}` (stop; requires `stop`), `GET /status/sse` (canonical) and alias.
- **GPU** (`/api/v1/gpu`): `GET /status`, `GET /status/sse`.
- **UI** (`/ui`): `/home`, `/models` (dashboard, behind `FEATURE_MODELS_DASHBOARD_ENABLED`), `/vllm` (management, behind `FEATURE_VLLM_MANAGEMENT_ENABLED`), plus `/login`, `/auth/callback`, `/logout`, `/status` delegating to Gatehouse.

## Requirements

- **PostgreSQL** — `switchboard.users`/`switchboard.sessions` (auth), `switchboard.model_cache` (discovered-model cache, keyed by path), `switchboard.vllm_architectures` (vLLM-support allow-list).
- **Gatehouse** — JWT/session auth and SSO delegation; `switchboard` is its own OAuth client and the target of Sage's client-credentials grant.
- **`vllm`** on `PATH` (Native mode) or a reachable Kubernetes cluster (Kubernetes mode) to actually serve models.
- **`rocm-smi`** on `PATH` for GPU status — the code shells out to it directly, so this service currently assumes an AMD ROCm GPU host; if it's missing, GPU status silently defaults to zeroed values.
- **Local filesystem access** to the HF/GGUF model directories being scanned (or the equivalent mounted paths in a container/pod).

## Configuration

Key environment variables (see `docker/switchboard-service/.env`):

- `VLLM_MANAGEMENT_MODE` — `native` (default), `kubernetes`/`k8s`, or `mock`.
- `HF_ROOTS`, `GGUF_ROOTS` — colon-separated filesystem roots to scan (defaults `/mnt/dev/huggingface/hub`, `/mnt/dev/quantized`).
- `VLLM_ARCHITECTURES_FILE` — path to a JSON file listing vLLM-supported HF architectures.
- `VLLM_K8S_NAMESPACE` — namespace for the Kubernetes engine (falls back to the in-cluster service-account namespace file, then `default`).
- `VLLM_LOG_DIR` — where Native-mode launch logs are written (default `dist/vllm_logs`).
- `VLLM_CPU_KVCACHE_SPACE` — GiB the CPU backend reserves for the KV cache on `device: "cpu"` launches (default `4`); set on the vLLM process (native) or pod (Kubernetes).
- `VLLM_CPU_IMAGE` — Kubernetes-mode container image for `device: "cpu"` pods (default `vllm/vllm-openai-cpu:latest-x86_64`); point it at a registry mirror in air-gapped estates.
- `VLLM_CPU_OMP_THREADS_BIND` — optional; passed to CPU pods to pin vLLM's OpenMP threads to specific cores (e.g. `0-15`, `auto`).
- `FEATURE_MODELS_DASHBOARD_ENABLED`, `FEATURE_VLLM_MANAGEMENT_ENABLED` — UI feature toggles.
- `DATABASE_URL`, `DB_SCHEMA` (`switchboard`), `DB_POOL_MAX_SIZE`, `DB_MIGRATION_TABLE`, `DB_RECREATE`.
- `GATEHOUSE_CLIENT_SECRET`, `GATEHOUSE_TLS_VERIFY` — this service's own OAuth identity toward Gatehouse.

In local dev (`foreman.toml`), Switchboard runs on port 7443 under base path `/switchboard`, `needs = ["gatehouse"]` (started before Sage, which depends on it), and has a `pre_stop` hook that authenticates as `sage-switchboard` to gracefully stop every running vLLM instance before the service itself shuts down — vLLM processes are Switchboard's children, not foreman's, and would otherwise be orphaned holding the GPU and their ports.

## Testing

Unit tests live under `tests/unit/` (currently `routers_models_types_tests.rs`, covering the `Quant`/`Context`/estimate math), aggregated through `tests/unit.rs`. Run via `anvil test -p switchboard-service` or `foreman test switchboard`.

[Home](../README.md)
