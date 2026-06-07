#!/usr/bin/env bash
# cc-bench.sh - A/B idle-load comparison of coolercontrold: tokio (default) vs compio-rt.
#
# Builds each flavor in turn, runs it under identical conditions, measures it with
# measure-daemon.py, and prints a side-by-side delta. Paths resolve relative to this script, so
# it can live in scripts/ and find the daemon crate at ../coolercontrold/daemon.
#
# Usage:  ./cc-bench.sh [window_sec] [settle_sec] [interval_sec]
#         defaults: window=180 settle=30 interval=2
#
# Before running:
#   * Stop any other coolercontrold (systemd service or manual) to avoid a port conflict.
#   * Close the web UI and Qt app so you measure the idle poll loop, not SSE traffic.
#   * For stable numbers pin the CPU governor on both runs, e.g.:
#       sudo cpupower frequency-set -g performance
#   * On the SBC, confirm the io_uring path is active (else compio falls back to polling):
#       cat /proc/sys/kernel/io_uring_disabled   # 0 = active
set -euo pipefail

WINDOW="${1:-180}"
SETTLE="${2:-30}"
INTERVAL="${3:-2}"

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MEASURE="$HERE/measure-daemon.py"
DAEMON_CRATE="$HERE/../coolercontrold/daemon"
BIN="$HERE/../coolercontrold/target/release/coolercontrold"

command -v cargo >/dev/null || { echo "cargo not found" >&2; exit 1; }
command -v python3 >/dev/null || { echo "python3 not found" >&2; exit 1; }
[[ -f "$MEASURE" ]] || { echo "measure-daemon.py not found next to this script" >&2; exit 1; }
if pgrep -x coolercontrold >/dev/null; then
  echo "coolercontrold is already running; stop it first (this script runs its own)." >&2
  exit 1
fi

echo "priming sudo (needed to run the daemon) ..."
sudo -v

# Runs the current target/release/coolercontrold, measures it, stops it.
# Prints "core wakeups rss" on stdout; progress goes to stderr.
run_one() {
  local label="$1"
  # Never stack a second daemon on a live one (port + hardware clash).
  if pgrep -x coolercontrold >/dev/null; then
    echo "  $label: a coolercontrold is still running; refusing to start another." >&2
    echo "0 0 0"
    return
  fi
  sudo "$BIN" >"/tmp/cc-bench-$label.log" 2>&1 &
  local sudopid=$! pid="" i=0
  while [[ -z "$pid" && $i -lt 100 ]]; do
    sleep 0.2
    # sudo use_pty makes the daemon a grandchild of $sudopid, not a direct child; match by name.
    pid="$(pgrep -x coolercontrold | head -n1 || true)"
    i=$((i + 1))
  done
  if [[ -z "$pid" ]]; then
    echo "  $label failed to start (see /tmp/cc-bench-$label.log)" >&2
    sudo kill "$sudopid" 2>/dev/null || true
    echo "0 0 0"
    return
  fi
  echo "  $label pid=$pid: settle ${SETTLE}s, measure ${WINDOW}s ..." >&2
  sleep "$SETTLE"
  local s
  s="$(python3 "$MEASURE" -p "$pid" -i "$INTERVAL" -d "$WINDOW" | tail -n1)"
  sudo kill "$pid" 2>/dev/null || true
  for _ in $(seq 1 25); do sudo kill -0 "$pid" 2>/dev/null || break; sleep 0.2; done
  sudo kill -9 "$pid" 2>/dev/null || true

  local core wake rss
  core="$(sed -nE 's/.* ([0-9.]+)% of one core.*/\1/p' <<<"$s")"
  wake="$(sed -nE 's/.* ([0-9.]+) wakeups\/s.*/\1/p' <<<"$s")"
  rss="$(sed -nE 's/.*peak RSS ([0-9.]+) MB.*/\1/p' <<<"$s")"
  echo "${core:-0} ${wake:-0} ${rss:-0}"
}

echo ">> building tokio (default) ..."
(cd "$DAEMON_CRATE" && cargo build --release >/dev/null)
read -r T_CORE T_WAKE T_RSS < <(run_one tokio)

echo ">> building compio-rt ..."
(cd "$DAEMON_CRATE" && cargo build --release --features compio-rt >/dev/null)
read -r C_CORE C_WAKE C_RSS < <(run_one compio)

delta() {
  awk -v a="$1" -v b="$2" 'BEGIN { if (a + 0 == 0) print "n/a"; else printf "%+.0f%%", (b - a) / a * 100 }'
}

echo
printf '%-10s %12s %12s %10s\n' build cpu/core% wakeups/s rss_mb
printf '%-10s %12s %12s %10s\n' tokio "$T_CORE" "$T_WAKE" "$T_RSS"
printf '%-10s %12s %12s %10s\n' compio "$C_CORE" "$C_WAKE" "$C_RSS"
printf '%-10s %12s %12s %10s\n' delta \
  "$(delta "$T_CORE" "$C_CORE")" "$(delta "$T_WAKE" "$C_WAKE")" "$(delta "$T_RSS" "$C_RSS")"
echo
echo "negative delta = compio lower (better). window=${WINDOW}s settle=${SETTLE}s interval=${INTERVAL}s"
echo "full logs: /tmp/cc-bench-tokio.log  /tmp/cc-bench-compio.log"
