---
title: Configuration
parent: Switchboard
grand_parent: Services
nav_order: 2
---

Platform variables (`BASE_PATH`, `SERVER_ADDR`, `SERVER_HTTP_REDIRECT_ADDR`, `SERVER_CERT_PATH`, `SERVER_KEY_PATH`, `SERVICE_NAME`, `SERVICE_AUTH_ENABLED`, `GATEHOUSE_*`, `AUTH_*`, `REDIS_URL`, `RUST_LOG`, `LOG_*`, `DATABASE_URL`, `ALLOW_IN_MEMORY_DB`, `DB_POOL_MAX_SIZE`) are in [Common service runtime](../common-runtime.md). `SERVICE_NAME` matters here: the grant check uses it as the service name (`switchboard`). The service has no config file.

| Variable | Default | Where | Meaning |
|---|---|---|---|
| `VLLM_MANAGEMENT_MODE` | native | `vllm/engine.rs` | `kubernetes` or `k8s`, `mock`, anything else native. |
| `FEATURE_MODELS_DASHBOARD_ENABLED` | `false` | `routers/mod.rs` | `1/true/yes/on` or `0/false/no/off`; other text keeps the default. Read once. |
| `FEATURE_VLLM_MANAGEMENT_ENABLED` | `false` | `routers/mod.rs` | Same. |
| `HF_ROOTS` | `/mnt/dev/huggingface/hub` | `models/mod_impl.rs`, `kubernetes.rs` | Colon-separated roots; also the allowed roots for model deletion and the read-only mounts of Kubernetes pods. |
| `GGUF_ROOTS` | `/mnt/dev/quantized` | same | Same for GGUF. |
| `VLLM_ARCHITECTURES_FILE` | `/opt/vllm_architectures.json` | `models/store.rs` | Supported architectures. |
| `VLLM_LOG_DIR` | `dist/vllm_logs` | `native.rs` | Directory of native launch logs; created on demand. |
| `VLLM_K8S_NAMESPACE` | service-account namespace, else `default` | `kubernetes.rs`, `capabilities.rs` | Namespace of pods. |
| `VLLM_K8S_GPU_RESOURCE` | `amd.com/gpu` | `kubernetes.rs` | Resource limit key (value `1`); empty or `none` requests no GPU resource. A key containing `amd`, or `none`, selects the AMD path (venv and ROCm mounts, `rocm/vllm` image). |
| `VLLM_IMAGE` | `rocm/vllm:latest` (AMD) or `vllm/vllm-openai:latest` | `kubernetes.rs` | GPU image. |
| `VLLM_VENV_PATH` | `/mnt/dev/vllm/vllm-venv` | `kubernetes.rs` | Host path mounted at `/opt/venv` (AMD). |
| `VLLM_ROCM_PATH` | `/opt/rocm` | `kubernetes.rs` | Host ROCm mounted at `/opt/rocm` (AMD). |
| `VLLM_CACHE_PATH` | `/mnt/dev/vllm/cache` | `kubernetes.rs` | Mounted at `/root/.cache/vllm` (AMD). |
| `HF_CACHE_PATH` | `/mnt/dev/huggingface/cache` | `kubernetes.rs` | Mounted at `/root/.cache/huggingface`. |
| `HSA_OVERRIDE_GFX_VERSION` | unset | `kubernetes.rs` | Passed to AMD pods when set. |
| `VLLM_CPU_IMAGE` | `vllm/vllm-openai-cpu:latest-x86_64` | `types.rs` | CPU pods. |
| `VLLM_CPU_KVCACHE_SPACE` | `4` | `types.rs` | GiB for the CPU KV cache (native: set on the process; Kubernetes: pod env). |
| `VLLM_CPU_OMP_THREADS_BIND` | unset | `kubernetes.rs` | Passed to CPU pods. |
| `DB_SCHEMA` | not read by Switchboard | | The table names `switchboard.model_cache` and `switchboard.vllm_architectures` are fixed in `store.rs`. |

Secrets: `GATEHOUSE_CLIENT_SECRET` (names only). Foreman additionally sets `SERVER_CERT_PATH` and `SERVER_KEY_PATH` to the shared development certificate.

The Python helper `service/vllm/vllm_arch_list.py` (run inside a vLLM environment) writes `vllm_architectures.json` into the current directory.
