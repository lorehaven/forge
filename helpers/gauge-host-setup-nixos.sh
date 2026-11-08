#!/usr/bin/env bash
# Set up everything Gauge reads on a NixOS host (see plans/gauge-smartmon-hosts.md and
# docs/services/gauge/metrics.md). The NixOS twin of gauge-host-setup-arch.sh. NixOS is declarative, so this
# installs nothing by itself: it writes a module, and `nixos-rebuild switch` makes it so.
#
#   * /etc/nixos/gauge/monitoring.nix: node_exporter on :9100 with the textfile collector (and the drm collector
#     when an AMD GPU is present), port opened in the firewall; smartmon.sh on a 5-minute timer (SMART: HDDs, SATA
#     SSDs, the NVMe verdict); nvme_metrics.py on a 5-minute timer when the host has NVMe drives (wear, spare,
#     hours, errors). Enough for a `kind = "node"` target.
#   * /etc/nixos/gauge/smartmon.sh and nvme_metrics.py: prometheus-community's node-exporter-textfile-collector-
#     scripts, kept next to the module (the build copies them into the store, so no fetch hash is needed). They run
#     as root: their sha256 is printed - read them if this is the first time.
#
# PCP is not offered (no NixOS module for pmcd/pmproxy), so a NixOS host is a `kind = "node"` target. NixOS's own
# services.prometheus.exporters.smartctl is not used: Gauge reads smartmon.sh's metric names, not smartctl_*.
#
# Usage:  sudo bash helpers/gauge-host-setup-nixos.sh [--no-nvme] [--import] [--switch]
#           --import  add ./gauge/monitoring.nix to the `imports` of /etc/nixos/configuration.nix
#           --switch  run `nixos-rebuild switch` afterwards, then check
#
# Without --import, add the module yourself: `imports = [ ./gauge/monitoring.nix ];` in configuration.nix, or the
# path in your flake's `modules`. Safe to run again: the files under /etc/nixos/gauge are rewritten.

set -euo pipefail

NVME=auto
IMPORT=0
SWITCH=0
for arg in "$@"; do
  case "$arg" in
    --no-nvme) NVME=0 ;;
    --import) IMPORT=1 ;;
    --switch) SWITCH=1 ;;
    --pcp)
      echo "--pcp: NixOS has no module for PCP; set the host up as a node target" >&2
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
if [ ! -e /etc/NIXOS ]; then
  echo "this is the NixOS script (no /etc/NIXOS here)" >&2
  exit 1
fi

NIXOS=/etc/nixos
OUT=$NIXOS/gauge
DIR=/var/lib/node_exporter/textfile
SCRIPTS=https://raw.githubusercontent.com/prometheus-community/node-exporter-textfile-collector-scripts/master

step() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }
note() { printf '    %s\n' "$*"; }

# ── What the host has ────────────────────────────────────────────────────────────────────────────

has_nvme=false
if [ "$NVME" != 0 ] && compgen -G '/sys/class/nvme/nvme*' >/dev/null; then
  has_nvme=true
fi
has_amd=false
for vendor in /sys/class/drm/card*/device/vendor; do
  [ -r "$vendor" ] && [ "$(cat "$vendor")" = 0x1002 ] && has_amd=true
done

step "Host: NVMe=$has_nvme AMD GPU=$has_amd"

# ── Scripts ──────────────────────────────────────────────────────────────────────────────────────

step "Textfile collector scripts ($OUT)"
install -d -m 755 "$OUT"
fetch() {
  local name="$1" tmp
  tmp="$(mktemp)"
  if command -v curl >/dev/null; then
    curl -fsSL -o "$tmp" "$SCRIPTS/$name"
  else
    # Always there on NixOS: prints the hash, then the store path of the download.
    cp "$(nix-prefetch-url --print-path "$SCRIPTS/$name" 2>/dev/null | tail -n1)" "$tmp"
  fi
  install -m 644 "$tmp" "$OUT/$name"
  rm -f "$tmp"
  note "$name  sha256 $(sha256sum "$OUT/$name" | cut -d' ' -f1)"
}
fetch smartmon.sh
if [ "$has_nvme" = true ]; then
  fetch nvme_metrics.py
else
  rm -f "$OUT/nvme_metrics.py"
fi

# ── The module ───────────────────────────────────────────────────────────────────────────────────

step "Module ($OUT/monitoring.nix)"
cat >"$OUT/monitoring.nix" <<EOF
# Written by helpers/gauge-host-setup-nixos.sh (forge): what Gauge reads on this host. Run the script again to
# change it (it detects NVMe drives and AMD GPUs), or edit the two settings below and rebuild.
{ pkgs, lib, ... }:

let
  nvme = $has_nvme;
  amdGpu = $has_amd;

  dir = "$DIR";

  # A script's output into dir, through a temp file and mv: node_exporter never reads a half-written file.
  export = name: file: command: path: {
    services.\${name} = {
      description = "Export \${name} metrics for node_exporter";
      serviceConfig.Type = "oneshot";
      inherit path;
      script = ''
        \${command} > \${dir}/\${file}.tmp
        mv \${dir}/\${file}.tmp \${dir}/\${file}
      '';
    };
    timers.\${name} = {
      wantedBy = [ "timers.target" ];
      timerConfig = {
        OnBootSec = "1min";
        OnUnitActiveSec = "5min";
      };
    };
  };

  smartmon = export "smartmon" "smartmon.prom" "\${pkgs.bash}/bin/bash \${./smartmon.sh}" (with pkgs; [
    smartmontools gawk coreutils gnugrep gnused findutils util-linux
  ]);

  nvmeMetrics = export "nvme-metrics" "nvme.prom"
    "\${pkgs.python3.withPackages (ps: [ ps.prometheus-client ])}/bin/python3 \${./nvme_metrics.py}"
    [ pkgs.nvme-cli ];
in
{
  services.prometheus.exporters.node = {
    enable = true;
    port = 9100;
    openFirewall = true;
    enabledCollectors = [ "textfile" ] ++ lib.optional amdGpu "drm";
    extraFlags = [ "--collector.textfile.directory=\${dir}" ];
  };

  environment.systemPackages = [ pkgs.smartmontools ] ++ lib.optional nvme pkgs.nvme-cli;

  systemd.tmpfiles.rules = [ "d \${dir} 0755 root root -" ];

  systemd.services = smartmon.services // lib.optionalAttrs nvme nvmeMetrics.services;
  systemd.timers = smartmon.timers // lib.optionalAttrs nvme nvmeMetrics.timers;
}
EOF
note "nvme = $has_nvme; amdGpu = $has_amd"

# ── Import ───────────────────────────────────────────────────────────────────────────────────────

step "Import"
config=$NIXOS/configuration.nix
if grep -qs 'gauge/monitoring.nix' "$config"; then
  note "configuration.nix already imports ./gauge/monitoring.nix"
elif [ -f "$NIXOS/flake.nix" ] && [ "$IMPORT" = 1 ]; then
  note "a flake configuration: add ./gauge/monitoring.nix to the system's modules (or to configuration.nix"
  note "imports) by hand - --import only edits a plain configuration.nix"
elif [ "$IMPORT" = 1 ]; then
  if grep -qE '^[[:space:]]*imports[[:space:]]*=[[:space:]]*\[' "$config"; then
    cp -p "$config" "$config.bak"
    sed -i -E '0,/^([[:space:]]*)imports[[:space:]]*=[[:space:]]*\[/s//&\n\1  .\/gauge\/monitoring.nix/' "$config"
    note "added ./gauge/monitoring.nix to imports (old file in $config.bak)"
  else
    echo "no \`imports = [\` line in $config: add \`imports = [ ./gauge/monitoring.nix ];\` by hand" >&2
    exit 1
  fi
else
  note "add to $config:   imports = [ ./gauge/monitoring.nix ];"
  note "(or to your flake's modules; or rerun with --import)"
fi

# ── Switch and check ─────────────────────────────────────────────────────────────────────────────

if [ "$SWITCH" = 1 ]; then
  step "nixos-rebuild switch"
  nixos-rebuild switch
  systemctl start smartmon.service || note "smartmon: the first run failed: journalctl -u smartmon.service -n 30"
  [ "$has_nvme" = true ] && { systemctl start nvme-metrics.service || note "nvme-metrics: the first run failed: journalctl -u nvme-metrics.service -n 30"; }

  step "Check"
  sleep 2
  if command -v curl >/dev/null; then
    metrics="$(curl -fsS --max-time 5 http://localhost:9100/metrics 2>/dev/null || true)"
  else
    metrics="$(nix-shell -p curl --run 'curl -fsS --max-time 5 http://localhost:9100/metrics' 2>/dev/null || true)"
  fi
  if [ -z "$metrics" ]; then
    note "node_exporter does not answer on :9100: systemctl status prometheus-node-exporter"
  else
    note "node_textfile_scrape_error $(grep -m1 '^node_textfile_scrape_error' <<<"$metrics" | awk '{print $2}') (0 = the directory is read)"
    note "drives with SMART: $(grep -c '^smartmon_device_smart_healthy' <<<"$metrics" || true)"
    [ "$has_nvme" = true ] && note "NVMe namespaces with nvme_metrics: $(grep -c '^nvme_critical_warning' <<<"$metrics" || true)"
    [ "$has_amd" = true ] && note "AMD GPU (drm): $(grep -c '^node_drm_gpu_busy_percent' <<<"$metrics" || true) card(s)"
  fi
else
  step "Next"
  note "sudo nixos-rebuild switch"
  note "curl -s localhost:9100/metrics | grep -E 'node_textfile_scrape_error|smartmon_device_smart_healthy'"
fi

ip="$(ip -4 route get 1.1.1.1 2>/dev/null | awk '{for (i = 1; i < NF; i++) if ($i == "src") print $(i + 1)}')"
step "Gauge target (homecloud overlays/forge/gauge/config.yaml.j2)"
printf '    kind = "node"\n    url = "http://%s:9100"\n' "${ip:-<host>}"
