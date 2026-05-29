#!/bin/bash
set -e

SCRIPT_DIR="$(dirname "$0")"

# Load .env if it exists
if [ -f "${SCRIPT_DIR}/.env" ]; then
    echo "Loading configuration from ${SCRIPT_DIR}/.env"
    export $(grep -v '^#' "${SCRIPT_DIR}/.env" | xargs)
fi

# Fallback defaults
REGISTRY=${REGISTRY:-"localhost:5000"}
TAG=${TAG:-"latest"}
VLLM_HOST=${VLLM_HOST:-"vllm-llama3-8000.vllm.svc.cluster.local"}
VLLM_PORT=${VLLM_PORT:-"8000"}
VLLM_MODEL=${VLLM_MODEL:-"llama3"}

# Use a temporary file for the processed manifest
TEMP_YAML=$(mktemp)

echo "Preparing Job manifest..."

# Use sed to replace placeholders
# Note: Using | as delimiter in sed to handle slashes in registry/model names
sed -e "s|{{REGISTRY}}|${REGISTRY}|g" \
    -e "s|{{TAG}}|${TAG}|g" \
    -e "s|{{VLLM_HOST}}|${VLLM_HOST}|g" \
    -e "s|{{VLLM_PORT}}|${VLLM_PORT}|g" \
    -e "s|{{VLLM_MODEL}}|${VLLM_MODEL}|g" \
    "${SCRIPT_DIR}/job.yaml" > "$TEMP_YAML"

echo "Applying Job to cluster..."
kubectl delete job vllm-cluster-test --ignore-not-found
kubectl apply -f "$TEMP_YAML"

rm "$TEMP_YAML"
echo "Done! Check logs with: kubectl logs -f job/vllm-cluster-test"
