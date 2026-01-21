# Ferrous 🤖

A minimal, local coding assistant that runs on your machine using llama.cpp server.  
Safe filesystem tools, syntax-highlighted output, GPU acceleration (AMD Vulkan tested).  
Written in Rust. Designed for developers who want fast, private coding help without cloud dependencies.

## Current Features

- Interactive REPL with command history (via rustyline)
- Syntax-highlighted code blocks in terminal (syntect + Markdown detection)
- Structured tool use via function calling:
  - read_file(path)
  - write_file(path, content)
  - list_directory(path)
  - get_directory_tree(path)
- Path safety: canonicalization + traversal prevention
- Single-shot query mode (ferrous query --text "...")
- help, clear, exit / quit commands
- GPU acceleration support (tested on AMD RX 7900 XTX via Vulkan)
- Colored output & clean UX
- Configurable sampling parameters (--temperature, --top-p, --top-k, --max-tokens)
- Stable non-streaming mode (reliable tool calling)

## Prerequisites

- Rust 1.75+
- llama-server binary from https://github.com/ggerganov/llama.cpp
  - Recommended build: -DGGML_VULKAN=ON (for AMD GPUs)
  - Recommended for Arch Linux: install llama.cpp-vulkan package from aur
- A GGUF model (strongly recommended: Qwen 2.5 Coder, DeepSeek-Coder, CodeLlama)
- Vulkan drivers (mesa / RADV on Linux)

## Installation

git clone https://github.com/yourusername/ferrous.git
cd ferrous
cargo build --release

Binary: target/release/ferrous

## Quick Start

### Interactive mode

# Most common usage
```
ferrous --model /mnt/dev/quantized/qwen2.5-coder-14b-instruct-q5_k_m.gguf
```

# With custom sampling & debug logs
```
ferrous --model ... --temperature 0.25 --top-p 0.85 --max-tokens 4096 --debug
```

### One-shot query

```
ferrous query --text "Write a Rust function that safely reads a TOML config file"
```

# With overrides
```
ferrous query --text "..." --temperature 0.1 --max-tokens 1024
```

## In-REPL Commands

```
>> help                  # show this help
>> clear                 # reset conversation history
>> exit  /  quit
>> List files here
>> Read Cargo.toml
>> Write hello world to main.py
>> Explain this error: expected &str, found String
```

## Recommended Models (2025–2026)

| Model                              | Quant     | VRAM est. | Notes                                 |
|------------------------------------|-----------|-----------|---------------------------------------|
| Qwen2.5-Coder-14B-Instruct         | Q5_K_M    | ~10–12 GB | excellent Rust & general coding       |
| DeepSeek-Coder-V2-Lite-Instruct    | Q5_K_M    | ~9–11 GB  | very fast, strong reasoning           |
| CodeLlama-34B-Instruct             | Q4_K_M    | ~18–20 GB | classic, still good                   |
| Qwen2.5-Coder-7B-Instruct          | Q6_K      | ~6–8 GB   | faster, lighter alternative           |

## Performance & Hardware Notes

- First model load can take 30–180 seconds (Vulkan shader compilation + weights transfer)
- Use full offload: -ngl 999 (already in code)
- Monitor GPU: radeontop, rocm-smi
- Release build required (cargo build --release)
- Recommended context: -c 4096 (adjustable in code) to avoid VRAM pressure

## Troubleshooting

Agent hangs at startup  
→ Wait 1–3 minutes the first time (model loading and shader cache)  
→ Check console for Vulkan device detection  
→ Run llama-server manually with same args to debug

No GPU usage / slow inference  
→ Verify device: llama-cli --list-devices  
→ Ensure --device Vulkan0 and GGML_VULKAN_DEVICE=0  
→ Check VRAM usage during the load

Port already in use  
→ lsof -i :8080 or change --port 8081

Server crashes during generation  
→ Reduce context size (-c 4096 or lower)  
→ Try --flash-attn off if using large context  
→ Monitor rocm-smi for VRAM exhaustion

## Project Status (Jan 2026)

- Single-file MVP – fully functional and stable (non-streaming for reliability)
- Next priorities:
  - split into modules (agent.rs, tools.rs, llm.rs)
  - simulated typewriter effect for full-response output
  - conversation save/load
  - more tools (git status/diff, grep, run tests)
  - optional hybrid streaming (non-stream tools → stream final answer)

### License
MIT
