# Coding Agent Implementation Plan

## Overview
Build a coding agent in Rust using the llama.cpp server for local LLM inference via HTTP API.  
The agent provides an interactive CLI interface for coding assistance with file operations and conversation memory.

---
## Implementation Steps

### ✅ Phase 1: Project Setup
- [x] Initialize Cargo project
- [x] Add core dependencies
  - [x] clap (CLI argument parsing)
  - [x] anyhow (error handling)
  - [x] rustyline (interactive REPL)
  - [x] reqwest and tokio (for HTTP API communication)
  - [x] serde_json (JSON handling)
  - [x] colored and syntect (response formatting and syntax highlighting)
  - [x] walkdir (directory traversal)
  - [x] once_cell (lazy statics)

### ✅ Phase 2: Core Components

#### ✅ LLM Interface (`src/llm.rs`)
- [x] Model loading from GGUF files via llama-server
- [x] Server process management and initialization
- [x] Context creation and management via API
- [x] Text generation with configurable parameters
  - [x] Temperature
  - [x] Top-K sampling
  - [x] Top-P sampling
  - [x] Max tokens
- [x] Token encoding/decoding (handled by server)
- [x] Mirostat sampling (v1 and v2)

#### ✅ File Operations (`src/tools/file.rs`)
- [x] Read file functionality (single and multiple)
- [x] Write/Append file functionality
- [x] Replace in file (exact string matching)
- [x] List files in directory (with extension filtering)
- [x] Directory tree traversal
- [x] File existence checking
- [x] Recursive file listing
- [x] Skip common ignore patterns (.git, target, node_modules)
- [x] Find file by pattern
- [x] Grep-like search functionality

#### ✅ Agent Core (`src/agent.rs` and `src/plan.rs`)
- [x] Agent struct with HTTP client and conversation history
- [x] System prompt configuration
- [x] Planning phase (generating a structured execution plan)
- [ ] Tool execution
  - [x] Project analysis (linter integration)
  - [x] File operations
  - [x] Git integration
  - [x] Shell command execution
- [x] Conversation history management with context window adjustment
- [x] Clear history functionality

#### ✅ CLI Interface (`src/cli.rs` and `src/main.rs`)
- [x] Command-line argument parsing
- [x] Model path configuration (CLI arg and config file)
- [x] Interactive REPL mode
  - [x] Readline with history
  - [x] Command processing loop
  - [x] Special commands (exit, clear, save, load, delete, config)
  - [x] Error handling
- [x] Single-shot query mode
- [x] Help text and user guidance
- [x] Syntax highlighting and colorized output

### ✅ Phase 3: Documentation
- [x] README.md with comprehensive usage guide
  - [x] Installation instructions
  - [x] Usage examples
  - [x] Configuration options
  - [x] Troubleshooting section
  - [x] Project structure overview
- [x] PLAN.md with implementation checklist

---

## ⏳ Phase 4: Future Enhancements (TODO)

### 🔲 Enhanced Tool System
- [x] JSON-based tool definitions
- [x] Structured tool calling (using OpenAI-style function calls)
- [ ] Code execution in sandboxed environment
- [x] Search/grep functionality

### 🔲 Improved LLM Integration
- [x] Better sampling strategies (mirostat)
- [x] Streaming token generation
- [x] Multi-turn conversation improvements
- [ ] Custom system prompts from a file
- [x] Temperature/parameter CLI overrides

### 🔲 Advanced Features
- [x] Code syntax highlighting in the terminal
- [x] Multi-file context management
- [x] Project-wide code analysis
- [ ] Automatic test generation
- [ ] Refactoring suggestions
- [ ] Code review mode

### 🔲 Performance & Quality
- [ ] Add comprehensive tests
- [ ] Benchmark LLM performance
- [x] Optimize context window usage
- [ ] Memory-efficient token handling
- [x] Error recovery and retry logic (partial)

### 🔲 User Experience
- [x] Configuration file support (`config.toml`)
- [ ] Model auto-discovery
- [x] Rich terminal UI (colors, formatting)
- [x] Progress indicators for model loading
- [x] Save/load conversation sessions
- [x] Interactive planning and execution

---

## Technical Notes

### Model Compatibility
- Supports GGUF format models via llama-server
- Tested with: CodeLlama, Llama-2, Mistral
- Requires sufficient RAM based on model size
- GPU support via llama-server features (e.g., --ngl, --flash-attn)

### Architecture Decisions
- **llama-server**: Chosen for HTTP API integration with OpenAI-compatible endpoints, simplifying Rust code and leveraging external optimizations
- **Sampling**: Uses server defaults with temperature override; can be extended
- **Conversation history**: Managed in memory without explicit limits
- **Tool detection**: Structured function calling via API; better than heuristics

### Build Configuration
- Release builds required for acceptable performance
- Debug builds will be extremely slow due to LLM inference overhead
- Consider enabling LTO (Link Time Optimization) for production

---

## Status Summary

**Completed**: ✅ All core implementation finished and refactored into modular architecture
- ✅ Project setup with Cargo and workspace-ready dependencies
- ✅ LLM interface via HTTP API with context management (`src/llm.rs`)
- ✅ Advanced file and search tools (`src/tools/file.rs`)
- ✅ Agent core with planning and tool execution (`src/agent.rs`, `src/plan.rs`)
- ✅ Modular CLI with interactive REPL and syntax highlighting (`src/cli.rs`, `src/main.rs`)
- ✅ Git integration tools
- ✅ Comprehensive documentation (README.md and updated PLAN.md)

**Current State**: Mature assistant capable of autonomous multi-step tasks across various tech stacks.

**Next Steps**:
1. Add code execution in a sandboxed environment
2. Implement automatic test generation and refactoring suggestions
3. Enhance error recovery and retry logic for tool calls
4. Expand project analysis for non-Rust projects
5. Improve multi-turn conversation coherence on very large projects
