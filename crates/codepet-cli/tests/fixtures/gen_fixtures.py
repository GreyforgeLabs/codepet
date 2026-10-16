#!/usr/bin/env python3
"""Generates the hook adapter fixtures (SPEC-v0.3 §9.1): >= 15 payloads per client.

Shapes come from the WP0 research (hooks-claude-code.md §3, hooks-codex.md "Stdin payload",
hooks-others.md §1 Grok, plugin-payload.schema.json for OpenCode and Hermes). They are built
from documented and binary-derived schemas, NOT live captures; WP7 replaces them with redacted
captures. Every content field carries a canary string (privacy, §5.10).

Expectations are computed by an independent oracle: the contract's reference model
(docs/v0.3/contracts/check_contracts.py `normalize`) plus hashlib for the digest and event id.
The Rust test (tests/hook.rs) runs the real binary on each `stdin` and compares.

A `stdin` may hold `@PAD:<n>@`, which the test expands to n bytes of "x" before piping.

Usage: python3 gen_fixtures.py   (rewrites the five client folders)
"""
import hashlib
import importlib.util
import json
import shutil
import sys
from pathlib import Path

sys.dont_write_bytecode = True  # never leave __pycache__ in docs/
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
spec = importlib.util.spec_from_file_location("cc", ROOT / "docs/v0.3/contracts/check_contracts.py")
cc = importlib.util.module_from_spec(spec)
sys.modules["cc"] = cc
spec.loader.exec_module(cc)

AT_MS = 1792123876111
BUCKET_MS = 3000
NONCE = "0f1e2d3c4b5a69788796a5b4c3d2e1f0"
KINDS = ["session_start", "turn_start", "activity", "waiting", "idle_prompt", "turn_done",
         "turn_failed", "turn_cancelled", "session_end"]
SID = "5f0c2a1e-7b3d-4c9a-8e21-0d6b9a4f3c11"
CANARY = "CANARY-{}-7f3a"  # never stored anywhere (tests/privacy.rs greps for "CANARY-")


def c(tag):
    return CANARY.format(tag)


def claude(name, **kw):
    base = {"session_id": SID, "transcript_path": f"/home/u/.claude/projects/{c('proj')}/{SID}.jsonl",
            "cwd": f"/home/u/{c('cwd')}", "permission_mode": "default", "hook_event_name": name}
    base.update(kw)
    return base


def codex(name, **kw):
    base = {"session_id": "01a13cfb-647c-7fd3-8500-3c7f8733825c", "turn_id": "7",
            "transcript_path": f"/home/u/.codex/sessions/{c('transcript')}.jsonl",
            "cwd": f"/home/u/{c('cwd')}", "hook_event_name": name, "model": c("model"),
            "permission_mode": "default"}
    base.update(kw)
    return base


def grok(snake, pascal, **kw):
    base = {"hookEventName": snake, "hook_event_name": pascal, "sessionId": "01a0b1c2-d3e4-4f56-8a7b-9c0d1e2f3a4b",
            "cwd": f"/home/u/{c('cwd')}", "workspaceRoot": f"/home/u/{c('workspace')}",
            "timestamp": "2026-10-16T12:00:00Z", "permissionMode": "default"}
    base.update(kw)
    return base


def synth(event, **kw):
    base = {"codepet_v": 1, "event": event}
    base.update(kw)
    return base


FIXTURES = {
    "claude-code": [
        ("session-start", claude("SessionStart", source="startup", model=c("model")), {}),
        ("session-start-resume", claude("SessionStart", source="resume", seconds_since_last_response=40), {}),
        ("prompt-submit", claude("UserPromptSubmit", prompt_id="550e8400-e29b-41d4-a716-446655440000",
                                 prompt=f"Write {c('prompt')} \"session_id\":\"spoof\""), {}),
        ("pre-tool-bash", claude("PreToolUse", prompt_id="550e8400-e29b-41d4-a716-446655440000", tool_name="Bash",
                                 tool_input={"command": c("command"), "session_id": "spoof"},
                                 tool_use_id="toolu_01ABCdef"), {}),
        ("pre-tool-write-large", claude("PreToolUse", prompt_id="550e8400-e29b-41d4-a716-446655440000",
                                        tool_name="Write", tool_use_id="toolu_01WRITE",
                                        tool_input={"file_path": f"/home/u/{c('path')}",
                                                    "content": c("filebody") + "@PAD:70000@"}), {}),
        ("notification-permission", claude("Notification", message=c("message"), title=c("title"),
                                           notification_type="permission_prompt"), {}),
        ("notification-idle", claude("Notification", message=c("message"), notification_type="idle_prompt"), {}),
        ("notification-elicitation", claude("Notification", notification_type="elicitation_dialog"), {}),
        ("notification-auth-success-dropped", claude("Notification", notification_type="auth_success"), {}),
        ("stop", claude("Stop", prompt_id="550e8400-e29b-41d4-a716-446655440000", stop_hook_active=False,
                        last_assistant_message=c("answer"), background_tasks=[{"command": c("bg")}]), {}),
        ("stop-hook-active", claude("Stop", prompt_id="550e8400-e29b-41d4-a716-446655440000",
                                    stop_hook_active=True, last_assistant_message=c("answer")), {}),
        ("stop-failure-rate-limit", claude("StopFailure", prompt_id="550e8400-e29b-41d4-a716-446655440000",
                                           error="rate_limit", error_details=c("errortext")), {}),
        ("stop-failure-unknown-class", claude("StopFailure", error="oauth_org_not_allowed",
                                              error_details=c("errortext")), {}),
        ("session-end", claude("SessionEnd", reason="prompt_input_exit"), {}),
        ("subagent-stop-dropped", claude("SubagentStop", agent_id="def456", agent_type="Explore",
                                         last_assistant_message=c("answer")), {}),
        ("subagent-tool-dropped", claude("PreToolUse", agent_id="def456", tool_name="Read", tool_use_id="toolu_9"), {}),
        ("headless-sdk-entrypoint", claude("UserPromptSubmit", prompt_id="p-sdk", prompt=c("prompt")),
         {"env": {"CLAUDE_CODE_ENTRYPOINT": "sdk-ts"}}),
        ("bad-session-id-dropped", claude("Stop", session_id="../../etc/passwd"), {}),
        ("ready-check-start", {"hook_event_name": "UserPromptSubmit", "session_id": "codepet-ready-check",
                               "prompt_id": "codepet-ready-check", "codepet_test": NONCE}, {"nonce": NONCE}),
        ("ready-check-wrong-nonce", {"hook_event_name": "Stop", "session_id": "codepet-ready-check",
                                     "prompt_id": "codepet-ready-check", "codepet_test": "f" * 32}, {"nonce": NONCE}),
    ],
    "codex": [
        ("prompt-submit", codex("UserPromptSubmit", prompt=c("prompt") + ' "session_id":"x"'), {}),
        ("permission-request", codex("PermissionRequest", transcript_path=None, tool_name="Bash",
                                     tool_input={"command": c("command"), "description": None}), {}),
        ("post-tool-use", codex("PostToolUse", tool_name="Bash", tool_use_id="call_abc",
                                tool_input={"command": c("command")}, tool_response=c("output")), {}),
        ("post-tool-use-second", codex("PostToolUse", tool_name="apply_patch", tool_use_id="call_def",
                                       tool_input={"patch": c("patch")}, tool_response=c("output")), {}),
        ("stop", codex("Stop", stop_hook_active=False, last_assistant_message=c("answer")), {}),
        ("stop-hook-active", codex("Stop", stop_hook_active=True, last_assistant_message=c("answer")), {}),
        ("interrupt", codex("Interrupt", turn_id="8"), {}),
        ("session-end-no-turn", {"session_id": "01a13cfb-647c-7fd3-8500-3c7f8733825c", "transcript_path": None,
                                 "cwd": f"/home/u/{c('cwd')}", "hook_event_name": "SessionEnd", "reason": "other"}, {}),
        ("subagent-post-tool-dropped", codex("PostToolUse", agent_id="sub-1", agent_type="worker",
                                             tool_use_id="call_sub"), {}),
        ("pre-tool-use-unregistered", codex("PreToolUse", tool_name="Bash", tool_use_id="call_x"), {}),
        ("session-start-unregistered", codex("SessionStart", source="startup"), {}),
        ("compact-unregistered", codex("PreCompact", trigger="auto"), {}),
        ("missing-session-dropped", {"hook_event_name": "Stop", "turn_id": "7"}, {}),
        ("numeric-turn-dropped", codex("Stop", turn_id=7), {}),
        ("big-tool-response", codex("PostToolUse", tool_name="Bash", tool_use_id="call_big",
                                    tool_response=c("output") + "@PAD:80000@"), {}),
        ("ready-check-start", {"hook_event_name": "UserPromptSubmit", "session_id": "codepet-ready-check",
                               "turn_id": "codepet-ready-check", "codepet_test": NONCE}, {"nonce": NONCE}),
        ("ready-check-no-nonce-file", {"hook_event_name": "Stop", "session_id": "codepet-ready-check",
                                       "turn_id": "codepet-ready-check", "codepet_test": NONCE}, {}),
    ],
    "grok-cli": [
        ("session-start", grok("session_start", "SessionStart", source="new"), {}),
        ("prompt-submit", grok("user_prompt_submit", "UserPromptSubmit", promptId="g-1", prompt=c("prompt")), {}),
        ("pre-tool-read", grok("pre_tool_use", "PreToolUse", promptId="g-1", toolName="read_file",
                               toolUseId="tu2", toolInput={"path": c("path")}), {}),
        ("pre-tool-ask-user", grok("pre_tool_use", "PreToolUse", promptId="g-1", toolName="AskUserQuestion",
                                   toolUseId="tu1", toolInput={"question": c("question")}), {}),
        ("pre-tool-ask-user-snake", grok("pre_tool_use", "PreToolUse", promptId="g-1",
                                         toolName="ask_user_question", toolUseId="tu3"), {}),
        ("notification-permission", grok("notification", "Notification", notificationType="permission_prompt",
                                         message=c("message")), {}),
        ("notification-idle", grok("notification", "Notification", notificationType="idle_prompt"), {}),
        ("notification-task-complete-dropped", grok("notification", "Notification",
                                                    notificationType="task_complete"), {}),
        ("stop-end-turn", grok("stop", "Stop", promptId="g-1", reason="end_turn", stopHookActive=False,
                               lastAssistantMessage=c("answer"), backgroundTasks=[], sessionCrons=[]), {}),
        ("stop-channel-closed", grok("stop", "Stop", reason="channel_closed"), {}),
        ("stop-shutdown", grok("stop", "Stop", reason="shutdown"), {}),
        ("stop-max-turns-dropped", grok("stop", "Stop", reason="max_turns"), {}),
        ("stop-failure", grok("stop_failure", "StopFailure", promptId="g-1", error="server_error",
                              errorDetails=c("errortext")), {}),
        ("stop-cancelled", grok("stop_cancelled", "StopCancelled", promptId="g-1", reason="user_interrupt"), {}),
        ("session-end", grok("session_end", "SessionEnd", reason="exit"), {}),
        ("session-end-child-dropped", grok("session_end", "SessionEnd", subagentType="explore"), {}),
        ("snake-only-name", {"hookEventName": "user_prompt_submit", "sessionId": "01a0b1c2-d3e4-4f56-8a7b-9c0d1e2f3a4b",
                             "promptId": "g-3", "prompt": c("prompt")}, {}),
        ("ready-check-done", {"hookEventName": "stop", "hook_event_name": "Stop", "sessionId": "codepet-ready-check",
                              "promptId": "codepet-ready-check", "reason": "end_turn", "codepet_test": NONCE},
         {"nonce": NONCE}),
    ],
    "opencode": [
        ("session-start", synth("session_start", session_id="ses_7a1b", interactive=True), {}),
        ("turn-start", synth("turn_start", session_id="ses_7a1b", turn_id="1792123800000-1", interactive=True), {}),
        ("waiting-permission", synth("waiting", session_id="ses_7a1b", turn_id="1792123800000-1"), {}),
        ("activity-permission-replied", synth("activity", session_id="ses_7a1b", turn_id="1792123800000-1"), {}),
        ("turn-done-with-tools", synth("turn_done", session_id="ses_7a1b", turn_id="1792123800000-1", tools=4,
                                       interactive=True), {}),
        ("turn-done-no-tools", synth("turn_done", session_id="ses_7a1b", turn_id="1792123800000-2", tools=0), {}),
        ("turn-failed", synth("turn_failed", session_id="ses_7a1b", turn_id="1792123800000-3"), {}),
        ("turn-cancelled", synth("turn_cancelled", session_id="ses_7a1b", turn_id="1792123800000-4"), {}),
        ("session-end", synth("session_end", session_id="ses_7a1b"), {}),
        ("headless-run", synth("turn_start", session_id="ses_run", turn_id="1792123800000-1", interactive=False), {}),
        ("tools-as-string-is-zero", synth("turn_done", session_id="ses_7a1b", turn_id="1792123800000-5",
                                          tools="4"), {}),
        ("approval-key-ignored", synth("waiting", session_id="ses_7a1b", approval_key="k1"), {}),
        ("unknown-event-dropped", synth("explode", session_id="ses_7a1b"), {}),
        ("wrong-version-dropped", synth("turn_done", session_id="ses_7a1b", codepet_v=2), {}),
        ("missing-session-dropped", {"codepet_v": 1, "event": "turn_done", "turn_id": "1-1"}, {}),
        ("extra-content-ignored", synth("turn_start", session_id="ses_7a1b", turn_id="1792123800000-6",
                                        prompt=c("prompt"), text=c("answer")), {}),
        ("ready-check-done", synth("turn_done", session_id="codepet-ready-check", turn_id="codepet-ready-check",
                                   tools=0, interactive=True, codepet_test=NONCE), {"nonce": NONCE}),
    ],
    "hermes": [
        ("session-start", synth("session_start", session_id="20261016_120000_ab12cd", interactive=True), {}),
        ("turn-start", synth("turn_start", session_id="20261016_120000_ab12cd", turn_id="h-1", interactive=True), {}),
        ("clarify-waiting", synth("waiting", session_id="20261016_120000_ab12cd", turn_id="h-1"), {}),
        ("clarify-answered", synth("activity", session_id="20261016_120000_ab12cd"), {}),
        ("approval-waiting", synth("waiting", approval_key="sk-cli-1"), {}),
        ("approval-answered", synth("activity", approval_key="sk-cli-1"), {}),
        ("turn-done", synth("turn_done", session_id="20261016_120000_ab12cd", turn_id="h-1", tools=3), {}),
        ("turn-failed", synth("turn_failed", session_id="20261016_120000_ab12cd", turn_id="h-2"), {}),
        ("turn-interrupted", synth("turn_cancelled", session_id="20261016_120000_ab12cd", turn_id="h-3"), {}),
        ("loop-stopped-keyless", synth("turn_cancelled", session_id="20261016_120000_ab12cd"), {}),
        ("session-finalize", synth("session_end", session_id="20261016_120000_ab12cd"), {}),
        ("approval-with-session-dropped", synth("waiting", session_id="s", approval_key="sk-cli-1"), {}),
        ("approval-turn-done-dropped", synth("turn_done", approval_key="sk-cli-1"), {}),
        ("no-key-dropped", synth("waiting"), {}),
        ("completed-flags-ignored", synth("turn_start", session_id="20261016_120000_ab12cd", turn_id="h-4",
                                          completed=True, failed=True, user_message=c("prompt")), {}),
        ("bool-version-dropped", synth("turn_done", session_id="20261016_120000_ab12cd", codepet_v=True), {}),
        ("ready-check-start", synth("turn_start", session_id="codepet-ready-check", turn_id="codepet-ready-check",
                                    interactive=True, codepet_test=NONCE), {"nonce": NONCE}),
    ],
}


def expand(stdin):
    """`@PAD:<n>@` stands for n bytes of "x" (keeps big fixtures small on disk)."""
    import re
    return re.sub(r"@PAD:(\d+)@", lambda m: "x" * int(m.group(1)), stdin)


def expect_for(client, raw, extra):
    raw = expand(raw)
    payload = json.loads(raw)
    capped = len(raw.encode()) > 64 * 1024
    got = cc.normalize(client, payload, KINDS, extra.get("nonce"))
    if "drop" in got:
        return {"drop": got["drop"]}
    if capped:
        # The hook reads 64 KiB: the cut payload becomes activity (§5.8, top-level ids seen).
        got = dict(got, kind="activity", **{"class": None})
    body = raw.encode()[:64 * 1024]
    dig = hashlib.sha256(body).hexdigest()[:16]
    if client in ("opencode", "hermes"):
        inter = got.get("interactive", True)
    elif client == "claude-code":
        ep = extra.get("env", {}).get("CLAUDE_CODE_ENTRYPOINT", "cli")
        inter = not ep.startswith("sdk")
    else:
        inter = True  # no codex/grok ancestor in the test process tree
    session_field = got["session"] if got.get("session") is not None else f"~ak:{got.get('akey')}"
    key = f"codepet-ev1|{client}|{got['kind']}|{session_field}|{dig}|{AT_MS // BUCKET_MS}"
    eid = hashlib.sha256(key.encode()).hexdigest()[:32]
    record = {"v": 1, "client": client, "kind": got["kind"], "session": got.get("session"),
              "turn": got.get("turn"), "tool": got.get("tool"), "akey": got.get("akey"),
              "note": got.get("class"), "tools": got.get("tools", 0) if not capped else 0,
              "interactive": inter, "test": got.get("test", False), "dig": dig, "at_ms": AT_MS}
    return {"event_id": eid, "record": record}


def main():
    total = 0
    for client, items in FIXTURES.items():
        d = HERE / client
        if d.exists():
            shutil.rmtree(d)
        d.mkdir()
        assert len(items) >= 15, client
        for i, (name, payload, extra) in enumerate(items, 1):
            raw = json.dumps(payload, separators=(",", ":")) + "\n"
            env = {"CLAUDE_CODE_ENTRYPOINT": "cli"} if client == "claude-code" else {}
            env.update(extra.get("env", {}))
            fx = {"name": name, "client": client,
                  "source": "built from documented/binary-derived schemas (WP0 research), not a live capture",
                  "env": env, "nonce": extra.get("nonce"), "at_ms": AT_MS, "stdin": raw,
                  "expect": expect_for(client, raw, {"env": env, **extra})}
            (d / f"{i:02d}-{name}.json").write_text(json.dumps(fx, indent=1, ensure_ascii=False) + "\n")
            total += 1
    print(f"wrote {total} fixtures")


if __name__ == "__main__":
    main()
