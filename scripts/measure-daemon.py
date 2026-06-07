#!/usr/bin/env python3
"""Measure CPU / wakeups / memory for the coolercontrold process (all threads).

Pure stdlib; reads /proc directly so it works on an SBC without psutil. The most telling metric for
the tokio-vs-compio comparison is wakeups/s (voluntary context switches): it counts how often the
runtime activates, where compio's io_uring batching should beat tokio's blocking-pool fs reads.
The summary also splits wakeups by thread group (main poll loop, worker pool, sidecar) so you can
see where they occur.

Usage:
    ./measure-daemon.py                 # 5s samples until Ctrl-C, then a summary
    ./measure-daemon.py -i 2 -d 300     # 2s samples for 5 minutes
    ./measure-daemon.py -p 1234 --children   # by pid, include children (e.g. liqctld)

Run with sudo only if /proc is hidden (hidepid); CPU and ctxt-switch fields are normally readable.
"""
import argparse
import glob
import os
import signal
import sys
import time

CLK = os.sysconf("SC_CLK_TCK")  # jiffies/sec (usually 100)
NCPU = os.cpu_count() or 1


def find_pid(name):
    for d in glob.glob("/proc/[0-9]*"):
        try:
            if open(f"{d}/comm").read().strip() == name:
                return int(os.path.basename(d))
        except OSError:
            pass
    return None


def children_of(pid):
    kids = {}
    for d in glob.glob("/proc/[0-9]*"):
        try:
            p = int(os.path.basename(d))
            ppid = int(open(f"{d}/stat").read().rsplit(") ", 1)[1].split()[1])
            kids.setdefault(ppid, []).append(p)
        except (OSError, ValueError, IndexError):
            pass
    out, stack = [], [pid]
    while stack:
        for c in kids.get(stack.pop(), []):
            out.append(c)
            stack.append(c)
    return out


def cpu_jiffies(pid):  # utime+stime across all threads of the process
    r = open(f"/proc/{pid}/stat").read().rsplit(") ", 1)[1].split()
    return int(r[11]) + int(r[12])  # fields 14 (utime) + 15 (stime)


# Wakeup-accounting buckets, keyed off the thread name (Name: in status, max 15 chars).
GROUPS = ("main", "pool", "sidecar", "other")


def thread_group(name):
    if name == "coolercontrold":
        return "main"  # current-thread runtime: poll loop (+ near-idle driver thread)
    if name == "cc-sidecar":
        return "sidecar"  # zbus/tonic host runtime
    if name.startswith("cc-wrk") or name.startswith("iou-wrk"):
        return "pool"  # tokio blocking pool / io_uring io-wq workers
    return "other"


def ctxt(
    pid,
):  # voluntary/nonvoluntary switches over all threads, plus voluntary split by group
    vol = nonvol = 0
    groups = dict.fromkeys(GROUPS, 0)
    for s in glob.glob(f"/proc/{pid}/task/*/status"):
        try:
            name = ""
            v = nv = 0
            for line in open(s):
                if line.startswith("Name:"):
                    name = line[5:].strip()
                elif line.startswith("voluntary_ctxt_switches:"):
                    v = int(line.split()[1])
                elif line.startswith("nonvoluntary_ctxt_switches:"):
                    nv = int(line.split()[1])
            vol += v
            nonvol += nv
            groups[thread_group(name)] += v
        except OSError:
            pass
    return vol, nonvol, groups


def rss_threads(pid):
    rss = thr = 0
    try:
        for line in open(f"/proc/{pid}/status"):
            if line.startswith("VmRSS:"):
                rss = int(line.split()[1])  # kB
            elif line.startswith("Threads:"):
                thr = int(line.split()[1])
    except OSError:
        pass
    return rss, thr


def sample(pids):
    cpu = v = nv = rss = thr = 0
    groups = dict.fromkeys(GROUPS, 0)
    for p in pids:
        try:
            cpu += cpu_jiffies(p)
            a, b, g = ctxt(p)
            v += a
            nv += b
            for k in GROUPS:
                groups[k] += g[k]
            r, t = rss_threads(p)
            rss += r
            thr += t
        except OSError:
            pass
    return cpu, v, nv, rss, thr, groups


def main():
    ap = argparse.ArgumentParser(
        description="Measure coolercontrold CPU/wakeups/memory."
    )
    ap.add_argument("-n", "--name", default="coolercontrold")
    ap.add_argument("-p", "--pid", type=int)
    ap.add_argument("-i", "--interval", type=float, default=5.0)
    ap.add_argument(
        "-d", "--duration", type=float, default=0.0, help="0 = until Ctrl-C"
    )
    ap.add_argument(
        "--children", action="store_true", help="include child procs (liqctld)"
    )
    a = ap.parse_args()

    pid = a.pid or find_pid(a.name)
    if not pid or not os.path.isdir(f"/proc/{pid}"):
        sys.exit(f"process not found: {a.pid or a.name} (try sudo if /proc is hidden)")

    def pids():
        ps = [pid, *(children_of(pid) if a.children else [])]
        return [p for p in ps if os.path.isdir(f"/proc/{p}")]

    print(
        f"pid={pid} name={a.name} cores={NCPU} interval={a.interval}s"
        f"{' +children' if a.children else ''}"
    )
    print(
        f"{'time':8} {'cpu/core%':>9} {'cpu/all%':>8} {'rss_mb':>7} "
        f"{'thr':>4} {'wakeups/s':>10} {'forced/s':>9}"
    )

    t0 = pt = time.monotonic()
    pc, pv, pnv, _, _, pg = sample(pids())
    cpu_sum = wake_sum = n = peak = 0
    group_sum = dict.fromkeys(GROUPS, 0.0)

    def summary(*_):
        wall = time.monotonic() - t0
        avg = cpu_sum / n if n else 0.0
        wake = wake_sum / n if n else 0.0
        grp = {k: (group_sum[k] / n if n else 0.0) for k in GROUPS}
        print("-" * 62)
        print(
            f"avg over {wall:.0f}s ({n} samples): {avg:.2f}% of one core "
            f"| {avg / NCPU:.2f}% of {NCPU} cores | {wake:.0f} wakeups/s "
            f"(main {grp['main']:.0f} pool {grp['pool']:.0f} sidecar {grp['sidecar']:.0f}) "
            f"| peak RSS {peak / 1024:.0f} MB"
        )
        # Machine-readable line for cc-bench. Groups partition the threads, so they sum to wakeups.
        print(
            f"SUMMARY core_pct={avg:.2f} wakeups={wake:.2f} rss_mb={peak / 1024:.0f} "
            f"main={grp['main']:.2f} pool={grp['pool']:.2f} "
            f"sidecar={grp['sidecar']:.2f} other={grp['other']:.2f}"
        )
        sys.exit(0)

    signal.signal(signal.SIGINT, summary)

    while True:
        time.sleep(a.interval)
        ps = pids()
        if not ps:
            print("process exited", file=sys.stderr)
            summary()
        now = time.monotonic()
        c, v, nv, rss, thr, g = sample(ps)
        dt = now - pt
        core = (c - pc) / CLK / dt * 100
        peak = max(peak, rss)
        print(
            f"{time.strftime('%H:%M:%S'):8} {core:9.2f} {core / NCPU:8.2f} "
            f"{rss / 1024:7.0f} {thr:4d} {(v - pv) / dt:10.0f} {(nv - pnv) / dt:9.0f}"
        )
        cpu_sum += core
        wake_sum += (v - pv) / dt
        for k in GROUPS:
            group_sum[k] += (g[k] - pg[k]) / dt
        n += 1
        pt, pc, pv, pnv, pg = now, c, v, nv, g
        if a.duration and now - t0 >= a.duration:
            summary()


if __name__ == "__main__":
    main()
