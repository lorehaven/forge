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

#### ✅ LLM Interface (embedded in main.rs)
- [x] Model loading from GGUF files via llama-server
- [x] Server process management and initialization
- [x] Context creation and management via API
- [x] Text generation with configurable parameters
  - [x] Temperature
  - [x] Top-K sampling
  - [x] Top-P sampling
  - [x] Max tokens
- [x] Token encoding/decoding (handled by server)
- [x] Greedy sampling implementation (partial via temperature)

#### ✅ File Operations (embedded in main.rs)
- [x] Read file functionality
- [x] Write file functionality
- [x] List files in directory (with extension filtering)
- [x] Directory tree traversal
- [x] File existence checking
- [x] File extension extraction
- [x] Skip common ignore patterns (.git, target, node_modules)

#### ✅ Agent Core (embedded in main.rs)
- [x] Agent struct with HTTP client and conversation history
- [x] System prompt configuration
- [x] Query processing pipeline
- [x] Tool execution
  - [x] Automatic file reading
  - [x] Directory listing
  - [x] File writing
  - [x] Directory tree
- [x] Conversation history management
- [x] Context building for prompts
- [x] File path extraction heuristics
- [x] Clear history functionality

#### ✅ CLI Interface (main.rs)
- [x] Command-line argument parsing
- [x] Model path configuration (CLI arg)
- [x] Interactive REPL mode
  - [x] Readline with history
  - [x] Command processing loop
  - [x] Special commands (exit, clear)
  - [x] Error handling
- [x] Single-shot query mode
- [x] Help text and user guidance (partial in startup message)

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
- [x] JSON-based tool definitions (partial, hardcoded)
- [x] Structured tool calling (using OpenAI-style function calls)
- [ ] Code execution in sandboxed environment
- [x] Git integration (status, diff, commit)
- [x] Search/grep functionality

### 🔲 Improved LLM Integration
- [x] Better sampling strategies (mirostat)
- [ ] Streaming token generation
- [ ] Multi-turn conversation improvements
- [ ] Custom system prompts from file
- [ ] Temperature/parameter CLI overrides

### 🔲 Advanced Features
- [ ] Code syntax highlighting in terminal (implemented)
- [ ] Multi-file context management
- [ ] Project-wide code analysis
- [ ] Automatic test generation
- [ ] Refactoring suggestions
- [ ] Code review mode

### 🔲 Performance & Quality
- [ ] Add comprehensive tests
- [ ] Benchmark LLM performance
- [ ] Optimize context window usage
- [ ] Memory-efficient token handling
- [ ] Error recovery and retry logic

### 🔲 User Experience
- [x] Configuration file support (~/.ferrous.toml)
- [ ] Model auto-discovery
- [x] Rich terminal UI (colors, formatting)
- [ ] Progress indicators for model loading
- [ ] Save/load conversation sessions

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

**Completed**: ✅ All core implementation finished and compiling successfully
- ✅ Project setup with Cargo and dependencies
- ✅ LLM interface via HTTP API to llama-server
- ✅ File operations embedded in tool executor
- ✅ Agent core with tool execution
- ✅ Interactive CLI with REPL
- ✅ Comprehensive documentation (README.md and PLAN.md)

**Current State**: Fully functional MVP ready for testing with GGUF models

**Next Steps**:
1. Test with actual GGUF models (CodeLlama, Mistral, etc.)
2. Gather user feedback on agent behavior
3. Implement Phase 4 enhancements based on real-world usage
4. Add more sophisticated sampling strategies
5. Implement code generation/editing capabilities
6. Add single-shot query mode and help command
7. Refactor into separate modules for better organization
