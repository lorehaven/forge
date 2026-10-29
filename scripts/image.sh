#!/usr/bin/env bash
# Builds, and optionally pushes, one of the images in images/ (run from anywhere; foreman's `build-*` and
# `publish-*` tasks call this):
#
#   scripts/image.sh rust-builder build
#   scripts/image.sh gantry-runner publish
#
# `pedlar-service` is the catalogue service of services/pedlar (its own cargo workspace, so its own build context):
#
#   scripts/image.sh pedlar-service publish
#
# The registry's crate token is passed as a build argument when ENNOR_TOKEN is set; pushing needs
# `docker login ossiriand.arda:8080` first.
set -euo pipefail

registry="ossiriand.arda:8080/forge"
root="$(cd "$(dirname "$0")/.." && pwd)"
name="${1:?image name: rust-builder, gantry-runner or pedlar-service}"
action="${2:?build or publish}"

case "$name" in
  rust-builder) tag="$registry/ci/rust-builder:latest" ;;
  gantry-runner)
    # Tagged with Gantry's version: the service names the image and the two are released together.
    version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/docker/gantry-service/Cargo.toml" | head -1)"
    [ -n "$version" ] || { echo "cannot read gantry-service's version" >&2; exit 1; }
    tag="$registry/gantry/runner:$version"
    ;;
  pedlar-service)
    version="$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' "$root/services/pedlar/Cargo.toml" | head -1)"
    [ -n "$version" ] || { echo "cannot read pedlar's version" >&2; exit 1; }
    tag="$registry/pedlar/pedlar:$version"
    dockerfile="$root/services/pedlar/service/Dockerfile"
    context="$root/services/pedlar"
    ;;
  *) echo "unknown image '$name' (rust-builder, gantry-runner, pedlar-service)" >&2; exit 2 ;;
esac
case "$action" in build | publish) ;; *) echo "unknown action '$action' (build, publish)" >&2; exit 2 ;; esac

dockerfile="${dockerfile:-$root/images/$name/Dockerfile}"
context="${context:-$root}"

extra=()
if [ -n "${ENNOR_TOKEN:-}" ]; then
  extra=(--build-arg "CARGO_REGISTRIES_ENNOR_TOKEN=$ENNOR_TOKEN")
fi

docker build "${extra[@]}" -f "$dockerfile" -t "$tag" "$context"
echo "built $tag"
if [ "$action" = publish ]; then
  docker push "$tag"
  echo "pushed $tag"
fi
