#!/bin/bash
# measure-overlay.sh: sample the resource use of running processes (spec §17.9 item 6)
# for docs/validation.md.
#
#   tools/measure-overlay.sh [--seconds N] [--label TEXT] PID [PID...]
#
# For each PID, over N seconds (default 60):
#   - PSS at the end, from /proc/PID/smaps_rollup (kB);
#   - CPU time used during the window (utime + stime from /proc/PID/stat), in ms and as
#     a percentage of one core;
#   - wakeups during the window: voluntary + involuntary context switches from
#     /proc/PID/status, summed over the process's threads (an upper bound on the
#     animation scheduler's wakeups, which share the process with Qt's own work);
#   - wakeups per second.
# It prints a Markdown table plus the host facts to record with it (kernel, CPU,
# Quickshell version). It only reads /proc; it never signals, traces or changes the
# processes, and writes nothing to disk.
#
# Not measured here: first-switch latency (chooser click to first new-species frame),
# which needs an in-scene timestamp; record it separately or mark it untested.
#
# Typical use: the overlay `qs` pid (codepet-overlay status / qs list) and the Omarchy
# shell pid, idle and active, with the plugin disabled and enabled.
set -euo pipefail

PROG=measure-overlay.sh
usage() {
    echo "usage: $PROG [--seconds N] [--label TEXT] PID [PID...]" >&2
    exit 2
}

SECONDS_WANTED=60
LABEL=""
pids=()
while [ "$#" -gt 0 ]; do
    case "$1" in
        --seconds) [ "$#" -ge 2 ] || usage; SECONDS_WANTED=$2; shift 2 ;;
        --label) [ "$#" -ge 2 ] || usage; LABEL=$2; shift 2 ;;
        -h|--help) sed -n '2,25p' "$0"; exit 0 ;;
        --*) usage ;;
        *) pids+=("$1"); shift ;;
    esac
done
[ "${#pids[@]}" -gt 0 ] || usage
[[ "$SECONDS_WANTED" =~ ^[1-9][0-9]{0,4}$ ]] || { echo "$PROG: --seconds must be 1..99999" >&2; exit 2; }
for pid in "${pids[@]}"; do
    [[ "$pid" =~ ^[1-9][0-9]*$ ]] || { echo "$PROG: not a pid: $pid" >&2; exit 2; }
    [ -r "/proc/$pid/stat" ] || { echo "$PROG: no readable process $pid" >&2; exit 1; }
done

TICKS=$(getconf CLK_TCK)

cpu_ticks() { # utime + stime; the comm field may hold spaces, so cut after ") "
    local rest
    rest=$(sed 's/^.*) //' "/proc/$1/stat")
    awk '{ print $12 + $13 }' <<<"$rest"
}
switches() { # voluntary + involuntary context switches over all threads
    local total=0 t v n
    for t in /proc/"$1"/task/*/status; do
        v=$(awk '/^voluntary_ctxt_switches:/ { print $2 }' "$t" 2>/dev/null || echo 0)
        n=$(awk '/^nonvoluntary_ctxt_switches:/ { print $2 }' "$t" 2>/dev/null || echo 0)
        total=$((total + ${v:-0} + ${n:-0}))
    done
    echo "$total"
}
pss_kb() {
    awk '/^Pss:/ { print $2; exit }' "/proc/$1/smaps_rollup" 2>/dev/null || echo "?"
}

declare -A cpu0 sw0
for pid in "${pids[@]}"; do
    cpu0[$pid]=$(cpu_ticks "$pid")
    sw0[$pid]=$(switches "$pid")
done
sleep "$SECONDS_WANTED"

echo "Host: $(uname -sr); CPU: $(awk -F': ' '/^model name/ { print $2; exit }' /proc/cpuinfo)"
if command -v qs >/dev/null 2>&1; then
    echo "Quickshell: $(qs --version 2>/dev/null | head -n 1)"
fi
echo "Window: ${SECONDS_WANTED} s${LABEL:+; $LABEL}"
echo
echo "| pid | command | PSS (kB) | CPU (ms) | CPU (% of one core) | wakeups | wakeups/s |"
echo "|---|---|---|---|---|---|---|"
for pid in "${pids[@]}"; do
    if [ ! -r "/proc/$pid/stat" ]; then
        echo "| $pid | (exited during the window) | ? | ? | ? | ? | ? |"
        continue
    fi
    cmd=$(tr '\0' ' ' <"/proc/$pid/cmdline" | cut -c1-60)
    cmd=${cmd//|//}
    dticks=$(( $(cpu_ticks "$pid") - ${cpu0[$pid]} ))
    dsw=$(( $(switches "$pid") - ${sw0[$pid]} ))
    ms=$(( dticks * 1000 / TICKS ))
    pct=$(awk -v ms="$ms" -v s="$SECONDS_WANTED" 'BEGIN { printf "%.2f", ms / (s * 10) }')
    rate=$(awk -v n="$dsw" -v s="$SECONDS_WANTED" 'BEGIN { printf "%.2f", n / s }')
    echo "| $pid | ${cmd% } | $(pss_kb "$pid") | $ms | $pct | $dsw | $rate |"
done
