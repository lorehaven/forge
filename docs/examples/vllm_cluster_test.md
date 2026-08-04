# Example: vLLM Cluster Test

`examples/vllm_cluster_test` is not a Rust crate — it's a standalone Docker image and Kubernetes `Job` manifest used to smoke-test connectivity to a vLLM deployment from inside a cluster. It's excluded from the Cargo workspace entirely (`exclude = ["examples/vllm_cluster_test"]` in the root `Cargo.toml`), so it never participates in `cargo build`/`test` for the rest of the workspace; it's built and applied independently with Docker and `kubectl`.

## What it covers

- **`Dockerfile`**: a `python:3.11-slim` image with `requests` installed, whose `ENTRYPOINT` runs a small inline Python script (`/usr/local/bin/vllm-test.py`, written by the `Dockerfile` itself via a `RUN echo ... >` step rather than copied in as a separate file). The script:
  - Takes `--host`, `--port` (default `8000`), and an optional `--model`.
  - `GET`s `http://<host>:<port>/v1/models` and prints the result.
  - If `--model` is given, `POST`s a minimal chat-completion request (`"Say 'Connection Verified'"`, `max_tokens: 10`) to `http://<host>:<port>/v1/chat/completions` and prints the reply.
  - Exits non-zero on any request failure, so the Kubernetes Job reports failure correctly.
- **`job.yaml`**: a `batch/v1` `Job` (`vllm-cluster-test`, `restartPolicy: Never`, `backoffLimit: 0`) that runs the image above with `--host`/`--port`/`--model` filled in from `{{REGISTRY}}`, `{{TAG}}`, `{{VLLM_HOST}}`, `{{VLLM_PORT}}`, `{{VLLM_MODEL}}` placeholders.
- **`build.sh`**: builds `${REGISTRY}/vllm-cluster-test:${TAG}` from the workspace root (`docker build -f examples/vllm_cluster_test/Dockerfile .`) and pushes it.
- **`apply.sh`**: loads `.env`, substitutes the same placeholders into `job.yaml` with `sed`, deletes any prior `vllm-cluster-test` Job, and `kubectl apply`s the rendered manifest.
- **`.env.example`**: documents the variables both scripts read — `REGISTRY`, `TAG`, `VLLM_HOST`, `VLLM_PORT`, `VLLM_MODEL`. A real `.env` (gitignored) supplies actual values, e.g. `VLLM_HOST=vllm-qwen-qwen2-5-0-5b-instruct-8000.vllm.svc.cluster.local`, `VLLM_MODEL=Qwen/Qwen2.5-0.5B-Instruct`.

## How to run it

```bash
cd examples/vllm_cluster_test
cp .env.example .env   # then edit REGISTRY/TAG/VLLM_HOST/VLLM_PORT/VLLM_MODEL
./build.sh              # docker build + push
./apply.sh               # renders job.yaml and kubectl apply's it
kubectl logs -f job/vllm-cluster-test
```

## Requirements

- `docker` (build/push access to the configured `REGISTRY`).
- `kubectl`, configured against the target cluster, with a `vllm` namespace/service reachable at `VLLM_HOST:VLLM_PORT`.
- Both scripts are `bash` (`#!/bin/bash`) and use `set -e`.

## Configuration

All configuration is environment-variable driven via `.env` (see `.env.example`); there is no other config file. Defaults baked into the scripts if `.env` is absent or a variable is unset: `REGISTRY=localhost:5000`, `TAG=latest`, `VLLM_HOST=vllm-llama3-8000.vllm.svc.cluster.local`, `VLLM_PORT=8000`, `VLLM_MODEL=llama3`.

[Home](../README.md)
