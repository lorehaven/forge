---
title: Models and GPU sizing
parent: Switchboard
grand_parent: Services
nav_order: 7
---

Sources: `service/src/routers/models/{discovery,store,sync,list}.rs`, `service/src/routers/gpu/monitor.rs`, `core/src/{fit,model,draft}.rs`.

## Discovery

Roots: `HF_ROOTS` (colon-separated, default `/mnt/dev/huggingface/hub`) and `GGUF_ROOTS` (default `/mnt/dev/quantized`). Roots that do not exist are skipped.

HuggingFace models (`source` `HF`): every file named `config.json` under a root; the model directory is the third ancestor of the file (`models--org--name/snapshots/<rev>/config.json` gives `models--org--name`). `name` is the normalised directory name. Values read from `config.json` with defaults: `hidden_size` (4096), `num_hidden_layers` (32), `vocab_size` (32000), `max_position_embeddings` (4096), `torch_dtype` (`float16`), the first of `architectures`. Derived:

* `quant`: `quantization_config.quant_method` first (`awq` to AWQ, `gptq` to GPTQ, `fp8` to FP8, `bitsandbytes` to INT8, `compressed-tensors` by the first group's weights: 4 bits to AWQ, 8-bit float to FP8, other 8-bit to INT8); otherwise `torch_dtype` (`bfloat16` BF16, `float16` FP16, `float8` FP8, `int8` INT8, else ALL).
* `context`: `max_position_embeddings` when it is one of 512...131072 (powers of two), else `ALL`.
* `params_billion`: the number before a `b` in the name (regex `(\d+(\.\d+)?)b`, case-insensitive) or the dense-transformer formula `(vocab*hidden + 12*layers*hidden^2) / 1e9`; rounded to two decimals.
* `vllm_supported`: the architecture is in the allow-list (see below).

GGUF models (`source` `GGUF`): files with extension `gguf` whose name does not match a shard pattern (`-N-of-M`). `quant` from the file name (`q8_0`, `q6_k`, `q5_k_m`, `q5_0`, `q4_k_m`, `q4_0`, `q3_k_m`, `q2_k`, `fp16`, `f16`, `bf16`, `fp8`, `awq`, `gptq`), else `ALL`; params from the name, else 7.0; layers, hidden size and context from the GGUF metadata keys ending `.block_count`, `.embedding_length`, `.context_length` (header only), else 32, 4096, 32768. `vllm_supported` is always false and `architecture` null.

The allow-list of vLLM architectures is read from the JSON file at `VLLM_ARCHITECTURES_FILE` (default `/opt/vllm_architectures.json`, shape `{"architectures": [...]}`; `service/vllm/vllm_arch_list.py` writes it from `ModelRegistry.get_supported_archs()` and adds `vllm_version`, `generated_at`); when unreadable, a built-in list of 15 classic architectures (Llama, Mistral, Mixtral, Qwen2, GPT2, GPTNeoX, GPTBigCode, OPT, Bloom, Falcon, MPT, Phi, Gemma, Dbrx, Arctic `...ForCausalLM`/LMHeadModel) is used. At start the list is merged with the rows of `switchboard.vllm_architectures` and the file's entries are inserted into that table.

## Cache and sync

Models live in `switchboard.model_cache` (`path`, JSON `data`, `updated_at`); an in-memory map caches single lookups. At start, `warm_model_cache` scans both kinds in parallel with the wait for Gatehouse, and the service is `ready` only after both finish. Every 60 seconds `sync_models` deletes cached rows whose path is no longer on disk and rescans: known HF models are re-read and rewritten only when `quant`, `context`, `layers`, `hidden_size` or `params_billion` changed; known GGUF paths are skipped. The architectures list is re-checked on each HF scan (`vllm_supported` follows the current list for rows already cached). `POST /api/v1/models/delete` removes the file or directory and the cache row.

## Estimates

For every model, a list of what-if rows. HF models get a row for every quantisation in `FP16, BF16, FP8, INT8, AWQ, GPTQ, Q80` and every context in 512...131072 not above the model's own context (`ALL` counts as 131072). GGUF models get one quantisation (the model's own) times the contexts. Per row:

```text
weights_gb = params_billion * 1e9 * bytes_per_weight / 1024^3
kv_gb      = 2 * 2.0 * layers * hidden_size * context / 1024^3
total_gb   = (weights_gb + kv_gb + 3.0) * 1.15
```

with `bytes_per_weight`: FP16/BF16 2.0; FP8/INT8/Q80 1.0; Q6K 0.75; Q5KM 0.65; Q50 0.625; Q4KM 0.55; Q40/AWQ/GPTQ 0.5; Q3KM 0.45; Q2K 0.35. All three values are rounded to two decimals. The constants `OVERHEAD_GB = 3.0` and `FRAGMENTATION_MARGIN = 1.15` are in `discovery.rs`. The KV formula assumes 2 bytes per element and `hidden_size` wide keys and values for every layer (no grouped-query reduction), so it overestimates for GQA models.

## Choosing and sorting (service)

`best_estimate`: among rows with `total_gb <= free_gb`, the one with the largest context, ties broken by quantisation rank (FP16 100, BF16 95, FP8 90, INT8 80, Q80 70, Q6K 60, AWQ 55, Q5KM and GPTQ 50, Q50 45, Q4KM 40, Q40 35, Q3KM 30, Q2K 20). The `vram_asc` and `vram_desc` sorts of `POST /api/v1/models/list` order by the `total_gb` of that best row (1000.0 when none fits).

## GPU memory

`GET /api/v1/gpu/status` reads `/sys/class/drm/card<N>/device/mem_info_vram_total` and `mem_info_vram_used` (bytes) of every `card<digits>` directory, picks the card with the most memory (ties by name), and returns GiB with two decimals. No `rocm-smi` and no NVIDIA support. With no readable card the response is `{"name":"","total_gb":0,"used_gb":0,"free_gb":0}`. The client polls it every second.

## Fit checks in the client (`switchboard-core::fit`)

* Registry card (`card_fit`): best estimate for the free memory; margin `free - total`; tone warn when the margin is 2.0 GB or less (`TIGHT_GB`), else ok; if nothing fits, "Fits: NO" with the smallest `total_gb` ("Minimum").
* Estimates window (`estimate_rows`): rows filtered by fit (`all`, `fit`, `nofit`), context and quantisation; tone no when it does not fit, warn when the margin is 2.0 or less.
* Launch form (`launch_fit`): picks the estimate with `find_launch_estimate`: with no quantisation override, the rows of the model's own quantisation; with an override `awq`/`awq_marlin` AWQ, `gptq`/`gptq_marlin` GPTQ, `fp8` FP8, `bitsandbytes` INT8; then the first row whose context is at least `max_model_len` (else the longest row). With `u` the typed GPU memory utilisation: `model_gb = weights_gb + kv_gb * u`, `budget_gb = total_gpu_gb * u`, `required_gb = max(model_gb, budget_gb)`, `remaining = free_gb - required_gb`. Outcomes: `model_gb > budget_gb` is "won't fit" (key `ui_vllm_fit_wont_fit_budget`, launch disabled); `required_gb > free_gb` is "won't fit right now" (`..._wont_fit_free`, disabled); `remaining < 2.0` is tight (`ui_vllm_fit_tight`, enabled); otherwise ok (`ui_vllm_fit_ok`). No model chosen or no estimate disables Launch (`ui_vllm_fit_select_model`, `ui_vllm_fit_no_estimate`). A CPU launch skips the check (`ui_vllm_fit_note_cpu`, enabled).
* Suggested utilisation (`minimum_gpu_util`): `(weights_gb + 1.5) / (total_gpu_gb - kv_gb)` rounded up to the next 0.05 and clamped to 0.20..1.00; the form uses it when the model, quantisation, length or device changes, and 0.90 when unknown.

The 1.5 GB safety margin, the 2.0 GB tight threshold, the 3.0 GB overhead and the 1.15 factor are the only reserve constants in the code. A "launch reserve" of vLLM beyond these is not modelled; *Unverified:* no measurement is stored in the repository.
