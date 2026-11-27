#!/usr/bin/env bash
# Restart PCP when pmproxy stops giving Gauge its metrics.
#
# pmproxy can stay "active (running)" while every /metrics request gets `400 Bad Request` (pmcd.log then fills with
# `DoFetch: no profile for ctxnum=...`). systemd sees nothing wrong, so Restart= never fires and the host stays
# "down" in Gauge until somebody restarts the service. This asks pmproxy the way Gauge does, once a minute:
#
#   1. three failed requests, 5 s apart  -> restart pmproxy
#   2. still failing                     -> restart pmcd and pmproxy (and pmlogger, if it is running)
#   3. still failing                     -> the unit fails (see the journal) and restarts pause for 15 minutes
#
# A disabled or masked pmproxy is left alone.
#
# Usage:  sudo bash helpers/gauge-pmproxy-watchdog.sh install     # copy to /usr/local/sbin, enable the timer
#         sudo bash helpers/gauge-pmproxy-watchdog.sh uninstall
#         bash helpers/gauge-pmproxy-watchdog.sh check [url]      # one request, changes nothing
#         gauge-pmproxy-watchdog run                              # what the timer does
#
# Log:    journalctl -u gauge-pmproxy-watchdog

set -euo pipefail

NAME=gauge-pmproxy-watchdog
BIN=${WATCHDOG_BIN:-/usr/local/sbin/$NAME}
UNITS=${WATCHDOG_UNIT_DIR:-/etc/systemd/system}
STATE=${WATCHDOG_STATE_DIR:-/run/$NAME}
URL=${WATCHDOG_URL:-http://localhost:44322}
ATTEMPTS=${WATCHDOG_ATTEMPTS:-3}
PAUSE=${WATCHDOG_PAUSE_SECS:-5}
SETTLE=${WATCHDOG_SETTLE_SECS:-10}
COOLDOWN=${WATCHDOG_COOLDOWN_SECS:-900}

why=""

# One request. Healthy is a 200 that carries the metric; otherwise `why` says what came back.
check() {
  local reply status
  if ! reply="$(curl -sS --max-time 8 -w '\n%{http_code}' "${1:-$URL}/metrics?names=kernel.all.load" 2>&1)"; then
    why="${reply%%$'\n'*}"
    return 1
  fi
  status="${reply##*$'\n'}"
  if [ "$status" != 200 ]; then
    why="HTTP $status"
    return 1
  fi
  if ! grep -q '^kernel_all_load' <<<"$reply"; then
    why="HTTP 200 without kernel_all_load"
    return 1
  fi
}

# Healthy if any of the attempts is.
healthy() {
  local n
  for ((n = 1; n <= ATTEMPTS; n++)); do
    check && return 0
    [ "$n" -lt "$ATTEMPTS" ] && sleep "$PAUSE"
  done
  return 1
}

recovered() {
  sleep "$SETTLE"
  healthy || return 1
  echo "recovered after restarting $1"
  rm -f "$STATE/gave-up"
}

run() {
  case "$(systemctl is-enabled pmproxy 2>/dev/null || true)" in
    disabled | masked*) return 0 ;;
  esac
  if healthy; then
    rm -f "$STATE/gave-up"
    return 0
  fi
  if [ -f "$STATE/gave-up" ] && [ $(($(date +%s) - $(cat "$STATE/gave-up"))) -lt "$COOLDOWN" ]; then
    echo "still failing ($why); restarts paused"
    return 0
  fi

  echo "pmproxy is failing ($why): restarting pmproxy"
  systemctl restart pmproxy || echo "restarting pmproxy failed"
  recovered pmproxy && return 0

  echo "still failing ($why): restarting pmcd, pmproxy and pmlogger"
  systemctl restart pmcd || echo "restarting pmcd failed"
  systemctl try-restart pmlogger || echo "restarting pmlogger failed"
  systemctl restart pmproxy || echo "restarting pmproxy failed"
  recovered "pmcd and pmproxy" && return 0

  mkdir -p "$STATE"
  date +%s >"$STATE/gave-up"
  echo "restarts did not help ($why): systemctl status pmcd pmproxy" >&2
  return 1
}

need_root() {
  if [ "$(id -u)" -ne 0 ] && [ -z "${WATCHDOG_UNIT_DIR:-}" ]; then
    echo "run as root: sudo bash $0 $1" >&2
    exit 1
  fi
}

install_() {
  need_root install
  command -v curl >/dev/null || { echo "curl is needed" >&2; exit 1; }
  systemctl cat pmproxy >/dev/null 2>&1 || { echo "no pmproxy unit on this host: nothing to watch" >&2; exit 1; }
  install -D -m 755 "${BASH_SOURCE[0]}" "$BIN"
  cat >"$UNITS/$NAME.service" <<EOF
[Unit]
Description=Restart PCP when pmproxy stops serving metrics
After=pmcd.service pmproxy.service

[Service]
Type=oneshot
ExecStart=$BIN run
TimeoutStartSec=5min
EOF
  cat >"$UNITS/$NAME.timer" <<EOF
[Unit]
Description=Check pmproxy every minute

[Timer]
OnBootSec=2min
OnUnitInactiveSec=1min
AccuracySec=5s

[Install]
WantedBy=timers.target
EOF
  systemctl daemon-reload
  systemctl enable "$NAME.timer" >/dev/null
  systemctl restart "$NAME.timer"
  echo "$NAME installed: journalctl -u $NAME"
}

uninstall() {
  need_root uninstall
  systemctl disable --now "$NAME.timer" >/dev/null 2>&1 || true
  rm -f "$UNITS/$NAME.service" "$UNITS/$NAME.timer" "$BIN"
  rm -rf "$STATE"
  systemctl daemon-reload
  echo "$NAME removed"
}

case "${1:-}" in
  run) run ;;
  check)
    if check "${2:-}"; then
      echo "ok: ${2:-$URL}"
    else
      echo "failing: ${2:-$URL}: $why"
      exit 1
    fi
    ;;
  install) install_ ;;
  uninstall) uninstall ;;
  -h | --help) sed -n '2,/^$/p' "$0" | sed -e '/^$/d' -e 's/^# \{0,1\}//' ;;
  *)
    echo "usage: $0 install|uninstall|check [url]|run (see --help)" >&2
    exit 2
    ;;
esac
