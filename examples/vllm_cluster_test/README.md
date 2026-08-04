# vLLM Cluster Test

A standalone Docker image and Kubernetes `Job` manifest that smoke-tests connectivity to a vLLM deployment from inside a cluster. It's excluded from the Cargo workspace and built/applied independently with Docker and `kubectl`.

See [docs/examples/vllm_cluster_test.md](../../docs/examples/vllm_cluster_test.md) for full documentation.
