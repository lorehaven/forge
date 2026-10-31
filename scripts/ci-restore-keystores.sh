#!/bin/sh
# Puts the Android release keys back where the builds look for them. The keystores are not in git (every service
# ignores `*.keystore`), so a conveyor job gets each one as a base64 secret named <SERVICE>_KEYSTORE_B64 and writes it to
# services/<service>/release.keystore. A service whose secret is not set is left alone (its build will say it has no key).
# The passwords are the secrets QUENCH_KEYSTORE_PASSWORD and QUENCH_KEY_PASSWORD, read by the build as they are.
set -eu
for service in hoist daybook forklift; do
  var=$(printf '%s' "$service" | tr '[:lower:]' '[:upper:]')_KEYSTORE_B64
  eval "value=\${$var:-}"
  if [ -n "$value" ]; then
    printf '%s' "$value" | base64 -d > "services/$service/release.keystore"
    chmod 600 "services/$service/release.keystore"
    echo "restored the release key of $service"
  fi
done
