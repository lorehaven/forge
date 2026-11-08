#!/usr/bin/env bash
# Install and enable everything Gauge reads on a Void Linux host (see plans/gauge-smartmon-hosts.md and
# docs/services/gauge/metrics.md). The Void twin of gauge-host-setup-arch.sh, with runit instead of systemd:
#
#   * node_exporter on :9100 (runit service `node_exporter`), with the textfile collector (and the drm collector
#     when an AMD GPU is present). Enough on its own for a `kind = "node"` target.
#   * smartmontools + smartmon.sh: SMART disk health (HDDs, SATA SSDs, the NVMe verdict).
#   * nvme-cli + nvme_metrics.py, when the host has NVMe drives: wear, spare, hours, errors.
#   Both run every 5 minutes from one runit service, `smartmon`.
#
# PCP is not offered: Void does not package it, so a Void host is a `kind = "node"` target.
#
# Usage:  sudo bash helpers/gauge-host-setup-void.sh [--no-nvme]
#
# Safe to run again: existing node_exporter settings in /etc/sv/node_exporter/conf are kept, files are
# rewritten. The two scripts are downloaded from prometheus-community's node-exporter-textfile-collector-scripts
# and run as root: their sha256 is printed - read them (/usr/local/bin/...) if this is the first time.

set -euo pipefail

NVME=auto
for arg in "$@"; do
  case "$arg" in
    --no-nvme) NVME=0 ;;
    --pcp)
      echo "--pcp: Void does not package PCP; set the host up as a node target" >&2
      exit 2
      ;;
    -h | --help)
      sed -n '2,/^$/p' "$0" | sed -e '/^$/d' -e 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "unknown option: $arg (try --help)" >&2
      exit 2
      ;;
  esac
done

if [ "$(id -u)" -ne 0 ]; then
  echo "run as root: sudo bash $0 $*" >&2
  exit 1
fi
if ! command -v xbps-install >/dev/null; then
  echo "this is the Void script (no xbps-install here)" >&2
  exit 1
fi

DIR=/var/lib/node_exporter/textfile
SV=/etc/sv/node_exporter
CONF=$SV/conf
SCRIPTS=https://raw.githubusercontent.com/prometheus-community/node-exporter-textfile-collector-scripts/master

step() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }
note() { printf '    %s\n' "$*"; }

# ── What the host has ────────────────────────────────────────────────────────────────────────────

has_nvme=0
if [ "$NVME" != 0 ] && compgen -G '/sys/class/nvme/nvme*' >/dev/null; then
  has_nvme=1
fi
has_amd=0
for vendor in /sys/class/drm/card*/device/vendor; do
  [ -r "$vendor" ] && [ "$(cat "$vendor")" = 0x1002 ] && has_amd=1
done

step "Host: NVMe=$has_nvme AMD GPU=$has_amd"

# ── Packages ─────────────────────────────────────────────────────────────────────────────────────

# The first of the names the repositories have (package names drift).
available() {
  local name
  for name in "$@"; do
    if xbps-query -R "$name" >/dev/null 2>&1; then
      echo "$name"
      return
    fi
  done
}

step "Packages"
xbps-install -S >/dev/null
packages=(node_exporter smartmontools bash curl)
if [ "$has_nvme" = 1 ]; then
  python_client="$(available python3-prometheus_client python3-prometheus-client)"
  if [ -z "$python_client" ]; then
    echo "no Python prometheus_client package found: xbps-query -Rs prometheus" >&2
    exit 1
  fi
  packages+=(nvme-cli python3 "$python_client")
fi
xbps-install -y "${packages[@]}"

# ── node_exporter ────────────────────────────────────────────────────────────────────────────────

step "node_exporter flags ($CONF)"
if ! grep -q 'ARGS' "$SV/run" 2>/dev/null; then
  echo "$SV/run does not pass \$ARGS any more: read it and add the flags where it takes them" >&2
  exit 1
fi
current=""
if [ -f "$CONF" ]; then
  # shellcheck disable=SC1090
  current="$(cd "$SV" && . ./conf && printf '%s' "${ARGS:-}")"
fi
# A textfile directory node_exporter is already given is the one the scripts write to.
for flag in $current; do
  case "$flag" in
    --collector.textfile.directory=*) DIR="${flag#*=}" ;;
  esac
done
install -d -m 755 "$DIR"
args="$current"
add_flag() {
  case " $args " in
    *" $1 "* | *" ${1%%=*}="*) ;;
    *) args="${args:+$args }$1" ;;
  esac
}
add_flag "--collector.textfile.directory=$DIR"
[ "$has_amd" = 1 ] && add_flag "--collector.drm"
if [ "$args" != "$current" ]; then
  [ -f "$CONF" ] && cp -p "$CONF" "$CONF.bak"
  if [ -f "$CONF" ] && grep -q '^ARGS=' "$CONF"; then
    sed -i "s|^ARGS=.*|ARGS=\"$args\"|" "$CONF"
  else
    printf 'ARGS="%s"\n' "$args" >>"$CONF"
  fi
  note "ARGS=\"$args\"${current:+ (was \"$current\"; old file in $CONF.bak)}"
else
  note "already set: \"$args\""
fi
note "textfile directory: $DIR"

# ── Scripts ──────────────────────────────────────────────────────────────────────────────────────

step "Textfile collector scripts"
fetch() {
  local name="$1" tmp
  tmp="$(mktemp)"
  curl -fsSL -o "$tmp" "$SCRIPTS/$name"
  install -m 755 "$tmp" "/usr/local/bin/$name"
  rm -f "$tmp"
  note "$name  sha256 $(sha256sum "/usr/local/bin/$name" | cut -d' ' -f1)"
}
fetch smartmon.sh
if [ "$has_nvme" = 1 ]; then
  fetch nvme_metrics.py
else
  rm -f /usr/local/bin/nvme_metrics.py
fi

# ── runit services ───────────────────────────────────────────────────────────────────────────────

step "runit services"
install -d /etc/sv/smartmon
cat >/etc/sv/smartmon/run <<EOF
#!/bin/sh
# Written by helpers/gauge-host-setup-void.sh: SMART (and NVMe, when the script is installed) for node_exporter's
# textfile collector, every 5 minutes. A temp file and mv: node_exporter never reads a half-written file.
exec 2>&1
dir=$DIR
while :; do
  /usr/local/bin/smartmon.sh > "\$dir/smartmon.prom.\$\$" && mv "\$dir/smartmon.prom.\$\$" "\$dir/smartmon.prom"
  if [ -x /usr/local/bin/nvme_metrics.py ]; then
    /usr/local/bin/nvme_metrics.py > "\$dir/nvme.prom.\$\$" && mv "\$dir/nvme.prom.\$\$" "\$dir/nvme.prom"
  fi
  rm -f "\$dir"/*.prom.\$\$
  sleep 300
done
EOF
chmod 755 /etc/sv/smartmon/run

for svc in node_exporter smartmon; do
  if [ -e "/var/service/$svc" ]; then
    sv restart "$svc" >/dev/null
    note "$svc: restarted"
  else
    ln -s "/etc/sv/$svc" /var/service/
    note "$svc: enabled"
  fi
done

# ── Check ────────────────────────────────────────────────────────────────────────────────────────

step "Check"
# runsvdir picks a new service up within a few seconds, and smartctl takes a moment per drive.
metrics=""
for _ in $(seq 20); do
  metrics="$(curl -fsS --max-time 5 http://localhost:9100/metrics 2>/dev/null || true)"
  grep -q '^smartmon_device_smart_healthy' <<<"$metrics" && break
  sleep 3
done
if [ -z "$metrics" ]; then
  note "node_exporter does not answer on :9100: sv status node_exporter"
else
  note "node_textfile_scrape_error $(grep -m1 '^node_textfile_scrape_error' <<<"$metrics" | awk '{print $2}') (0 = the directory is read)"
  note "drives with SMART: $(grep -c '^smartmon_device_smart_healthy' <<<"$metrics" || true)"
  [ "$has_nvme" = 1 ] && note "NVMe namespaces with nvme_metrics: $(grep -c '^nvme_critical_warning' <<<"$metrics" || true)"
  [ "$has_amd" = 1 ] && note "AMD GPU (drm): $(grep -c '^node_drm_gpu_busy_percent' <<<"$metrics" || true) card(s)"
fi
note "services: $(sv status node_exporter smartmon 2>/dev/null | cut -d: -f1-2 | tr '\n' ' ')"

for svc in ufw nftables iptables; do
  if [ -e "/var/service/$svc" ]; then
    note "$svc is enabled: let Gauge in on 9100/tcp"
  fi
done

ip="$(ip -4 route get 1.1.1.1 2>/dev/null | awk '{for (i = 1; i < NF; i++) if ($i == "src") print $(i + 1)}')"
step "Gauge target (homecloud overlays/forge/gauge/config.yaml.j2)"
printf '    kind = "node"\n    url = "http://%s:9100"\n' "${ip:-<host>}"
