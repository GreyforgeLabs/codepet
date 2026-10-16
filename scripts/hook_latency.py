#!/usr/bin/env python3
"""WP7 latency harness, using only stdlib and an already-built release binary.

Every row has 200 measured calls after 20 warmups by default. Baseline runs come
first; injected load is opt-in (--load python or stress-ng), never host-wide.
Each load group has at most two workers, a 60 s wall deadline, and cleanup even
on failure. Results and isolated synthetic state are retained under --out.
No client executable or client settings are used. Cold means missing state,
not a flushed OS page cache. Codex's bash -lc cases are explicitly unmeasured:
this harness has no qualified filesystem-isolated login-shell runner. Changing
HOME alone cannot prevent system profiles from reading the operator's files.
"""

import argparse
import contextlib
import datetime
import fcntl
import hashlib
import json
import math
import os
from pathlib import Path
import shlex
import shutil
import signal
import sqlite3
import subprocess
import sys
import time


REPO = Path(__file__).resolve().parent.parent
FIXTURES = {
    "claude-code": "04-pre-tool-bash.json",
    "codex": "03-post-tool-use.json",
    "grok-cli": "03-pre-tool-read.json",
    "opencode": "04-activity-permission-replied.json",
    "hermes": "02-turn-start.json",
}
CASES = ("off", "busy", "available", "cold", "missing-binary")
LOGIN_SHELL_UNMEASURED = (
    "bash -lc omitted: no qualified filesystem-isolated login-shell runner; "
    "HOME overrides cannot prevent startup scripts from reading real user files."
)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def cpu_ticks():
    ticks = [int(v) for v in Path("/proc/stat").read_text().splitlines()[0].split()[1:9]]
    return sum(ticks), ticks[3] + ticks[4]


def host():
    return {"loadavg": list(os.getloadavg()), "logical_cpus": os.cpu_count(),
            "allowed_cpus": len(os.sched_getaffinity(0)), "cpu_ticks": cpu_ticks()}


def stats(values):
    ordered = sorted(values)
    return {**{f"p{q}_ms": ordered[math.ceil(len(ordered) * q / 100) - 1]
               for q in (50, 90, 99)}, "max_ms": max(values), "runs": len(values)}


def snapshot(root):
    # Content + metadata, excluding read-only atime changes. Also catches new files.
    result = {}
    for p in sorted(root.rglob("*")):
        st = p.lstat()
        result[str(p.relative_to(root))] = [st.st_mode, st.st_size, st.st_mtime_ns,
                                           digest(p) if p.is_file() else None]
    return result


def checked(argv, env, cwd, data=None):
    r = subprocess.run(argv, input=data, capture_output=True, env=env, cwd=cwd, timeout=10)
    if r.returncode:
        raise RuntimeError(f"setup failed: {argv[1:]}: {r.stderr.decode(errors='replace')}")
    return r


def signal_group(process, sig):
    # A reaped leader does not imply its descendants have left the group.
    try:
        os.killpg(process.pid, sig)
    except ProcessLookupError:
        pass  # The entire group may have exited between observation and signal.


def timed_hook(argv, data, env, cwd):
    # communicate(timeout=...) can enter wait()'s exponential polling after a
    # silent shim closes its pipes early. Use blocking waitpid plus an independent
    # alarm so those sleeps cannot become part of the latency distribution.
    def expired(signum, frame):
        raise TimeoutError("hook exceeded the 2 s measurement safety deadline")

    previous = signal.signal(signal.SIGALRM, expired)
    process = None
    completed = False
    signal.setitimer(signal.ITIMER_REAL, 2)
    started = time.perf_counter_ns()
    try:
        process = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, env=env, cwd=cwd,
                                   start_new_session=True)
        stdout, stderr = process.communicate(data)
        elapsed = (time.perf_counter_ns() - started) / 1_000_000
        completed = True
        return process.returncode, stdout, stderr, elapsed
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, previous)
        if process is not None and not completed:
            signal_group(process, signal.SIGKILL)
            process.wait()
            for stream in (process.stdin, process.stdout, process.stderr):
                if stream is not None:
                    stream.close()


class Sandbox:
    def __init__(self, root, binary, case):
        self.root, self.binary, self.case = root, binary, case
        root.mkdir(mode=0o700)
        for name in ("home", "state", "data", "config", "cache", "runtime", "tmp"):
            (root / name).mkdir(mode=0o700)
        self.env = {"PATH": "/usr/bin:/bin", "LC_ALL": "C", "HOME": str(root / "home"),
                    "TMPDIR": str(root / "tmp"), "CODEPET_TEST_STATE": "1"}
        for var, name in (("STATE", "state"), ("DATA", "data"), ("CONFIG", "config"),
                          ("CACHE", "cache")):
            self.env[f"XDG_{var}_HOME"] = str(root / name)
        self.env["XDG_RUNTIME_DIR"] = str(root / "runtime")
        self.state = root / "state/codepet"
        self.shim = root / "data/codepet/bin/codepet-hook"
        self.shim.parent.mkdir(parents=True, mode=0o700)
        if case != "cold":
            checked([str(binary), "migrate"], self.env, root)
            for name in ("inbox", "clients"):
                (self.state / name).mkdir(mode=0o700, exist_ok=True)
        if case == "off":
            (self.state / "off").touch(mode=0o600)
        shim_binary = binary if case != "missing-binary" else root / "absent-codepet"
        for path in (self.state, shim_binary):
            if "'" in str(path) or "\n" in str(path):
                raise ValueError("shim paths cannot contain single quotes or newlines")
        template = (REPO / "docs/v0.3/contracts/shim.sh.template").read_text()
        self.shim.write_text(template.replace("@CODEPET_STATE_DIR@", str(self.state), 1)
                             .replace("@CODEPET_BIN@", str(shim_binary), 1))
        self.shim.chmod(0o755)
        self.lock = None
        if case in ("busy", "available"):
            self.lock = open(self.state / "fold.lock", "ab")
            os.chmod(self.state / "fold.lock", 0o600)
        if case == "busy":
            fcntl.flock(self.lock, fcntl.LOCK_EX | fcntl.LOCK_NB)

    def command(self, client, mode):
        if mode == "shim":
            return [str(self.shim), client]
        command = f"{shlex.quote(str(self.shim))} {client} >/dev/null 2>&1 || true"
        if mode == "bash-lc":
            raise ValueError(LOGIN_SHELL_UNMEASURED)
        if client in ("opencode", "hermes"):
            return [str(self.shim), client]
        return ["/bin/sh", "-c", command]

    def records(self):
        return list((self.state / "inbox").glob("*.ev"))

    def settle(self):
        deadline = time.monotonic() + 6
        while time.monotonic() < deadline:
            try:
                fcntl.flock(self.lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                time.sleep(0.001)
                continue
            try:
                if not self.records():
                    return
            finally:
                fcntl.flock(self.lock, fcntl.LOCK_UN)
            time.sleep(0.001)
        raise RuntimeError("fold did not finish within 6 s")

    def event_count(self):
        with sqlite3.connect(f"file:{self.state}/codepet.sqlite3?mode=ro", uri=True) as db:
            return db.execute("SELECT COUNT(*) FROM agent_events").fetchone()[0]

    def close(self):
        if self.lock:
            self.lock.close()


@contextlib.contextmanager
def cpu_load(kind, workers, env, cwd):
    processes = []
    try:
        if kind == "python":
            program = ("import time,resource; resource.setrlimit(resource.RLIMIT_CPU,(60,60)); "
                       "end=time.monotonic()+60\nwhile time.monotonic()<end: sum(i*i for i in range(10000))")
            for _ in range(workers):
                processes.append(subprocess.Popen([sys.executable, "-I", "-c", program],
                    env=env, cwd=cwd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                    start_new_session=True))
        elif kind == "stress-ng":
            processes.append(subprocess.Popen([shutil.which("stress-ng"), "--cpu", str(workers),
                "--timeout", "60s", "--quiet"], env=env, cwd=cwd,
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True))
        if processes:
            time.sleep(0.1)
        yield processes
    finally:
        for process in processes:
            signal_group(process, signal.SIGTERM)
        for process in processes:
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                pass
            finally:
                # Kill surviving descendants even when their leader exited early.
                signal_group(process, signal.SIGKILL)
                process.wait()


def measure(args, phase, client, case, mode, index):
    if mode == "bash-lc":
        raise ValueError(LOGIN_SHELL_UNMEASURED)
    box = Sandbox(args.out / f"{index:03d}-{phase}-{client}-{case}-{mode}", args.bin, case)
    fixture_path = REPO / "crates/codepet-cli/tests/fixtures" / client / FIXTURES[client]
    fixture = json.loads(fixture_path.read_text())
    payload = json.loads(fixture["stdin"])
    env = dict(box.env, **fixture["env"])
    before = snapshot(box.root) if case in ("off", "cold", "missing-binary") else None
    values = []
    folded_before = box.event_count() if case == "available" else 0
    load_kind = args.load if phase == "loaded" else "none"
    try:
        with cpu_load(load_kind, args.workers, box.env, box.root) as workers:
            started = time.monotonic()
            observation_start = host()
            for i in range(args.warmup + args.runs):
                if time.monotonic() - started > 55 and workers:
                    raise RuntimeError("load group reached 55 s safety deadline; reduce --runs")
                if workers and any(p.poll() is not None for p in workers):
                    raise RuntimeError("CPU load worker exited before the group finished")
                # Unique payload digest without changing the representative event shape.
                payload["codepet_latency_sample"] = i
                data = (json.dumps(payload, separators=(",", ":")) + "\n").encode()
                if case == "available":
                    box.settle()
                    # Age only synthetic export mtime OUTSIDE timing. This avoids paying
                    # the fold's 250 ms export debounce while guaranteeing free-lock calls.
                    export = box.state / "bar-state.json"
                    if export.exists():
                        os.utime(export, (time.time() - 1, time.time() - 1))
                rc, stdout, stderr, elapsed = timed_hook(box.command(client, mode), data, env, box.root)
                if rc or stdout or stderr:
                    raise RuntimeError(f"non-silent hook: rc={rc}, "
                                       f"stdout={len(stdout)}, stderr={len(stderr)}")
                if i >= args.warmup:
                    values.append(elapsed)
                if case == "busy":
                    records = box.records()
                    if len(records) != 1:
                        raise RuntimeError(f"busy hook wrote {len(records)} records, expected one")
                    records[0].unlink()  # Only this synthetic fixture's measured event.
                elif case == "available":
                    box.settle()
            observation_end = host()
            if workers and any(p.poll() is not None for p in workers):
                raise RuntimeError("CPU load ended during measurement")
        if before is not None and snapshot(box.root) != before:
            raise RuntimeError(f"{case} changed isolated HOME/XDG state")
        folded = box.event_count() - folded_before if case == "available" else None
        if folded is not None and folded != args.runs + args.warmup:
            raise RuntimeError(f"folded {folded} events, expected {args.runs + args.warmup}")
        total = observation_end["cpu_ticks"][0] - observation_start["cpu_ticks"][0]
        idle = observation_end["cpu_ticks"][1] - observation_start["cpu_ticks"][1]
        utilization = 1 - idle / total if total else None
        idle_eligible = (phase == "baseline" and utilization is not None and utilization < .25
            and max(observation_start["loadavg"][0], observation_end["loadavg"][0])
            < observation_start["allowed_cpus"] * .5)
        summary = stats(values)
        budget = None
        if mode == "shim":
            if case == "off" and idle_eligible:
                budget = summary["p50_ms"] <= 3
            if case in ("busy", "available"):
                if idle_eligible:
                    budget = summary["p50_ms"] <= 10 and summary["p99_ms"] <= 20
                elif phase == "loaded":
                    budget = summary["p99_ms"] <= 40
        return {"phase": phase, "client": client, "case": case, "command": mode,
                **summary, "samples_ms": values, "host_start": observation_start,
                "host_end": observation_end, "host_cpu_busy_fraction": utilization,
                "idle_eligible": idle_eligible, "load_method": load_kind,
                "load_workers": args.workers if workers else 0, "budget_pass": budget,
                "silent_exit_zero": True, "unchanged_state": True if before is not None else None,
                "folded_events": folded, "fixture": str(fixture_path.relative_to(REPO)),
                "fixture_sha256": digest(fixture_path)}
    finally:
        box.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, default=REPO / "target/release/codepet")
    parser.add_argument("--out", type=Path, default=Path.home() / "Work/runs" /
        ("codepet--" + datetime.date.today().isoformat() + "--latency-" + time.strftime("%H%M%S")))
    parser.add_argument("--runs", type=int, default=200)
    parser.add_argument("--warmup", type=int, default=20)
    parser.add_argument("--load", choices=("none", "python", "stress-ng"), default="none")
    parser.add_argument("--workers", type=int, choices=(1, 2), default=2)
    args = parser.parse_args()
    if not 1 <= args.runs <= 2000 or not 0 <= args.warmup <= 200:
        parser.error("runs must be 1..2000, warmup 0..200")
    args.bin = args.bin.resolve(strict=True)
    args.out = args.out.expanduser().resolve()
    if args.out.exists():
        parser.error("--out must be a new directory; results are never overwritten")
    if args.load == "stress-ng" and not shutil.which("stress-ng"):
        parser.error("stress-ng is unavailable; explicitly choose --load python for bounded fallback")
    version = subprocess.run([str(args.bin), "--version"], capture_output=True, timeout=5,
                             env={"PATH": "/usr/bin:/bin", "LC_ALL": "C"})
    if version.returncode or version.stdout.strip() != b"codepet 0.3.0":
        parser.error("requires a verified codepet 0.3.0 release binary")
    os.umask(0o077)
    args.out.mkdir(parents=True, mode=0o700)
    git = subprocess.run(["git", "status", "--porcelain"], cwd=REPO,
                         capture_output=True, text=True, timeout=5)
    identity = subprocess.run(["git", "rev-parse", "HEAD"], cwd=REPO,
                              capture_output=True, text=True, timeout=5)
    report = {"owner": str(REPO), "purpose": "WP7 isolated hook latency",
        "source_commit": identity.stdout.strip(), "source_dirty": bool(git.stdout),
        "binary": str(args.bin), "binary_sha256": digest(args.bin), "version": "0.3.0",
        "dependencies": ["Linux /proc", "Python stdlib", "bash", "sh", "timeout"],
        "retention": "qualification evidence; synthetic state is reconstructable",
        "review_status": "running", "rows": [], "unmeasured": [], "limitations": [
            "Synthetic representative fixture per client, not live client launches.",
            "Baseline means no harness load; idle eligibility requires load1 < allowed CPUs / 2 and observed CPU busy < 25%.",
            "Available-fold samples wait for a free lock and age synthetic export mtime outside timing to avoid debounce waits.",
            "Cold means absent state, not OS cache eviction. No cache dropping is performed.",
            LOGIN_SHELL_UNMEASURED,
            "Python load is an explicit bounded fallback, not stress-ng equivalence.",
            "Timing includes process-group creation for safety; blocking waitpid avoids Python timeout-polling bias.",
            "Silence/state checks do not prove syscall-only Off behavior; use the separate strace qualification."]}
    report_path = args.out / "report.json"
    try:
        for phase in (["baseline", "loaded"] if args.load != "none" else ["baseline"]):
            for client in FIXTURES:
                for case in CASES:
                    # Keep shim budgets separate from actual stored command/launcher cost.
                    modes = ["shim", "stored"] if client in ("claude-code", "codex", "grok-cli") else ["shim"]
                    if client == "codex":
                        modes.append("bash-lc")
                    for mode in modes:
                        if mode == "bash-lc":
                            report["unmeasured"].append({"phase": phase, "client": client,
                                "case": case, "command": mode, "reason": LOGIN_SHELL_UNMEASURED})
                            print(f"{phase:8} {client:11} {case:14} {mode:7} UNMEASURED", flush=True)
                            continue
                        row = measure(args, phase, client, case, mode, len(report["rows"]))
                        report["rows"].append(row)
                        report_path.write_text(json.dumps(report, indent=2) + "\n")
                        print(f"{phase:8} {client:11} {case:14} {mode:7} n={row['runs']} "
                              f"p50={row['p50_ms']:.2f} p90={row['p90_ms']:.2f} "
                              f"p99={row['p99_ms']:.2f} ms idle={row['idle_eligible']} "
                              f"budget={row['budget_pass']}", flush=True)
        failures = [r for r in report["rows"] if r["budget_pass"] is False]
        report["review_status"] = ("budget-failed" if failures else
                                   "measured-partial" if report["unmeasured"] else "measured")
        report["full_200_run_matrix"] = (args.runs >= 200 and args.load != "none"
                                         and not report["unmeasured"])
        return 1 if failures else 0
    except (Exception, KeyboardInterrupt) as error:
        report["review_status"] = "incomplete"
        report["error"] = f"{type(error).__name__}: {error}"
        print(report["error"], file=sys.stderr)
        return 1
    finally:
        report_path.write_text(json.dumps(report, indent=2) + "\n")
        lines = ["| Phase | Client | Case | Command | n | p50 ms | p90 ms | p99 ms | Idle eligible | Budget |",
                 "|---|---|---|---|---:|---:|---:|---:|---|---|"]
        for r in report["rows"]:
            lines.append(f"| {r['phase']} | {r['client']} | {r['case']} | {r['command']} | {r['runs']} | "
                         f"{r['p50_ms']:.2f} | {r['p90_ms']:.2f} | {r['p99_ms']:.2f} | "
                         f"{r['idle_eligible']} | {r['budget_pass']} |")
        for r in report["unmeasured"]:
            lines.append(f"| {r['phase']} | {r['client']} | {r['case']} | {r['command']} | "
                         "0 | unmeasured | unmeasured | unmeasured | n/a | n/a |")
        (args.out / "summary.md").write_text("\n".join(lines) + "\n\n" +
            "\n".join("- " + item for item in report["limitations"]) + "\n")


if __name__ == "__main__":
    sys.exit(main())
