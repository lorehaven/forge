#!/usr/bin/env bash
# Builds the browser clients (every services/<name>/web crate) for wasm and puts each module where its service reads it:
# services/<name>/service/web/app.wasm (WEB_DIR defaults to `web` in the service's working directory). The images do the
# same in their build (WASM_PACKAGE); this is what `foreman` runs before a start.
set -euo pipefail
cd "$(dirname "$0")/.."
for dir in services/*/web; do
  [ -f "$dir/Cargo.toml" ] || continue
  service="$(basename "$(dirname "$dir")")"
  package="$(sed -n 's/^name = "\(.*\)"/\1/p' "$dir/Cargo.toml" | head -1)"
  [ -d "services/$service/service" ] || continue
  cargo build -q --target wasm32-unknown-unknown --profile wasm -p "$package"
  mkdir -p "services/$service/service/web"
  cp "target/wasm32-unknown-unknown/wasm/${package//-/_}.wasm" "services/$service/service/web/app.wasm"
  echo "$package -> services/$service/service/web/app.wasm"
done
