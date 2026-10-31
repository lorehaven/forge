#!/bin/sh
# Logs the job's Docker CLI in to the registries it pushes images to. The daemon is the job's `dind` service (see
# .conveyor.toml), reached through DOCKER_HOST; what it pushes it pushes with the credentials stored here.
#
#   DOCKER_REGISTRIES         the registry hosts to log in to, separated by spaces (as `docker login` takes them: a
#                             host and port, no path), for example `ennor.ddns.net ossiriand.arda:8080`
#   DOCKER_REGISTRY_USER,
#   DOCKER_REGISTRY_PASSWORD  the conveyor secrets holding one account's name and password (or token)
#
# The password goes in through standard input, not the command line, so it is in no process list.
set -eu

: "${DOCKER_REGISTRIES:?DOCKER_REGISTRIES is not set: the registry hosts to log in to}"
: "${DOCKER_REGISTRY_USER:?the conveyor secret DOCKER_REGISTRY_USER is not set}"
: "${DOCKER_REGISTRY_PASSWORD:?the conveyor secret DOCKER_REGISTRY_PASSWORD is not set}"

# The daemon is a sidecar: ready() already waited for it, but the CLI should not fail on a daemon a moment late.
tries=0
until docker info >/dev/null 2>&1; do
  tries=$((tries + 1))
  if [ "$tries" -ge 30 ]; then
    echo "no Docker daemon at ${DOCKER_HOST:-the default socket}: the job needs the privileged dind service (and the deployment must allow it)" >&2
    exit 1
  fi
  sleep 2
done

for registry in $DOCKER_REGISTRIES; do
  printf '%s' "$DOCKER_REGISTRY_PASSWORD" | docker login "$registry" --username "$DOCKER_REGISTRY_USER" --password-stdin
done
