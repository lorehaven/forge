#!/usr/bin/env bash
# Copies anvil's docs theme (the one `anvil docs build` stages) into docs/, so the GitHub Pages build looks the same.
# Anvil leaves files a book already has alone, so these are used by both builds. Run after the theme changes.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
src="${1:-$here/cli/anvil/assets/docs-theme}"
mkdir -p "$here/docs/_sass/custom" "$here/docs/_includes"
{ cat "$src/forge-tokens.css"; echo; cat "$src/forge.scss"; } > "$here/docs/_sass/custom/custom.scss"
for f in head_custom.html header_custom.html title.html nav_footer_custom.html; do
  cp "$src/$f" "$here/docs/_includes/$f"
done
echo "synced docs theme from $src"
