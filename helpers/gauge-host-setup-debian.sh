#!/usr/bin/env bash
# Install and enable everything Gauge reads on a Debian, Ubuntu or Raspberry Pi OS host (see
# plans/gauge-smartmon-hosts.md and docs/services/gauge/metrics.md). The Debian twin of gauge-host-setup-arch.sh:
#
#   * prometheus-node-exporter on :9100. Debian's already reads the textfile directory
#     /var/lib/prometheus/node-exporter; the drm collector is added when an AMD GPU is present. Enough on its own
#     for a `kind = "node"` target.
#   * smartmontools + the SMART script of prometheus-node-exporter-collectors on its packaged timer.
#   * nvme-cli + its NVMe script, when the host has NVMe drives: wear, spare, hours, errors.
#   * with --pcp: PCP's pmcd and pmproxy on :44322 (plus the nvidia/amdgpu PMDAs when such a GPU is present), for
#     a `kind = "pcp"` target. Such a target still needs node_exporter as its `also` for sensors and SMART.
#
# Gauge reads `smartmon_device_smart_healthy` and `nvme_critical_warning` / `nvme_percentage_used_ratio`. Releases
# ship different versions of the scripts; when the packaged one does not give those names (or there is no packaged
# timer), its timer is disabled and prometheus-community's current script runs on a timer of this script's own.
#
# Usage:  sudo bash helpers/gauge-host-setup-debian.sh [--pcp] [--no-nvme]
#
# Safe to run again. SD cards (a Raspberry Pi's mmcblk0) have no SMART: such a host shows no disk health.

set -euo pipefail

PCP=0
NVME=auto
for arg in "$@"; do
  case "$arg" in
    --pcp) PCP=1 ;;
    --no-nvme) NVME=0 ;;
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
if ! command -v apt-get >/dev/null; then
  echo "this is the Debian script (no apt-get here)" >&2
  exit 1
fi

DIR=/var/lib/prometheus/node-exporter
CONF=/etc/default/prometheus-node-exporter
SCRIPTS=https://raw.githubusercontent.com/prometheus-community/node-exporter-textfile-collector-scripts/master
export DEBIAN_FRONTEND=noninteractive

step() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }
note() { printf '    %s\n' "$*"; }

# ── What the host has ────────────────────────────────────────────────────────────────────────────

has_nvme=0
if [ "$NVME" != 0 ] && compgen -G '/sys/class/nvme/nvme*' >/dev/null; then
  has_nvme=1
fi
has_amd=0 has_nvidia=0
for vendor in /sys/class/drm/card*/device/vendor; do
  [ -r "$vendor" ] || continue
  case "$(cat "$vendor")" in
    0x1002) has_amd=1 ;;
    0x10de) has_nvidia=1 ;;
  esac
done

step "Host: NVMe=$has_nvme AMD GPU=$has_amd NVIDIA GPU=$has_nvidia PCP=$PCP"

# ── Packages ─────────────────────────────────────────────────────────────────────────────────────

step "Packages"
packages=(prometheus-node-exporter prometheus-node-exporter-collectors smartmontools curl)
[ "$has_nvme" = 1 ] && packages+=(nvme-cli python3-prometheus-client)
[ "$PCP" = 1 ] && packages+=(pcp)
apt-get update -qq
apt-get install -y -qq --no-install-recommends "${packages[@]}"

# ── node_exporter ────────────────────────────────────────────────────────────────────────────────

step "node_exporter flags ($CONF)"
current=""
if [ -f "$CONF" ]; then
  # shellcheck disable=SC1090
  current="$(. "$CONF" && printf '%s' "${ARGS:-}")"
fi
# A textfile directory node_exporter is given explicitly is the one the scripts write to.
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
  note "unchanged: ARGS=\"$args\""
fi
note "textfile directory: $DIR"
systemctl enable prometheus-node-exporter >/dev/null
systemctl restart prometheus-node-exporter

# ── SMART and NVMe exports ───────────────────────────────────────────────────────────────────────

# own_timer <name> <description> <script> <file in $DIR>: prometheus-community's current script on a timer of ours.
own_timer() {
  local name="$1" what="$2" script="$3" file="$4" tmp
  tmp="$(mktemp)"
  curl -fsSL -o "$tmp" "$SCRIPTS/$script"
  install -m 755 "$tmp" "/usr/local/bin/$script"
  rm -f "$tmp"
  note "$script  sha256 $(sha256sum "/usr/local/bin/$script" | cut -d' ' -f1) (runs as root: read it)"
  cat >"/etc/systemd/system/$name.service" <<EOF
[Unit]
Description=Export $what for node_exporter

[Service]
Type=oneshot
ExecStart=/bin/sh -c '/usr/local/bin/$script > $DIR/$file.tmp && mv $DIR/$file.tmp $DIR/$file'
EOF
  cat >"/etc/systemd/system/$name.timer" <<EOF
[Unit]
Description=Export $what every 5 minutes

[Timer]
OnBootSec=1min
OnUnitActiveSec=5min

[Install]
WantedBy=timers.target
EOF
  systemctl daemon-reload
  systemctl enable --now "$name.timer" >/dev/null
  systemctl start "$name.service" || note "$name: the first run failed: journalctl -u $name.service -n 30"
}

# The packaged timer whose name contains <word>, if the package has one.
packaged_timer() {
  systemctl list-unit-files --type=timer --no-legend 'prometheus-node-exporter-*.timer' 2>/dev/null |
    awk '{print $1}' | grep -m1 -i "$1" || true
}

# export_ <word> <own unit name> <description> <upstream script> <file> <metric Gauge needs>...
export_() {
  local word="$1" own="$2" what="$3" script="$4" file="$5"
  shift 5
  local timer metrics m missing=""
  timer="$(packaged_timer "$word")"
  if [ -n "$timer" ]; then
    systemctl enable --now "$timer" >/dev/null
    systemctl start "${timer%.timer}.service" || true
    sleep 2
    metrics="$(cat "$DIR"/*.prom 2>/dev/null || true)"
    for m in "$@"; do
      grep -q "^$m" <<<"$metrics" || missing="$missing $m"
    done
    if [ -z "$missing" ]; then
      note "$what: packaged $timer, enabled and run"
      return
    fi
    note "$what: packaged $timer gives no${missing}; replacing it with the upstream $script"
    systemctl disable --now "$timer" >/dev/null 2>&1 || true
  else
    note "$what: no packaged timer; using the upstream $script"
  fi
  own_timer "$own" "$what" "$script" "$file"
}

step "SMART and NVMe exports"
export_ smart smartmon "SMART metrics" smartmon.sh smartmon.prom smartmon_device_smart_healthy
if [ "$has_nvme" = 1 ]; then
  export_ nvme nvme-metrics "NVMe metrics" nvme_metrics.py nvme.prom nvme_critical_warning nvme_percentage_used_ratio
fi

# ── PCP ──────────────────────────────────────────────────────────────────────────────────────────

if [ "$PCP" = 1 ]; then
  step "PCP (pmcd, pmproxy :44322)"
  pmda() {
    local d="/var/lib/pcp/pmdas/$1"
    if [ ! -x "$d/Install" ]; then
      note "PMDA $1: not shipped by this pcp, skipped"
    elif pminfo "$1" >/dev/null 2>&1; then
      note "PMDA $1: already installed"
    elif (cd "$d" && ./Install </dev/null >/dev/null 2>&1); then
      note "PMDA $1: installed"
    else
      note "PMDA $1: Install failed: cd $d && sudo ./Install"
    fi
  }
  systemctl enable --now pmcd pmproxy >/dev/null
  [ "$has_nvidia" = 1 ] && pmda nvidia
  [ "$has_amd" = 1 ] && pmda amdgpu
fi

# ── Check ────────────────────────────────────────────────────────────────────────────────────────

step "Check"
sleep 2
metrics="$(curl -fsS --max-time 5 http://localhost:9100/metrics || true)"
if [ -z "$metrics" ]; then
  note "node_exporter does not answer on :9100: systemctl status prometheus-node-exporter"
else
  note "node_textfile_scrape_error $(grep -m1 '^node_textfile_scrape_error' <<<"$metrics" | awk '{print $2}') (0 = the directory is read)"
  note "drives with SMART: $(grep -c '^smartmon_device_smart_healthy' <<<"$metrics" || true)"
  [ "$has_nvme" = 1 ] && note "NVMe namespaces with nvme_metrics: $(grep -c '^nvme_critical_warning' <<<"$metrics" || true)"
  [ "$has_amd" = 1 ] && note "AMD GPU (drm): $(grep -c '^node_drm_gpu_busy_percent' <<<"$metrics" || true) card(s)"
  if ! compgen -G '/dev/sd?' >/dev/null && ! compgen -G '/dev/nvme*' >/dev/null; then
    note "no SATA/NVMe drive (an SD card has no SMART): no disk health for this host"
  fi
fi
if [ "$PCP" = 1 ]; then
  if curl -fsS --max-time 5 'http://localhost:44322/metrics?names=kernel.all.load' | grep -q kernel_all_load; then
    note "pmproxy answers on :44322"
  else
    note "pmproxy does not answer on :44322: systemctl status pmcd pmproxy"
  fi
fi

for svc in ufw firewalld; do
  if systemctl is-active -q "$svc" 2>/dev/null; then
    ports=9100/tcp
    [ "$PCP" = 1 ] && ports="$ports and 44322/tcp"
    note "$svc is active: let Gauge in on $ports"
  fi
done

ip="$(ip -4 route get 1.1.1.1 2>/dev/null | awk '{for (i = 1; i < NF; i++) if ($i == "src") print $(i + 1)}')"
step "Gauge target (homecloud overlays/forge/gauge/config.yaml.j2)"
if [ "$PCP" = 1 ]; then
  printf '    kind = "pcp"\n    url = "http://%s:44322"\n    also = "http://%s:9100"\n' "${ip:-<host>}" "${ip:-<host>}"
else
  printf '    kind = "node"\n    url = "http://%s:9100"\n' "${ip:-<host>}"
fi
