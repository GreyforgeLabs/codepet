#!/usr/bin/env python3
"""Driver of scripts/e2e-hooks.sh (SPEC-v0.3 §9.2). Expects the wrapper's throwaway
environment: a v5 database, the inbox and the rendered shim under a fake HOME. Never run it
against a real state directory (it refuses a state dir under the real ~/.local/state)."""
import argparse
import json
import os
import sqlite3
import subprocess
import sys
import threading
import time

ap = argparse.ArgumentParser()
ap.add_argument("--state", required=True)
ap.add_argument("--shim", required=True)
a = ap.parse_args()
STATE, SHIM = a.state, a.shim
import pwd  # noqa: E402

_real = os.path.realpath(os.path.join(pwd.getpwuid(os.getuid()).pw_dir, ".local"))
if os.path.realpath(STATE).startswith(_real + os.sep):
    sys.exit("e2e: refusing to run against the real ~/.local state")
INBOX = os.path.join(STATE, "inbox")
BAR = os.path.join(STATE, "bar-state.json")
DB = os.path.join(STATE, "codepet.sqlite3")
ENV = {"PATH": "/usr/bin:/bin", "HOME": os.environ["HOME"]}
SHELL_CLIENTS = {"claude-code", "codex", "grok-cli"}
failures = []


def check(cond, msg):
    print(("ok   " if cond else "FAIL ") + msg)
    if not cond:
        failures.append(msg)


def stored(client, payload, extra_env=None):
    """The client's stored command: sh -c '<shim> <id> >/dev/null 2>&1 || true', or argv."""
    env = dict(ENV, **(extra_env or {}))
    line = (json.dumps(payload, separators=(",", ":")) + "\n").encode()
    if client in SHELL_CLIENTS:
        cmd = ["/bin/sh", "-c", f"{SHIM} {client} >/dev/null 2>&1 || true"]
    else:
        cmd = [SHIM, client]
    r = subprocess.run(cmd, input=line, capture_output=True, env=env, timeout=5)
    if r.returncode != 0 or r.stdout or r.stderr:
        failures.append(f"{client}: rc {r.returncode} out {r.stdout!r} err {r.stderr!r}")


def fold_processes():
    want = f"fold\0--state-dir\0{STATE}\0".encode()
    n = 0
    for pid in os.listdir("/proc"):
        if pid.isdigit():
            try:
                with open(f"/proc/{pid}/cmdline", "rb") as f:
                    n += want in f.read()
            except OSError:
                pass
    return n


def inbox_records():
    return [n for n in os.listdir(INBOX) if n.endswith(".ev") and not n.startswith(".")]


def settle(limit=5.0):
    end = time.monotonic() + limit
    while time.monotonic() < end:
        if not inbox_records() and fold_processes() == 0:
            return True
        time.sleep(0.005)
    return False


def bar():
    with open(BAR) as f:
        return json.load(f)


def db(sql):
    c = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
    try:
        return c.execute(sql).fetchone()[0]
    finally:
        c.close()


def cl(event, extra=None, sid="cc-session-1"):
    p = {"hook_event_name": event, "session_id": sid, "cwd": "/home/u/proj",
         "transcript_path": "/home/u/.claude/x.jsonl"}
    p.update(extra or {})
    return p


def cx(event, extra=None):
    p = {"hook_event_name": event, "session_id": "cx-session-1", "turn_id": "1", "model": "m"}
    p.update(extra or {})
    return p


def gk(snake, pascal, extra=None):
    p = {"hookEventName": snake, "hook_event_name": pascal, "sessionId": "gk-session-1"}
    p.update(extra or {})
    return p


def sy(event, extra=None, sid="oc-session-1"):
    p = {"codepet_v": 1, "event": event, "session_id": sid}
    p.update(extra or {})
    return p


# (client, payload, expected agent.state after the fold, extra env)
SEQUENCES = {
    "claude-code": [
        (cl("SessionStart", {"source": "startup"}), "idle"),
        (cl("UserPromptSubmit", {"prompt_id": "p1", "prompt": "fix it"}), "working"),
        (cl("PreToolUse", {"prompt_id": "p1", "tool_use_id": "tu1", "tool_name": "Bash"}), "working"),
        (cl("Notification", {"notification_type": "permission_prompt", "message": "m"}), "waiting"),
        (cl("PreToolUse", {"prompt_id": "p1", "tool_use_id": "tu2", "tool_name": "Edit"}), "working"),
        (cl("Stop", {"prompt_id": "p1", "stop_hook_active": False}), "idle"),
        (cl("SessionEnd", {"reason": "prompt_input_exit"}), "idle"),
    ],
    "codex": [
        (cx("UserPromptSubmit", {"prompt": "hi"}), "working"),
        (cx("PermissionRequest", {"tool_name": "Bash", "tool_input": {"command": "x"}}), "waiting"),
        (cx("PostToolUse", {"tool_use_id": "call_1", "tool_name": "Bash"}), "working"),
        (cx("Stop", {"stop_hook_active": False}), "idle"),
        ({"hook_event_name": "SessionEnd", "session_id": "cx-session-1", "reason": "other"}, "idle"),
    ],
    "grok-cli": [
        (gk("session_start", "SessionStart", {"source": "new"}), "idle"),
        (gk("user_prompt_submit", "UserPromptSubmit", {"promptId": "g1"}), "working"),
        (gk("pre_tool_use", "PreToolUse", {"promptId": "g1", "toolName": "AskUserQuestion", "toolUseId": "t0"}),
         "waiting"),
        (gk("pre_tool_use", "PreToolUse", {"promptId": "g1", "toolName": "read_file", "toolUseId": "t1"}), "working"),
        (gk("stop", "Stop", {"promptId": "g1", "reason": "end_turn"}), "idle"),
        (gk("session_end", "SessionEnd", {"reason": "exit"}), "idle"),
    ],
    "opencode": [
        (sy("session_start", {"interactive": True}), "idle"),
        (sy("turn_start", {"turn_id": "1792-1", "interactive": True}), "working"),
        (sy("waiting", {"turn_id": "1792-1"}), "waiting"),
        (sy("activity", {"turn_id": "1792-1"}), "working"),
        (sy("turn_done", {"turn_id": "1792-1", "tools": 2, "interactive": True}), "idle"),
        (sy("session_end"), "idle"),
    ],
    "hermes": [
        (sy("session_start", {"interactive": True}, sid="hm-session-1"), "idle"),
        (sy("turn_start", {"turn_id": "h1", "interactive": True}, sid="hm-session-1"), "working"),
        ({"codepet_v": 1, "event": "waiting", "approval_key": "sk-cli-1"}, "waiting"),
        ({"codepet_v": 1, "event": "activity", "approval_key": "sk-cli-1"}, "working"),
        (sy("turn_done", {"turn_id": "h1", "tools": 1}, sid="hm-session-1"), "idle"),
        (sy("session_end", sid="hm-session-1"), "idle"),
    ],
}

print("== sequences through the stored command strings")
treats = finished = hello = 0
for client, steps in SEQUENCES.items():
    env = {"CLAUDE_CODE_ENTRYPOINT": "cli"} if client == "claude-code" else {}
    before = time.time() * 1000
    for i, (payload, want) in enumerate(steps):
        stored(client, payload, env)
        settled = settle()
        state = bar()["agent"]["state"] if settled else "?"
        check(settled and state == want, f"{client} step {i} ({payload.get('hook_event_name') or payload['event']}): "
                                         f"agent.state {state} (want {want})")
    treats += 1
    finished += 1
    hello += 1
    b = bar()
    check(b["agent"]["finished_seq"] == finished, f"{client}: finished_seq {b['agent']['finished_seq']} == {finished}")
    check(b["agent"]["finished_app"] == client, f"{client}: finished_app {b['agent']['finished_app']}")
    check(b["agent"]["finished_treat"] is True, f"{client}: the turn earned a treat")
    check(b["agent"]["hello_seq"] == hello, f"{client}: hello_seq {b['agent']['hello_seq']} == {hello}")
    check(b["treats"]["jar"] == treats, f"{client}: jar {b['treats']['jar']} == {treats}")
    seen = db(f"SELECT last_seen_ms FROM client_hooks WHERE client = '{client}'") or 0
    check(seen >= before - 1000, f"{client}: client_hooks.last_seen_ms recorded")

print("== burst: 500 activity events from one writer")
stored("claude-code", cl("UserPromptSubmit", {"prompt_id": "b1"}, sid="burst-1"), {"CLAUDE_CODE_ENTRYPOINT": "cli"})
settle()
stop = threading.Event()
peak = [0]
renames = []


def sampler():
    last = None
    while not stop.is_set():
        peak[0] = max(peak[0], fold_processes())
        try:
            ino = os.stat(BAR).st_ino
            if ino != last:
                renames.append(time.monotonic())
                last = ino
        except OSError:
            pass
        time.sleep(0.005)


t = threading.Thread(target=sampler)
t.start()
before_events = db("SELECT COUNT(*) FROM agent_events")
start = time.monotonic()
for i in range(500):
    stored("claude-code", cl("PreToolUse", {"prompt_id": "b1", "tool_use_id": f"b-{i}"}, sid="burst-1"),
           {"CLAUDE_CODE_ENTRYPOINT": "cli"})
last_event = time.monotonic()
emptied = None
while time.monotonic() < last_event + 1.0:
    if not inbox_records():
        emptied = time.monotonic() - last_event
        break
    time.sleep(0.005)
settle()
stop.set()
t.join()
burst_s = last_event - start
check(emptied is not None, f"inbox empty within 1 s of the last event ({emptied if emptied is not None else '>1'} s)")
check(peak[0] <= 1, f"at most one fold process at any 5 ms sample (peak {peak[0]})")
got = db("SELECT COUNT(*) FROM agent_events") - before_events
check(got == 500, f"nothing lost: {got} of 500 recorded")
rate = (len(renames) - 1) / max(burst_s, 0.001)
check(rate <= 5.0 + 1.0 / max(burst_s, 0.001), f"bar-state renames {len(renames) - 1} in {burst_s:.2f} s "
                                                 f"({rate:.1f}/s; non-urgent changes only)")

print("== burst: 20 concurrent writers x 25")
before_events = db("SELECT COUNT(*) FROM agent_events")
peak[0] = 0
stop.clear()
t = threading.Thread(target=sampler)
t.start()


def writer(w):
    for i in range(25):
        stored("claude-code", cl("PreToolUse", {"prompt_id": "b1", "tool_use_id": f"c-{w}-{i}"}, sid="burst-1"),
               {"CLAUDE_CODE_ENTRYPOINT": "cli"})


ws = [threading.Thread(target=writer, args=(w,)) for w in range(20)]
for w in ws:
    w.start()
for w in ws:
    w.join()
last_event = time.monotonic()
emptied = None
while time.monotonic() < last_event + 1.0:
    if not inbox_records():
        emptied = time.monotonic() - last_event
        break
    time.sleep(0.005)
settle()
stop.set()
t.join()
check(emptied is not None, "inbox empty within 1 s of the last concurrent event")
check(peak[0] <= 1, f"at most one fold process (peak {peak[0]})")
got = db("SELECT COUNT(*) FROM agent_events") - before_events
check(got == 500, f"nothing lost: {got} of 500 recorded")
check(db("SELECT open_turn_tools FROM agent_sessions WHERE state = 'working'") == 1000,
      "1000 distinct tools counted in the burst turn")
check(not os.path.exists(os.path.join(STATE, "fold.backoff")), "no fold failed")

print()
if failures:
    print(f"e2e: {len(failures)} FAILED")
    for f in failures:
        print("  -", f)
    sys.exit(1)
print("e2e: all checks passed")
