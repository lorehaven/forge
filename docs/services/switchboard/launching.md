---
title: Launching
parent: Switchboard
grand_parent: Services
nav_order: 9
---

Sources: `service/src/routers/vllm/{engine,native,kubernetes,mock,types,launch,stop,reaper}.rs`.

## Engines

`VLLM_MANAGEMENT_MODE` (case-insensitive): `kubernetes` or `k8s` selects Kubernetes, `mock` the mock, anything else (including unset) `native`. If the Kubernetes client cannot be created (`Client::infer`), the service logs `Failed to initialize Kubernetes vLLM engine: ... Falling back to Native.` and runs native. The engine is created once at start (`Initializing vLLM management in <mode> mode`). The trait `VllmEngine` has `list_instances`, `launch_instance`, `stop_instance` and `restore_next` (default: nothing to restore).

### Launch request to vLLM arguments

Common to native and Kubernetes (`serve_args` / `launch_args`), in this order, each only when the field is set:

| Request field | Argument |
|---|---|
| `quantization` | `--quantization <v>` |
| `dtype` | `--dtype <v>` |
| `device` (not cpu/gpu/auto/default/empty) | `--device <v>` |
| `limit_mm_per_prompt` | `--limit-mm-per-prompt <v>` |
| `max_num_seqs` | `--max-num-seqs <n>` |
| `max_model_len` | `--max-model-len <n>` |
| `gpu_memory_utilization` (not for CPU) | `--gpu-memory-utilization <v with 2 decimals>` |
| `enable_prefix_caching` | `--enable-prefix-caching` |
| `enable_tool_calling` | `--enable-auto-tool-choice --tool-call-parser hermes` |
| `task` | `embed`/`embedding`: `--runner pooling --convert embed`; `classify`: `--runner pooling --convert classify`; `generate`: `--runner generate`; other: `--runner <value>` |

Native prefix: `vllm serve <model> --host <host> --port <port>`. Kubernetes prefix: `--model <model> --host 0.0.0.0 --port <port>` (GPU image's API server) or `<model> --host 0.0.0.0 --port <port>` (CPU image). CPU is chosen when `device` equals `cpu` (any case, trimmed); it is not passed to vLLM (`--device cpu` is rejected by current vLLM).

### Native

`launch_instance`: finds the first port at or above `port` that no non-terminating launch record claims and that `TcpListener::bind((host, port))` accepts (error `no free port available starting from <port>`); opens the log file `$VLLM_LOG_DIR/<YYYYmmddTHHMMSS>-<model with / as __>-<port>.log` (default directory `dist/vllm_logs`) for stdout and stderr; spawns `vllm` in its own process group with stdin closed (for CPU launches also `VLLM_CPU_KVCACHE_SPACE=<gib>`); renames the log to `...-pid-<pid>.log`; records the launch under the key `<model with / as -- >-<port>`; waits 1.2 seconds; if the process has already exited, stores the last 40 log lines as `last_error` and returns an error (`vLLM exited during startup with status ... Log: ...`, which the API turns into the key `api_error_vllm_launch_failed`). On success it returns an instance with id `pid-<pid>`, namespace `native`, status `starting`.

`list_instances` scans `/proc/<pid>/cmdline` for command lines containing `vllm serve` and rebuilds the instance from the arguments (`--host`, `--port`, `--quantization`, `--dtype`, `--limit-mm-per-prompt`, `--max-model-len`, `--gpu-memory-utilization`, `--enable-prefix-caching`, `--enable-auto-tool-choice`, `--convert`/`--runner`/`--task`, `--device`), so instances survive a restart of Switchboard. Status: a process launched by this run is `starting` until its log contains `Started server process [<pid>]`, then `running`; any other process is `running`. A stopped instance stays in the listing as `terminating` for up to 15 seconds.

`stop_instance(pid-N)`: marks the record terminating and runs `kill -TERM N`; error `Failed to kill process N`. An id that does not start with `pid-` is `Invalid instance id: <id>`.

### Kubernetes

Pod name `vllm-<model with / and . replaced by -, lowercased>-<port>`, at most 63 characters; longer names are truncated with an 8-hex-digit hash (`vllm-<truncated>-<hash>-<port>`). The pod is created in `req.namespace` or the engine's namespace (`VLLM_K8S_NAMESPACE`, else the service-account namespace file, else `default`), with a `ClusterIP` service of the same name owned by the pod (selector label `vllm-pod-name`).

Pod labels: `app=vllm`, `app.kubernetes.io/managed-by=switchboard`, `vllm-model` (model with `/` as `--`), `vllm-port`, `vllm-pod-name`. Annotations hold the launch parameters (`vllm-model`, `vllm-quantization`, `vllm-dtype`, `vllm-limit-mm-per-prompt`, `vllm-max-model-len`, `vllm-gpu-memory-utilization`, `vllm-enable-prefix-caching`, `vllm-enable-tool-calling`, `vllm-task`, `vllm-device`); `list_instances` reads instances back from them. The managed set is the selector `app.kubernetes.io/managed-by=switchboard,app=vllm`. Spec: one container `vllm`, `restartPolicy: Never`, `hostIPC: true`, readiness probe `GET /health` on the port (initial delay 5 s, period 5 s, timeout 3 s), env `HF_HUB_OFFLINE=1`.

| Path | Image | Mounts and settings |
|---|---|---|
| GPU | `$VLLM_IMAGE`, default `rocm/vllm:latest` (AMD; the resource key contains `amd` or is `none`) else `vllm/vllm-openai:latest` | command `/opt/venv/bin/python -m vllm.entrypoints.openai.api_server`; `privileged: true`; resource limit `<VLLM_K8S_GPU_RESOURCE, default amd.com/gpu>: 1` (none when the key is empty or `none`); hostPath mounts `/dev/kfd`, `/dev/dri`; AMD: `$VLLM_VENV_PATH` (default `/mnt/dev/vllm/vllm-venv`) at `/opt/venv`, `$VLLM_ROCM_PATH` (default `/opt/rocm`), `$VLLM_CACHE_PATH` (default `/mnt/dev/vllm/cache`) at `/root/.cache/vllm`; env `VLLM_WORKER_MULTIPROC_METHOD=spawn`, `VLLM_TARGET_DEVICE=rocm`, and `HSA_OVERRIDE_GFX_VERSION` when set in Switchboard's environment |
| CPU | `$VLLM_CPU_IMAGE`, default `vllm/vllm-openai-cpu:latest-x86_64` | capability `SYS_NICE`, no GPU resource, env `VLLM_CPU_KVCACHE_SPACE` (default 4) and `VLLM_CPU_OMP_THREADS_BIND` if set |
| both | | HF cache `$HF_CACHE_PATH` (default `/mnt/dev/huggingface/cache`) at `/root/.cache/huggingface`; every directory of `HF_ROOTS` and `GGUF_ROOTS` read-only at the same path |

Returned instance: id `pod:<namespace>:<pod name>`, host `<pod>.<namespace>.svc.cluster.local`, status `starting`. `stop_instance` deletes the pod (ids `pod:<ns>:<name>` or the older `pod-<name>` in the default namespace; anything else is `Invalid instance id for kubernetes: <id>`).

### Mock

An in-process map. Launch returns id `mock-<epoch millis>`, namespace `req.namespace` or `default`, status `running`. Stop of an unknown id is the error `Instance <id> not found`, which the API turns into `404`.

## Background tasks

* Reaper (`spawn_reaper`, every 30 s, all engines): stops every instance whose status is exactly `Failed` (the raw pod phase of a crashed Kubernetes pod; a `restartPolicy: Never` pod would otherwise block relaunching the model with "already exists"). Native and mock never report `Failed`.
* Restorer (`spawn_restorer`, every 30 s; only the Kubernetes engine implements `restore_next`): if no managed pod is still loading (status `starting` and younger than 1200 s), picks the first (by name) pod whose status is `completed` or `interrupted` (what a machine reboot leaves: phase `Succeeded`, or `Failed` with reason `Shutdown`, `Terminated` or `UnexpectedAdmissionError`), deletes the pod and its service, waits for the pod to vanish, recreates both from the old pod's spec, labels and annotations. The annotation `vllm-restarts` counts consecutive restarts; it resets to 1 when the old container ran at least 600 s; above 5 the pod is left as it is and `vLLM restorer: <ns>/<name> keeps stopping; giving up after 5 restarts` is logged.
* Model sync (`start_sync_job`, every 60 s): see [Models and GPU sizing](gpu-sizing.md).

## Not implemented

No proxying of inference requests, no log endpoint (the native log path is returned as `log_path`), no restart/start endpoint, no GPU lease or reservation across instances.
