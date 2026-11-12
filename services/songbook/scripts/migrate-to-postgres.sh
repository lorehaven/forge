#!/usr/bin/env bash
# Moves a Songbook 0.1 library (SQLite: meta/library.db and meta/user.db) into the Postgres of Songbook 0.2.
#
#   MUSIC_HOME=/mnt/dev/music DATABASE_URL=postgres://... SONGBOOK_BIN=./songbook-service ./migrate-to-postgres.sh
#
# What it does: checks the files, copies them to <MUSIC_HOME>/meta/sqlite-backup-<time>/, runs the move as a dry run (it copies
# and checks every table, then rolls back), asks, then runs it for real. The SQLite files are only read, and the move is one
# transaction: it either copies every table and finds the row counts equal, or changes nothing.
#
# Before: stop the old (0.1) service, which holds the SQLite files; install the songbook tables with Foundry
#   foundry apply --catalog services/foundry/service/migrations --database-url "$DATABASE_URL" --install songbook
# After: start the 0.2 service with DATABASE_URL (and DB_SCHEMA if it is not `songbook`).
#
# Environment
#   MUSIC_HOME      the music home (default: .)
#   DATABASE_URL    the Postgres to write to (required)
#   DB_SCHEMA       the schema Foundry installed the module in (default: songbook)
#   SONGBOOK_BIN    the songbook-service binary of 0.2 (default: songbook-service on the PATH)
#   SONGBOOK_IMAGE  or: a songbook image to run it from (docker run --network host), when there is no binary
# Options: --yes (no question), --replace (empty the songbook tables first), --no-backup, --dry-run (stop after the dry run)
set -euo pipefail

home="${MUSIC_HOME:-.}"
yes=0 replace=0 backup=1 stop_after_dry=0
for arg in "$@"; do
  case "$arg" in
    --yes) yes=1 ;;
    --replace) replace=1 ;;
    --no-backup) backup=0 ;;
    --dry-run) stop_after_dry=1 ;;
    -h|--help) sed -n '2,/^set -e/p' "$0" | sed 's/^# \{0,1\}//; /^set -e/d'; exit 0 ;;
    *) echo "unknown option: $arg (see --help)" >&2; exit 2 ;;
  esac
done

: "${DATABASE_URL:?set DATABASE_URL to the Postgres to write to}"
library="$home/meta/library.db"
[ -f "$library" ] || { echo "no $library: set MUSIC_HOME to the music home of the 0.1 service" >&2; exit 1; }
[ -f "$home/meta/user.db" ] || echo "note: $home/meta/user.db is missing, so there are no playlists, plays or resume points to move"

if [ "$backup" = 1 ]; then
  dest="$home/meta/sqlite-backup-$(date +%Y%m%d-%H%M%S)"
  mkdir -p "$dest"
  cp -p "$library" "$dest/"
  [ -f "$home/meta/user.db" ] && cp -p "$home/meta/user.db" "$dest/"
  echo "backup: $dest"
fi

extra=()
[ "$replace" = 1 ] && extra+=(--replace)

migrate() {
  if [ -n "${SONGBOOK_IMAGE:-}" ]; then
    docker run --rm --network host -v "$(cd "$home" && pwd)":/music \
      -e DATABASE_URL -e DB_SCHEMA="${DB_SCHEMA:-songbook}" "$SONGBOOK_IMAGE" \
      ./service migrate-sqlite --home /music "${extra[@]}" "$@"
  else
    "${SONGBOOK_BIN:-songbook-service}" migrate-sqlite --home "$home" "${extra[@]}" "$@"
  fi
}

echo "== dry run =="
migrate --dry-run
[ "$stop_after_dry" = 1 ] && exit 0

if [ "$yes" != 1 ]; then
  printf 'Copy it for real? [y/N] '
  read -r answer
  case "$answer" in y|Y|yes) ;; *) echo "stopped; nothing was changed"; exit 1 ;; esac
fi

echo "== copy =="
migrate
echo "done. Start Songbook 0.2 with DATABASE_URL; keep the SQLite files until you have checked the library."
