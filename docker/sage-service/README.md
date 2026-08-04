# Sage Service

Sage is Forge's AI workspace/chat service: an Actix Web application that gives users a browser-based chat UI (and a matching JSON API) backed by vLLM models served through Switchboard. It manages conversations organized into projects, supports tool-calling (web search, calculator, file operations, shell commands, code execution), and lets users upload files that are extracted, chunked, embedded into pgvector, and retrieved as RAG context to ground chat answers.

See [docs/docker/sage-service.md](../../docs/docker/sage-service.md) for full documentation.
