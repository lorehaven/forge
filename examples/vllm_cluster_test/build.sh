#!/bin/bash
set -e

SCRIPT_DIR="$(dirname "$0")"

# Load .env if it exists
if [ -f "${SCRIPT_DIR}/.env" ]; then
    echo "Loading configuration from ${SCRIPT_DIR}/.env"
    # Use a method that handles values with spaces or special chars if needed, 
    # but for simple registry/tag this is usually enough:
    export $(grep -v '^#' "${SCRIPT_DIR}/.env" | xargs)
fi

# Fallback defaults
REGISTRY=${REGISTRY:-"localhost:5000"}
IMAGE_NAME="vllm-cluster-test"
TAG=${TAG:-"latest"}
FULL_IMAGE="${REGISTRY}/${IMAGE_NAME}:${TAG}"

echo "Building ${FULL_IMAGE}..."

# Build from the project root
cd "${SCRIPT_DIR}/../.."
docker build -t "${FULL_IMAGE}" -f examples/vllm_cluster_test/Dockerfile .

echo "Pushing ${FULL_IMAGE}..."
docker push "${FULL_IMAGE}"

echo "Done!"
