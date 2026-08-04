# Switchboard Service

Switchboard is Forge's model-serving gateway and dashboard: it discovers local model files (HuggingFace-format and GGUF), estimates whether they'll fit in available GPU VRAM at various quantization/context combinations, and manages the lifecycle of vLLM server processes that actually serve them. Other services in the estate — most notably Sage — never talk to vLLM directly to launch or discover models; they go through Switchboard's `/api/v1/vllm/*` API. Switchboard also exposes an HTMX dashboard for browsing the local model catalog and (optionally) launching/stopping vLLM instances by hand.

See [docs/docker/switchboard-service.md](../../docs/docker/switchboard-service.md) for full documentation.
