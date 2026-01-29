# Ferrous 🤖

Ferrous is an expert multi-purpose assistant and autonomous agent designed to help analyze, modify, improve, and maintain projects efficiently and safely. It runs locally using a `llama.cpp` server, providing a private and fast alternative to cloud-based coding assistants.

Originally focused on Rust, Ferrous has evolved into a versatile tool capable of discovering and working with various technologies and project structures.

## Core Capabilities

- **Autonomous Agent**: Ferrous can plan and execute multi-step tasks using a suite of built-in tools.
- **Smart Planning**: Generates a structured execution plan before starting work, ensuring transparency and safety.
- **Advanced File Operations**: Beyond simple read/write, it can perform exact string replacements, search text (grep-style), and find files by pattern.
- **Git Integration**: Built-in support for checking status, viewing diffs, staging changes, and committing.
- **Project Analysis**: Integrates with build tools and linters (like `cargo clippy` for Rust) to provide project-wide insights.
- **Interactive REPL & One-Shot Query**: Use it as a persistent assistant or for quick tasks.
- **Session Management**: Save and load conversation histories to pick up where you left off.
- **Syntax Highlighting**: Beautifully rendered code blocks and terminal output.
- **GPU Acceleration**: Built-in support for GPU-accelerated inference via `llama.cpp`.

## Available Tools

Ferrous uses a sophisticated tool-calling mechanism to interact with your project:

- `analyze_project()`: Runs appropriate static analysis tools for the project type.
- `read_file(path)` / `read_multiple_files(paths)`: Reads file content into context.
- `write_file(path, content)`: Creates or overwrites files.
- `replace_in_file(path, search, replace)`: Performs precise, minimal edits.
- `search_text(pattern)` / `find_file(pattern)`: Quickly locates code or files.
- `list_directory()` / `get_directory_tree()`: Explores the project structure.
- `execute_shell_command(command)`: Runs safe, project-specific commands (build, test, etc.).
- `git_status()` / `git_diff()` / `git_add()` / `git_commit()`: Manage version control.

## Prerequisites

- **Rust 1.83+** (edition 2024)
- **llama-server**: Download or build from [llama.cpp](https://github.com/ggerganov/llama.cpp).
- **GGUF Model**: A high-quality coding model is recommended (e.g., Qwen 2.5 Coder, DeepSeek-Coder-V2).

## Installation

```bash
git clone https://github.com/lorehaven/build-tools.git
cd build-tools/ferrous
cargo build --release
```

The binary will be located at `target/release/ferrous`.

## Usage

### Launching Ferrous

Start the interactive assistant:
```bash
./target/release/ferrous --model /path/to/your/model.gguf
```

### REPL Commands

- `help`: Show help and available tools.
- `clear`: Clear conversation history.
- `save [name]`: Save current session.
- `load [prefix]`: Load a previous session.
- `list`: List saved conversations.
- `delete [prefix]`: Delete a saved conversation.
- `config`: Show current configuration.
- `exit` / `quit`: End the session.

### One-Shot Query

```bash
ferrous query --text "Explain the architecture of the riveter module"
```

## Configuration

Ferrous can be configured via a `config.toml` file in the current directory:

```toml
model = "models/qwen2.5-coder-7b-instruct.gguf"
temperature = 0.1
context = 32768
max_tokens = 16384
debug = false
```

## Hardware & Performance

Ferrous is optimized for local execution. It automatically attempts to offload work to your GPU if supported by your `llama-server` build (Vulkan, CUDA, or Metal). For large projects, models with ≥32k context are recommended.

### License
MIT
