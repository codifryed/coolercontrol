#!/usr/bin/env bash
# cc-bench.sh - A/B idle-load comparison of coolercontrold: tokio (default) vs compio-rt.
#
# Builds each flavor, runs it under identical conditions for REPS repetitions, measures each run
# with measure-daemon.py, and prints mean(sd) plus a per-thread-group wakeups breakdown
# (main poll loop / worker pool / sidecar). Paths resolve relative to this script.
#
# Usage:  ./cc-bench.sh [window_sec] [settle_sec] [interval_sec] [reps]
#         defaults: window=180 settle=30 interval=2 reps=3
#         Total measuring time is about reps * 2 * (settle + window), plus two builds.
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
REPS="${4:-3}"

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MEASURE="$HERE/measure-daemon.py"
DAEMON_CRATE="$HERE/../coolercontrold/daemon"
BIN="$HERE/../coolercontrold/target/release/coolercontrold"

command -v cargo >/dev/null || {
    echo "cargo not found" >&2
    exit 1
}
command -v python3 >/dev/null || {
    echo "python3 not found" >&2
    exit 1
}
[[ -f $MEASURE ]] || {
    echo "measure-daemon.py not found next to this script" >&2
    exit 1
}
if pgrep -x coolercontrold >/dev/null; then
    echo "coolercontrold is already running; stop it first (this script runs its own)." >&2
    exit 1
fi

echo "priming sudo (needed to run the daemon) ..."
sudo -v

# One measured run of the current target/release/coolercontrold: start, settle, measure, stop.
# Echoes "core wakeups rss main pool sidecar other" on stdout; progress goes to stderr.
run_one() {
    local label="$1"
    # Never stack a second daemon on a live one (port + hardware clash).
    if pgrep -x coolercontrold >/dev/null; then
        echo "  $label: a coolercontrold is still running; refusing to start another." >&2
        echo "0 0 0 0 0 0 0"
        return
    fi
    sudo "$BIN" >"/tmp/cc-bench-$label.log" 2>&1 &
    local sudopid=$! pid="" i=0
    while [[ -z $pid && $i -lt 100 ]]; do
        sleep 0.2
        # sudo use_pty makes the daemon a grandchild of $sudopid, not a direct child; match by name.
        pid="$(pgrep -x coolercontrold | head -n1 || true)"
        i=$((i + 1))
    done
    if [[ -z $pid ]]; then
        echo "  $label failed to start (see /tmp/cc-bench-$label.log)" >&2
        sudo kill "$sudopid" 2>/dev/null || true
        echo "0 0 0 0 0 0 0"
        return
    fi
    sleep "$SETTLE"
    local summary
    summary="$(python3 "$MEASURE" -p "$pid" -i "$INTERVAL" -d "$WINDOW" | grep '^SUMMARY' | tail -n1 || true)"
    sudo kill "$pid" 2>/dev/null || true
    for _ in $(seq 1 25); do
        sudo kill -0 "$pid" 2>/dev/null || break
        sleep 0.2
    done
    sudo kill -9 "$pid" 2>/dev/null || true

    # SUMMARY is "SUMMARY key=val ...". Emit the 7 metrics in a fixed order, 0 for any missing.
    awk '
    function g(k) { return (k in v) ? v[k] : 0 }
    { for (i = 1; i <= NF; i++) if (split($i, kv, "=") == 2) v[kv[1]] = kv[2] }
    END {
      printf "%s %s %s %s %s %s %s\n",
        g("core_pct"), g("wakeups"), g("rss_mb"), g("main"), g("pool"), g("sidecar"), g("other")
    }
  ' <<<"$summary"
}

# Runs REPS measured reps of the current binary; echoes one metrics line per rep to stdout.
bench_flavor() {
    local label="$1" i line
    for ((i = 1; i <= REPS; i++)); do
        echo "  [$label] rep $i/$REPS: settle ${SETTLE}s + measure ${WINDOW}s ..." >&2
        line="$(run_one "${label}-${i}")"
        echo "    -> wakeups=$(awk '{print $2}' <<<"$line") main=$(awk '{print $4}' <<<"$line")" \
            "pool=$(awk '{print $5}' <<<"$line") sidecar=$(awk '{print $6}' <<<"$line")" >&2
        echo "$line"
    done
}

# stdin: rows of 7 numbers (failed runs have wakeups=0 and are skipped).
# stdout: line 1 = column means, line 2 = column sample standard deviations.
agg() {
    awk '
    ($2 + 0) == 0 { next }
    { for (i = 1; i <= NF; i++) { sum[i] += $i; sq[i] += $i * $i }; if (NF > nf) nf = NF; rows++ }
    END {
      if (rows == 0) { print "0 0 0 0 0 0 0"; print "0 0 0 0 0 0 0"; exit }
      for (i = 1; i <= nf; i++) printf "%.2f%s", sum[i] / rows, (i < nf ? OFS : ORS)
      for (i = 1; i <= nf; i++) {
        m = sum[i] / rows
        var = (rows > 1) ? (sq[i] - m * m * rows) / (rows - 1) : 0
        printf "%.2f%s", (var > 0 ? sqrt(var) : 0), (i < nf ? OFS : ORS)
      }
    }'
}

delta() {
    awk -v a="$1" -v b="$2" 'BEGIN { if (a + 0 == 0) print "n/a"; else printf "%+.0f%%", (b - a) / a * 100 }'
}

est=$((REPS * 2 * (SETTLE + WINDOW)))
echo ">> reps=$REPS window=${WINDOW}s settle=${SETTLE}s: measuring ~$((est / 60)) min plus builds" >&2

echo ">> building tokio (default) ..."
(cd "$DAEMON_CRATE" && cargo build --release >/dev/null)
T_OUT="$(bench_flavor tokio)"

echo ">> building compio-rt ..."
(cd "$DAEMON_CRATE" && cargo build --release --features compio-rt >/dev/null)
C_OUT="$(bench_flavor compio)"

mapfile -t TA < <(printf '%s\n' "$T_OUT" | agg)
mapfile -t CA < <(printf '%s\n' "$C_OUT" | agg)
read -ra TM <<<"${TA[0]}"
read -ra TSD <<<"${TA[1]}"
read -ra CM <<<"${CA[0]}"
read -ra CSD <<<"${CA[1]}"
# column order: 0 core  1 wakeups  2 rss  3 main  4 pool  5 sidecar  6 other

ms() { printf '%s(%s)' "$1" "$2"; } # mean(sd)

echo
printf '%-8s %-13s %-14s %-8s %-8s %-9s %-7s\n' build 'cpu/core%' 'wakeups/s' main pool sidecar rss_mb
printf '%-8s %-13s %-14s %-8s %-8s %-9s %-7s\n' tokio \
    "$(ms "${TM[0]}" "${TSD[0]}")" "$(ms "${TM[1]}" "${TSD[1]}")" "${TM[3]}" "${TM[4]}" "${TM[5]}" "${TM[2]}"
printf '%-8s %-13s %-14s %-8s %-8s %-9s %-7s\n' compio \
    "$(ms "${CM[0]}" "${CSD[0]}")" "$(ms "${CM[1]}" "${CSD[1]}")" "${CM[3]}" "${CM[4]}" "${CM[5]}" "${CM[2]}"
printf '%-8s %-13s %-14s %-8s %-8s %-9s %-7s\n' delta \
    "" "$(delta "${TM[1]}" "${CM[1]}")" "$(delta "${TM[3]}" "${CM[3]}")" \
    "$(delta "${TM[4]}" "${CM[4]}")" "$(delta "${TM[5]}" "${CM[5]}")" ""

echo
echo "values are mean(sd) over n=$REPS reps/build. wakeups/s = voluntary ctxt switches (lower is"
echo "better). main=poll loop, pool=blocking/io-wq workers, sidecar=zbus/tonic host."
echo "The tokio total includes the sidecar; if tokio stays the default the sidecar would be dropped,"
echo "so for a migrate/no-migrate call compare compio against tokio's (wakeups - sidecar)."
echo "window=${WINDOW}s settle=${SETTLE}s interval=${INTERVAL}s"
echo "full logs: /tmp/cc-bench-tokio-*.log  /tmp/cc-bench-compio-*.log"
