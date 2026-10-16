#!/usr/bin/env python3
"""Writes docs/v0.3/contracts/agent_vectors.json (WP0). Expectations are hand-written here;
check_contracts.py replays them through an independent reference model. Run from anywhere:
    python3 docs/v0.3/contracts/tools/gen_agent_vectors.py
"""
import hashlib
import json
from collections import OrderedDict
from pathlib import Path

OUT = Path(__file__).resolve().parents[1] / "agent_vectors.json"

constants = OrderedDict([
    ("stale_working_ms", 600000), ("stale_waiting_ms", 1800000), ("min_turn_ms", 30000),
    ("jar_max", 10), ("cap_session_ms", 30000), ("cap_per_hour", 12), ("cap_per_day", 48),
    ("late_tool_ms", 1000), ("celebrate_window_ms", 30000), ("celebrate_grace_ms", 2000),
    ("error_recent_ms", 120000), ("error_frame_ms", 3000), ("future_clamp_ms", 120000),
    ("max_event_age_ms", 86400000), ("treat_xp", 5), ("max_hook_read_bytes", 65536),
    ("hook_drain_ms", 250), ("max_inbox_record_bytes", 512), ("max_inbox_files", 2048),
    ("max_id_field_bytes", 128), ("event_bucket_ms", 3000), ("fold_batch_events", 64),
    ("fold_batch_ms", 50), ("fold_run_ms", 5000), ("fold_backoff_start_ms", 5000),
    ("fold_backoff_max_ms", 600000), ("agent_events_keep_ms", 604800000),
    ("agent_events_max_rows", 20000), ("agent_sessions_keep_ms", 2592000000),
    ("turn_times_exact_ms", 86400000), ("retention_interval_ms", 3600000),
    ("retention_limit", 500), ("export_min_interval_ms", 250), ("test_done_delay_ms", 1500),
    ("test_arrival_ms", 5000),
])

clients = [
    {"id": "claude-code", "name": "Claude Code", "first_run_card": True, "payload": "native snake_case",
     "turn_key": "prompt_id", "transport": "shell command via Claude plugin hooks.json"},
    {"id": "codex", "name": "Codex", "first_run_card": False, "payload": "native snake_case",
     "turn_key": "turn_id", "transport": "shell command via ~/.codex/hooks.json ($SHELL -lc)"},
    {"id": "opencode", "name": "OpenCode", "first_run_card": True, "payload": "synthesized codepet_v 1",
     "turn_key": "turn_id (<pluginStartMs>-<n>)", "transport": "argv spawn from codepet.js"},
    {"id": "hermes", "name": "Hermes", "first_run_card": True, "payload": "synthesized codepet_v 1",
     "turn_key": "turn_id", "transport": "argv spawn from the Hermes plugin"},
    {"id": "grok-cli", "name": "Grok", "first_run_card": True, "payload": "native camelCase + hook_event_name",
     "turn_key": "promptId", "transport": "shell command via ~/.grok/hooks/codepet.json"},
]

kinds = ["session_start", "turn_start", "activity", "waiting", "idle_prompt", "turn_done",
         "turn_failed", "turn_cancelled", "session_end"]

# ---------------------------------------------------------------- normalization (§5.3-5.8)
def n(client, payload, expect):
    return {"client": client, "payload": payload, "expect": expect}

S = "11111111-2222-3333-4444-555555555555"
normalization = {
    "rules": [
        "argv client id is authoritative; the payload only passes a per-client schema sanity check",
        "only top-level keys are read (depth 1); nested values are skipped unbuffered",
        "id fields: strings <= 128 bytes of [A-Za-z0-9._:-], else the event is dropped",
        "agent_id / subagentType present -> dropped before any write",
        "unmapped event names or notification types -> dropped",
        "Grok: prefer hook_event_name (PascalCase) over hookEventName (snake_case); camelCase ids",
        "error: one of rate_limit|authentication_failed|invalid_request|server_error|max_output_tokens|overloaded|billing_error|unknown, else unknown",
    ],
    "event_names": {
        "claude-code": {"SessionStart": "session_start", "UserPromptSubmit": "turn_start",
                        "PreToolUse": "activity", "Notification": "by notification_type", "Stop": "turn_done",
                        "StopFailure": "turn_failed", "SessionEnd": "session_end"},
        "codex": {"UserPromptSubmit": "turn_start", "PermissionRequest": "waiting", "PostToolUse": "activity",
                  "Stop": "turn_done", "Interrupt": "turn_cancelled", "SessionEnd": "session_end"},
        "grok-cli": {"SessionStart": "session_start", "UserPromptSubmit": "turn_start",
                     "PreToolUse": "activity; AskUserQuestion/ask_user_question -> waiting",
                     "Notification": "by notificationType", "Stop": "reason end_turn -> turn_done; channel_closed/shutdown -> session_end; other -> dropped",
                     "StopFailure": "turn_failed", "StopCancelled": "turn_cancelled", "SessionEnd": "session_end"},
        "opencode": {"(synthesized)": "event field is one of the nine kinds"},
        "hermes": {"(synthesized)": "event field is one of the nine kinds"},
    },
    "notification_types": {
        "claude-code": {"permission_prompt": "waiting", "elicitation_dialog": "waiting",
                        "elicitation_url_dialog": "waiting", "agent_needs_input": "waiting", "idle_prompt": "idle_prompt"},
        "grok-cli": {"permission_prompt": "waiting", "idle_prompt": "idle_prompt"},
    },
    "vectors": [
        n("claude-code", {"hook_event_name": "SessionStart", "session_id": S, "source": "startup", "cwd": "/p"},
          {"kind": "session_start", "session": S, "turn": None, "tool": None, "class": None}),
        n("claude-code", {"hook_event_name": "UserPromptSubmit", "session_id": S, "prompt_id": "p-1",
                          "prompt": "say \"session_id\":\"x\" please"},
          {"kind": "turn_start", "session": S, "turn": "p-1", "tool": None, "class": None}),
        n("claude-code", {"hook_event_name": "PreToolUse", "session_id": S, "prompt_id": "p-1", "tool_name": "Bash",
                          "tool_use_id": "toolu_01", "tool_input": {"command": "ls", "session_id": "spoof"}},
          {"kind": "activity", "session": S, "turn": "p-1", "tool": "toolu_01", "class": None}),
        n("claude-code", {"hook_event_name": "Notification", "session_id": S, "notification_type": "permission_prompt",
                          "message": "Claude needs your permission"},
          {"kind": "waiting", "session": S, "turn": None, "tool": None, "class": "permission_prompt"}),
        n("claude-code", {"hook_event_name": "Notification", "session_id": S, "notification_type": "idle_prompt"},
          {"kind": "idle_prompt", "session": S, "turn": None, "tool": None, "class": "idle_prompt"}),
        n("claude-code", {"hook_event_name": "Notification", "session_id": S, "notification_type": "auth_success"},
          {"drop": "unmapped"}),
        n("claude-code", {"hook_event_name": "Stop", "session_id": S, "prompt_id": "p-1", "stop_hook_active": False},
          {"kind": "turn_done", "session": S, "turn": "p-1", "tool": None, "class": None}),
        n("claude-code", {"hook_event_name": "StopFailure", "session_id": S, "prompt_id": "p-1", "error": "rate_limit"},
          {"kind": "turn_failed", "session": S, "turn": "p-1", "tool": None, "class": "rate_limit"}),
        n("claude-code", {"hook_event_name": "StopFailure", "session_id": S, "prompt_id": "p-1", "error": "disk on fire"},
          {"kind": "turn_failed", "session": S, "turn": "p-1", "tool": None, "class": "unknown"}),
        n("claude-code", {"hook_event_name": "SessionEnd", "session_id": S, "reason": "prompt_input_exit"},
          {"kind": "session_end", "session": S, "turn": None, "tool": None, "class": None}),
        n("claude-code", {"hook_event_name": "PreToolUse", "session_id": S, "agent_id": "def456", "tool_use_id": "t"},
          {"drop": "child"}),
        n("claude-code", {"hook_event_name": "PreToolUse", "session_id": "../etc/passwd", "tool_use_id": "t"},
          {"drop": "bad_id"}),
        n("claude-code", {"hook_event_name": "SubagentStop", "session_id": S},
          {"drop": "unmapped"}),
        n("codex", {"hook_event_name": "UserPromptSubmit", "session_id": S, "turn_id": "7", "prompt": "hi"},
          {"kind": "turn_start", "session": S, "turn": "7", "tool": None, "class": None}),
        n("codex", {"hook_event_name": "PermissionRequest", "session_id": S, "turn_id": "7", "tool_name": "Bash",
                    "tool_input": {"command": "cargo test"}},
          {"kind": "waiting", "session": S, "turn": "7", "tool": None, "class": None}),
        n("codex", {"hook_event_name": "PostToolUse", "session_id": S, "turn_id": "7", "tool_use_id": "call_1",
                    "tool_name": "Bash"},
          {"kind": "activity", "session": S, "turn": "7", "tool": "call_1", "class": None}),
        n("codex", {"hook_event_name": "Stop", "session_id": S, "turn_id": "7"},
          {"kind": "turn_done", "session": S, "turn": "7", "tool": None, "class": None}),
        n("codex", {"hook_event_name": "Interrupt", "session_id": S, "turn_id": "7"},
          {"kind": "turn_cancelled", "session": S, "turn": "7", "tool": None, "class": None}),
        n("codex", {"hook_event_name": "SessionEnd", "session_id": S},
          {"kind": "session_end", "session": S, "turn": None, "tool": None, "class": None}),
        n("codex", {"hook_event_name": "PreToolUse", "session_id": S, "turn_id": "7"},
          {"drop": "unmapped"}),
        n("grok-cli", {"hookEventName": "user_prompt_submit", "hook_event_name": "UserPromptSubmit",
                       "sessionId": S, "promptId": "g-1"},
          {"kind": "turn_start", "session": S, "turn": "g-1", "tool": None, "class": None}),
        n("grok-cli", {"hookEventName": "pre_tool_use", "hook_event_name": "PreToolUse", "sessionId": S,
                       "promptId": "g-1", "toolName": "AskUserQuestion", "toolUseId": "tu1"},
          {"kind": "waiting", "session": S, "turn": "g-1", "tool": "tu1", "class": "ask_user"}),
        n("grok-cli", {"hookEventName": "pre_tool_use", "hook_event_name": "PreToolUse", "sessionId": S,
                       "promptId": "g-1", "toolName": "read_file", "toolUseId": "tu2"},
          {"kind": "activity", "session": S, "turn": "g-1", "tool": "tu2", "class": None}),
        n("grok-cli", {"hookEventName": "notification", "hook_event_name": "Notification", "sessionId": S,
                       "notificationType": "permission_prompt"},
          {"kind": "waiting", "session": S, "turn": None, "tool": None, "class": "permission_prompt"}),
        n("grok-cli", {"hookEventName": "stop", "hook_event_name": "Stop", "sessionId": S, "promptId": "g-1",
                       "reason": "end_turn", "stopHookActive": False, "lastAssistantMessage": "do not store"},
          {"kind": "turn_done", "session": S, "turn": "g-1", "tool": None, "class": None}),
        n("grok-cli", {"hookEventName": "stop", "hook_event_name": "Stop", "sessionId": S, "reason": "channel_closed"},
          {"kind": "session_end", "session": S, "turn": None, "tool": None, "class": None}),
        n("grok-cli", {"hookEventName": "stop", "hook_event_name": "Stop", "sessionId": S, "reason": "max_turns"},
          {"drop": "unmapped"}),
        n("grok-cli", {"hookEventName": "stop_cancelled", "hook_event_name": "StopCancelled", "sessionId": S,
                       "promptId": "g-1", "reason": "user_interrupt"},
          {"kind": "turn_cancelled", "session": S, "turn": "g-1", "tool": None, "class": None}),
        n("grok-cli", {"hookEventName": "session_end", "hook_event_name": "SessionEnd", "sessionId": S,
                       "subagentType": "explore"},
          {"drop": "child"}),
        n("opencode", {"codepet_v": 1, "event": "turn_done", "session_id": "ses_1", "turn_id": "1792-3",
                       "tools": 4, "interactive": True},
          {"kind": "turn_done", "session": "ses_1", "turn": "1792-3", "tool": None, "class": None, "tools": 4,
           "interactive": True}),
        n("opencode", {"codepet_v": 1, "event": "explode", "session_id": "ses_1"}, {"drop": "unmapped"}),
        n("hermes", {"codepet_v": 1, "event": "waiting", "approval_key": "sk-cli-1"},
          {"kind": "waiting", "session": None, "turn": None, "tool": None, "class": None, "akey": "sk-cli-1"}),
        n("hermes", {"codepet_v": 1, "event": "turn_cancelled", "session_id": "h_1"},
          {"kind": "turn_cancelled", "session": "h_1", "turn": None, "tool": None, "class": None}),
    ],
}


def event_id(client, kind, session, body, at_ms):
    dig = hashlib.sha256(body).hexdigest()[:16]
    key = f"codepet-ev1|{client}|{kind}|{session}|{dig}|{at_ms // 3000}"
    return dig, hashlib.sha256(key.encode()).hexdigest()[:32]


eid_cases = []
for client, kind, session, body, at in [
    ("claude-code", "turn_done", S, b'{"hook_event_name":"Stop","session_id":"' + S.encode() + b'","prompt_id":"p-1"}\n', 1792123876111),
    ("claude-code", "turn_done", S, b'{"hook_event_name":"Stop","session_id":"' + S.encode() + b'","prompt_id":"p-1"}\n', 1792123877999),
    ("claude-code", "turn_done", S, b'{"hook_event_name":"Stop","session_id":"' + S.encode() + b'","prompt_id":"p-1"}\n', 1792123878000),
    ("codex", "session_end", S, b'{"hook_event_name":"SessionEnd","session_id":"' + S.encode() + b'"}', 1792123800000),
]:
    dig, eid = event_id(client, kind, session, body, at)
    eid_cases.append({"client": client, "kind": kind, "session": session, "payload_utf8": body.decode(),
                      "at_ms": at, "expect": {"dig": dig, "event_id": eid}})
event_ids = {
    "formula": "dig = hex(sha256(bytes read))[..16]; event_id = hex(sha256('codepet-ev1|' + client + '|' + kind + '|' + session + '|' + dig + '|' + floor(at_ms / 3000)))[..32]",
    "notes": ["cases 1 and 2 share a 3 s bucket and dedupe; case 3 is the next bucket and does not"],
    "vectors": eid_cases,
}
assert eid_cases[0]["expect"]["event_id"] == eid_cases[1]["expect"]["event_id"]
assert eid_cases[1]["expect"]["event_id"] != eid_cases[2]["expect"]["event_id"]

# ---------------------------------------------------------------- interactivity (§6.4)
interactivity = {
    "policy": "positive headless evidence -> non-interactive; no evidence or /proc unreadable -> interactive",
    "hermes_platforms": ["cli", "tui", "desktop", "acp"],
    "rules": {
        "claude-code": "CLAUDE_CODE_ENTRYPOINT starts with 'sdk' -> false; else if the variable is absent, the nearest ancestor (depth <= 4) whose comm is 'claude' has an argv token exactly '-p' or '--print' -> false; else true",
        "codex": "nearest 'codex' ancestor (depth <= 4): argv[1] == 'e', or any argv token exactly 'exec' or 'review' -> false",
        "opencode": "the plugin's process.argv has the exact token 'run' or 'serve' after the binary -> false (sent as payload 'interactive')",
        "grok-cli": "session-mode field once WP7 confirms it; until then the nearest 'grok' ancestor (depth <= 4) has an exact token in headless_flags, a token starting with one of headless_prefixes, or the adjacent pair 'agent' 'headless' -> false. 'agent stdio' (IDE/ACP), 'agent serve' and 'agent leader' stay interactive",
        "hermes": "platform (remembered per session_id) not in hermes_platforms -> dropped by the plugin, never sent; background-review agents per WP7",
    },
    "grok_headless_flags": ["-p", "--single", "--prompt-file", "--prompt-json", "--output-format", "--json-schema"],
    "grok_headless_prefixes": ["--single=", "--prompt-file=", "--prompt-json=", "--output-format=", "--json-schema="],
    "grok_source": "grok 1.0.52 (0920c8169e27) `grok --help` and `grok agent --help`, read 2026-10-16 under a throwaway HOME",
    "vectors": [
        {"client": "claude-code", "env": {"CLAUDE_CODE_ENTRYPOINT": "cli"}, "ancestor_argv": ["claude", "-p", "x"], "expect": True},
        {"client": "claude-code", "env": {"CLAUDE_CODE_ENTRYPOINT": "sdk-cli"}, "ancestor_argv": ["claude"], "expect": False},
        {"client": "claude-code", "env": {"CLAUDE_CODE_ENTRYPOINT": "sdk-ts"}, "ancestor_argv": None, "expect": False},
        {"client": "claude-code", "env": {}, "ancestor_argv": ["claude", "--print", "hi"], "expect": False},
        {"client": "claude-code", "env": {}, "ancestor_argv": ["claude", "--prompt-p"], "expect": True},
        {"client": "claude-code", "env": {}, "ancestor_argv": ["claude", "fix -p bug"], "expect": True},
        {"client": "claude-code", "env": {}, "ancestor_argv": None, "expect": True},
        {"client": "codex", "env": {}, "ancestor_argv": ["codex", "exec", "hi"], "expect": False},
        {"client": "codex", "env": {}, "ancestor_argv": ["codex", "e", "hi"], "expect": False},
        {"client": "codex", "env": {}, "ancestor_argv": ["codex", "review"], "expect": False},
        {"client": "codex", "env": {}, "ancestor_argv": ["codex", "-m", "gpt", "e"], "expect": True},
        {"client": "codex", "env": {}, "ancestor_argv": ["codex", "app-server"], "expect": True},
        {"client": "codex", "env": {}, "ancestor_argv": ["codex", "explain exec please"], "expect": True},
        {"client": "codex", "env": {}, "ancestor_argv": None, "expect": True},
        {"client": "grok-cli", "env": {}, "ancestor_argv": ["grok", "-p", "hi"], "expect": False},
        {"client": "grok-cli", "env": {}, "ancestor_argv": ["grok", "--single=hi"], "expect": False},
        {"client": "grok-cli", "env": {}, "ancestor_argv": ["grok", "--output-format", "json"], "expect": False},
        {"client": "grok-cli", "env": {}, "ancestor_argv": ["grok", "agent", "headless"], "expect": False},
        {"client": "grok-cli", "env": {}, "ancestor_argv": ["grok", "agent", "stdio"], "expect": True},
        {"client": "grok-cli", "env": {}, "ancestor_argv": ["grok", "agent", "leader"], "expect": True},
        {"client": "grok-cli", "env": {}, "ancestor_argv": ["grok", "--agent", "headless"], "expect": True},
        {"client": "grok-cli", "env": {}, "ancestor_argv": ["grok", "fix it"], "expect": True},
        {"client": "opencode", "payload_interactive": False, "expect": False},
        {"client": "opencode", "payload_interactive": True, "expect": True},
        {"client": "opencode", "payload_interactive": None, "expect": True},
    ],
}

# ---------------------------------------------------------------- pure functions
turn_qualifies = {
    "rule": "matching turn was open AND (tools >= 1 OR duration_ms >= min_turn_ms) AND interactive AND NOT test",
    "vectors": [
        {"open": True, "tools": 1, "duration_ms": 500, "interactive": True, "test": False, "expect": True},
        {"open": True, "tools": 0, "duration_ms": 29999, "interactive": True, "test": False, "expect": False},
        {"open": True, "tools": 0, "duration_ms": 30000, "interactive": True, "test": False, "expect": True},
        {"open": False, "tools": 3, "duration_ms": 60000, "interactive": True, "test": False, "expect": False},
        {"open": True, "tools": 3, "duration_ms": 60000, "interactive": False, "test": False, "expect": False},
        {"open": True, "tools": 3, "duration_ms": 60000, "interactive": True, "test": True, "expect": False},
    ],
}

H = 3600000
caps = {
    "rule": "jar_full if jar >= jar_max; else capped_session if any earned_at of this session > at - cap_session_ms; else capped_hour if count(earned_at > at - 1 h) >= cap_per_hour; else capped_day if count(earned_at > at - 24 h) >= cap_per_day; else allow. Windows are half-open: an earn exactly one window ago no longer counts. Forgotten rows (earned_at 0) never count.",
    "vectors": [
        {"name": "empty", "at": 100 * H, "jar": 0, "session_earned": [], "all_earned": [], "expect": "allow"},
        {"name": "jar_full_beats_caps", "at": 100 * H, "jar": 10, "session_earned": [100 * H - 1000], "all_earned": [100 * H - 1000], "expect": "jar_full"},
        {"name": "session_30s", "at": 100 * H, "jar": 1, "session_earned": [100 * H - 10000], "all_earned": [100 * H - 10000], "expect": "capped_session"},
        {"name": "session_exactly_30s_is_allowed", "at": 100 * H, "jar": 1, "session_earned": [100 * H - 30000], "all_earned": [100 * H - 30000], "expect": "allow"},
        {"name": "hour_12", "at": 100 * H, "jar": 5, "session_earned": [],
         "all_earned": [100 * H - i * 60000 - 60000 for i in range(12)], "expect": "capped_hour"},
        {"name": "hour_11_plus_one_expired", "at": 100 * H, "jar": 5, "session_earned": [],
         "all_earned": [100 * H - i * 60000 - 60000 for i in range(11)] + [100 * H - H], "expect": "allow"},
        {"name": "day_48", "at": 100 * H, "jar": 5, "session_earned": [],
         "all_earned": [100 * H - 2 * H - i * 1200000 for i in range(48)], "expect": "capped_day"},
        {"name": "day_47_plus_forgotten", "at": 100 * H, "jar": 5, "session_earned": [],
         "all_earned": [100 * H - 2 * H - i * 1200000 for i in range(47)] + [0], "expect": "allow"},
    ],
}

# ---------------------------------------------------------------- session machine (§6.4 step 7-8)
def ev(at, kind, turn=None, tool=None, **kw):
    e = {"at": at, "kind": kind, "turn": turn, "tool": tool}
    e.update(kw)
    return e

sessions = {
    "model": {
        "defaults": {"session": "s1", "interactive": True, "test": False, "enabled": True, "life": True, "jar": 0,
                     "now": "each event folds with now = its at (no clamping in these vectors)"},
        "per_event_expect": "state of the vector's session after the event, 'earned' = a treat dropped by this event",
        "final_expect": "treats = treats earned in the vector; finished_seq = meta.finished_seq increments; open_turn = open turn key or null",
        "transitions": {
            "session_start": "state idle (an ended session restarts idle)",
            "turn_start": "a still-open turn closes neutrally (no treat, no finished_seq); open the new turn (tools 0, started at); state working",
            "activity": "(a) key == open turn: count a new distinct tool; waiting/idle -> working. (b) key == last done turn and at <= last_done_ms + late_tool_ms: count a new distinct tool toward the finished turn; if it earned nothing and was not re-checked, re-run the treat check once (duration = last_done_ms - its start). (c) key == last done turn, at > last_done_ms + late_tool_ms and a tool not seen in that turn: continuation - reopen the turn (same start, its tools + 1), state working. (d) otherwise only last_event_ms",
            "waiting": "state waiting (approval rows: their own row)",
            "idle_prompt": "state idle; the open turn stays open",
            "turn_done": "closes the open turn with the same key: state idle, finished_seq++ (interactive sessions only), treat check with tools = counted + payload tools. No matching open turn -> only last_event_ms",
            "turn_failed": "closes only the matching turn, no treat: state error, error_at_ms = at",
            "turn_cancelled": "closes only the matching turn, no treat: state idle. Keyless: closes the open turn only if it started before at",
            "session_end": "state ended; an open turn closes neutrally",
        },
        "approval_rows": "Hermes approval_key events use a separate row keyed by HMAC(approval_key): waiting -> waiting; activity with the same key -> idle; nothing else touches it",
        "non_interactive": "tracked (state machine runs) but never counts for the face, never increments finished_seq and never earns",
        "hello": "the first non-test event of a client since Connect sets first_seen_ms and hello_seq++ (any interactivity); see fold_vectors",
    },
    "vectors": [
        {"name": "claude_simple_turn_earns", "client": "claude-code",
         "events": [ev(0, "session_start", expect="idle"), ev(1000, "turn_start", "t1", expect="working"),
                    ev(2000, "activity", "t1", "a", expect="working"), ev(2500, "activity", "t1", "a", expect="working"),
                    ev(5000, "turn_done", "t1", expect="idle", earned=True), ev(6000, "session_end", expect="ended")],
         "final": {"treats": 1, "finished_seq": 1, "open_turn": None, "last_done_tools": 1}},
        {"name": "short_turn_without_tools_celebrates_without_treat", "client": "claude-code",
         "events": [ev(0, "turn_start", "t1", expect="working"), ev(10000, "turn_done", "t1", expect="idle")],
         "final": {"treats": 0, "finished_seq": 1, "open_turn": None, "last_done_tools": 0}},
        {"name": "thirty_second_turn_without_tools_earns", "client": "grok-cli",
         "events": [ev(0, "turn_start", "t1", expect="working"), ev(30000, "turn_done", "t1", expect="idle", earned=True)],
         "final": {"treats": 1, "finished_seq": 1, "open_turn": None, "last_done_tools": 0}},
        {"name": "stop_veto_continue_then_stop_one_treat", "client": "claude-code",
         "events": [ev(0, "turn_start", "t1", expect="working"), ev(1000, "activity", "t1", "a", expect="working"),
                    ev(5000, "turn_done", "t1", expect="idle", earned=True),
                    ev(6500, "activity", "t1", "b", expect="working", note="continuation (> late window, new tool)"),
                    ev(12000, "turn_done", "t1", expect="idle", note="X1: the turn already earned")],
         "final": {"treats": 1, "finished_seq": 2, "open_turn": None, "last_done_tools": 2}},
        {"name": "late_async_pretooluse_after_stop_counts_without_reopen", "client": "claude-code",
         "events": [ev(0, "turn_start", "t1", expect="working"), ev(1000, "activity", "t1", "a", expect="working"),
                    ev(3000, "turn_done", "t1", expect="idle", earned=True),
                    ev(3500, "activity", "t1", "b", expect="idle", note="late tool of the finished turn")],
         "final": {"treats": 1, "finished_seq": 1, "open_turn": None, "last_done_tools": 2}},
        {"name": "codex_stop_folded_before_its_posttooluse_late_treat", "client": "codex",
         "events": [ev(0, "turn_start", "7", expect="working"),
                    ev(5000, "turn_done", "7", expect="idle", note="0 tools, 5 s: no treat yet"),
                    ev(4900, "activity", "7", "call_1", expect="idle", earned=True,
                       note="arrives in a later batch; at <= last_done + 1 s; re-check earns")],
         "final": {"treats": 1, "finished_seq": 1, "open_turn": None, "last_done_tools": 1}},
        {"name": "grok_cancel_report_after_next_prompt_keeps_t2_open", "client": "grok-cli",
         "events": [ev(0, "turn_start", "g1", expect="working"),
                    ev(5000, "turn_start", "g2", expect="working", note="g1 closes neutrally"),
                    ev(5100, "turn_cancelled", "g1", expect="working", note="key mismatch: only last_event_ms"),
                    ev(6000, "activity", "g2", "tu1", expect="working"),
                    ev(9000, "turn_done", "g2", expect="idle", earned=True)],
         "final": {"treats": 1, "finished_seq": 1, "open_turn": None, "last_done_tools": 1}},
        {"name": "hermes_interrupted_then_loop_stopped_closes_once", "client": "hermes",
         "events": [ev(0, "session_start", expect="idle"), ev(100, "turn_start", "h1", expect="working"),
                    ev(2000, "turn_cancelled", "h1", expect="idle"),
                    ev(2100, "turn_cancelled", None, expect="idle", note="keyless, nothing open: no-op")],
         "final": {"treats": 0, "finished_seq": 0, "open_turn": None, "last_done_tools": 0}},
        {"name": "hermes_loop_stopped_alone_closes_the_open_turn", "client": "hermes",
         "events": [ev(100, "turn_start", "h1", expect="working"),
                    ev(2100, "turn_cancelled", None, expect="idle", note="keyless; turn started before it")],
         "final": {"treats": 0, "finished_seq": 0, "open_turn": None, "last_done_tools": 0}},
        {"name": "second_permission_prompt_in_the_same_turn_waits_again", "client": "claude-code",
         "events": [ev(0, "turn_start", "t1", expect="working"), ev(1000, "waiting", "t1", expect="waiting"),
                    ev(2000, "activity", "t1", None, expect="working"), ev(3000, "waiting", "t1", expect="waiting")],
         "final": {"treats": 0, "finished_seq": 0, "open_turn": "t1", "last_done_tools": 0}},
        {"name": "opencode_restart_reusing_counters_gets_distinct_keys", "client": "opencode", "session": "ses_1",
         "events": [ev(1000, "turn_start", "1000-1", expect="working"),
                    ev(5000, "turn_done", "1000-1", tools=2, expect="idle", earned=True),
                    ev(60000, "session_start", expect="idle", note="OpenCode restarted; same session resumed"),
                    ev(61000, "turn_start", "60000-1", expect="working"),
                    ev(65000, "turn_done", "60000-1", tools=1, expect="idle", earned=True)],
         "final": {"treats": 2, "finished_seq": 2, "open_turn": None, "last_done_tools": 1}},
        {"name": "hermes_approval_key_waiting_then_activity_clears", "client": "hermes",
         "events": [ev(0, "turn_start", "h1", expect="working"),
                    ev(1000, "waiting", None, akey="k1", expect="working", expect_approval="waiting"),
                    ev(3000, "activity", None, akey="k1", expect="working", expect_approval="idle")],
         "final": {"treats": 0, "finished_seq": 0, "open_turn": "h1", "last_done_tools": 0, "approval_rows": {"k1": "idle"}}},
        {"name": "failed_turn_no_treat_error_state", "client": "claude-code",
         "events": [ev(0, "turn_start", "t1", expect="working"), ev(1000, "activity", "t1", "a", expect="working"),
                    ev(2000, "turn_failed", "t1", expect="error")],
         "final": {"treats": 0, "finished_seq": 0, "open_turn": None, "last_done_tools": 0, "error_at_ms": 2000}},
        {"name": "cancelled_turn_is_neutral", "client": "codex",
         "events": [ev(0, "turn_start", "7", expect="working"), ev(1000, "activity", "7", "c1", expect="working"),
                    ev(2000, "turn_cancelled", "7", expect="idle")],
         "final": {"treats": 0, "finished_seq": 0, "open_turn": None, "last_done_tools": 0}},
        {"name": "non_interactive_never_counts_or_earns", "client": "claude-code", "interactive": False,
         "events": [ev(0, "turn_start", "t1", expect="working"), ev(1000, "activity", "t1", "a", expect="working"),
                    ev(40000, "turn_done", "t1", expect="idle")],
         "final": {"treats": 0, "finished_seq": 0, "open_turn": None, "last_done_tools": 1}},
        {"name": "duplicate_stop_after_close_is_a_noop", "client": "claude-code",
         "events": [ev(0, "turn_start", "t1", expect="working"), ev(1000, "activity", "t1", "a", expect="working"),
                    ev(5000, "turn_done", "t1", expect="idle", earned=True), ev(5200, "turn_done", "t1", expect="idle")],
         "final": {"treats": 1, "finished_seq": 1, "open_turn": None, "last_done_tools": 1}},
        {"name": "turn_start_closes_the_previous_turn_neutrally", "client": "claude-code",
         "events": [ev(0, "turn_start", "t1", expect="working"), ev(1000, "activity", "t1", "a", expect="working"),
                    ev(40000, "turn_start", "t2", expect="working"), ev(41000, "turn_done", "t2", expect="idle")],
         "final": {"treats": 0, "finished_seq": 1, "open_turn": None, "last_done_tools": 0}},
        {"name": "session_end_closes_an_open_turn_neutrally", "client": "grok-cli",
         "events": [ev(0, "turn_start", "g1", expect="working"), ev(1000, "activity", "g1", "x", expect="working"),
                    ev(2000, "session_end", expect="ended")],
         "final": {"treats": 0, "finished_seq": 0, "open_turn": None, "last_done_tools": 0}},
        {"name": "idle_prompt_is_never_waiting", "client": "claude-code",
         "events": [ev(0, "turn_start", "t1", expect="working"), ev(31000, "turn_done", "t1", expect="idle", earned=True),
                    ev(91000, "idle_prompt", expect="idle")],
         "final": {"treats": 1, "finished_seq": 1, "open_turn": None, "last_done_tools": 0}},
        {"name": "second_turn_within_30s_is_capped_but_celebrates", "client": "claude-code",
         "events": [ev(0, "turn_start", "t1", expect="working"), ev(1000, "activity", "t1", "a", expect="working"),
                    ev(2000, "turn_done", "t1", expect="idle", earned=True),
                    ev(3000, "turn_start", "t2", expect="working"), ev(4000, "activity", "t2", "b", expect="working"),
                    ev(10000, "turn_done", "t2", expect="idle", note="capped_session")],
         "final": {"treats": 1, "finished_seq": 2, "open_turn": None, "last_done_tools": 1}},
        {"name": "full_jar_celebrates_without_a_treat", "client": "codex", "jar": 10,
         "events": [ev(0, "turn_start", "7", expect="working"), ev(1000, "activity", "7", "c1", expect="working"),
                    ev(2000, "turn_done", "7", expect="idle", note="jar_full -> finished_jar_full")],
         "final": {"treats": 0, "finished_seq": 1, "open_turn": None, "last_done_tools": 1, "finished_jar_full": True}},
    ],
}

# ---------------------------------------------------------------- fold drops (§6.4 steps 2-6)
NOW = 1792123900000
fold = {
    "rule": "outcomes in order: duplicate (dedupe_key exists: unlink, no row change) -> dropped_off (meta.enabled = 0) -> dropped_old (at < now - max_event_age_ms) -> clamp (at > now + future_clamp_ms -> at = now, still folded) -> dropped_child -> test (only test_state/test_at_ms) -> client record (+hello) -> no_life (stop) -> folded",
    "now": NOW,
    "vectors": [
        {"name": "off_drops", "enabled": False, "life": True, "event": {"at": NOW, "kind": "turn_start", "turn": "t"},
         "expect": {"outcome": "dropped_off", "client_record": False, "hello": False}},
        {"name": "too_old_drops", "enabled": True, "life": True, "event": {"at": NOW - 86400001, "kind": "turn_start", "turn": "t"},
         "expect": {"outcome": "dropped_old", "client_record": False, "hello": False}},
        {"name": "exactly_24h_is_kept", "enabled": True, "life": True, "event": {"at": NOW - 86400000, "kind": "turn_start", "turn": "t"},
         "expect": {"outcome": "folded", "client_record": True, "hello": True, "at": NOW - 86400000}},
        {"name": "future_is_clamped", "enabled": True, "life": True, "event": {"at": NOW + 120001, "kind": "turn_start", "turn": "t"},
         "expect": {"outcome": "folded", "client_record": True, "hello": True, "at": NOW}},
        {"name": "near_future_is_kept", "enabled": True, "life": True, "event": {"at": NOW + 120000, "kind": "turn_start", "turn": "t"},
         "expect": {"outcome": "folded", "client_record": True, "hello": True, "at": NOW + 120000}},
        {"name": "test_event_only_drives_agent_test", "enabled": True, "life": True,
         "event": {"at": NOW, "kind": "turn_done", "turn": "t", "test": True},
         "expect": {"outcome": "test", "client_record": False, "hello": False, "test_state": "done"}},
        {"name": "test_turn_start_is_running", "enabled": True, "life": True,
         "event": {"at": NOW, "kind": "turn_start", "turn": "t", "test": True},
         "expect": {"outcome": "test", "client_record": False, "hello": False, "test_state": "running"}},
        {"name": "no_life_still_greets", "enabled": True, "life": False, "event": {"at": NOW, "kind": "session_start"},
         "expect": {"outcome": "no_life", "client_record": True, "hello": True}},
        {"name": "second_event_does_not_greet_again", "enabled": True, "life": True, "first_seen": True,
         "event": {"at": NOW, "kind": "activity", "turn": "t", "tool": "x"},
         "expect": {"outcome": "folded", "client_record": True, "hello": False}},
    ],
}

# ---------------------------------------------------------------- pet state (§6.4 shared priority)
priority = {
    "order": [
        {"row": 1, "state": "off", "when": "enabled == false", "bar": "off", "line": "off.line", "overlay": "none"},
        {"row": 1.5, "state": "no_pet", "when": "life == null (first run, or after a goodbye/death)", "bar": "egg", "line": None, "overlay": "none"},
        {"row": 2, "state": "fainted|asleep", "when": "life.fainted, else care.sleeping", "bar": "sleep (+bandage)", "line": "mood.fainted|mood.asleep", "overlay": "sleep"},
        {"row": 3, "state": "waiting", "when": "a counting app is waiting", "bar": "hand (pulse)", "line": "act.waiting", "overlay": "waiting"},
        {"row": 4, "state": "need_now", "when": "a need is empty (now) or critical", "bar": "need badge", "line": "mood.<need>.now|critical", "overlay": "needs_care"},
        {"row": 5, "state": "working", "when": "a counting app is working", "bar": "working frame", "line": "act.working.one|two|many", "overlay": "working"},
        {"row": 6, "state": "celebrate", "when": "a new test done, hello or finished_seq inside its window", "bar": "star", "line": "act.ready|act.hello|act.done*", "overlay": "celebrate"},
        {"row": 7, "state": "oops", "when": "error_at_ms within error_recent_ms", "bar": "none (star if treats wait)", "line": "act.oops", "overlay": "error for error_frame_ms, then idle"},
        {"row": 8, "state": "need_soon", "when": "a need is low (soon)", "bar": "none (star if treats wait)", "line": "mood.<need>.soon", "overlay": "needs_care_frame"},
        {"row": 9, "state": "idle", "when": "otherwise", "bar": "none (star if treats wait)", "line": "mood.happy", "overlay": "wander"},
    ],
    "need_levels": {"ok": "happy", "low": "soon", "empty": "now", "critical": "critical"},
    "need_priority": ["food", "water", "play", "wash"],
    "need_pick": "among needs at the highest present group (now/critical first, then soon), pick by need_priority; the line uses that need's own level",
    "counting_app": "an agent.apps entry whose stale_at_ms > now",
    "app_order": "agent.apps order (since_ms ascending, then client order); {app} is the first app in the row's set, {app2} the second",
    "waiting_app": "the waiting app with the latest since_ms (ties: client order)",
    "celebrate": {
        "candidates_in_order": ["test", "hello", "finished"],
        "test": "agent.test.state == 'done' and test.at_ms > seen.test_at_ms and now < at_ms + celebrate_window_ms",
        "hello": "hello_seq > seen.hello_seq and now < hello_at_ms + celebrate_window_ms",
        "finished": "finished_seq > seen.finished_seq and now >= finished_at_ms + celebrate_grace_ms and now < finished_at_ms + celebrate_window_ms and finished_app is not a counting working app",
        "lines": {"test": "act.ready", "hello": "act.hello", "finished": "act.done_full if finished_jar_full, else act.done_treat if finished_treat, else act.done"},
        "seen": "the UI records the celebrated seq/at and passes it back as `seen`",
    },
    "treats_visible": "treats.jar > 0 and life awake (not care.sleeping) and not fainted and enabled",
    "star_badge": "rows 7-9 show the star badge when treats_visible",
    "wake_at_ms": "null when Off. Otherwise the minimum of these future times (null when none), whatever row wins: stale_at_ms of each counting app; for each unseen celebration candidate whose window is still open, its grace end if still ahead, else its window end; the error frame end and the oops end while ahead",
    "vars": "exactly the placeholders of the line template; {app}/{app2} are client ids (the UI maps them to display names)",
    "bubble": {"waiting": "apps: [waiting app]", "need_now": "need: <need>", "working": "apps: up to 3 working apps", "fainted": "bandage", "celebrate": "star", "need_soon": "need: <need>", "idle": "helpers when helpers_busy, else none"},
}

def pv(name, fixture, now, expect, seen=None):
    return {"name": name, "fixture": fixture, "now": now,
            "seen": seen or {"finished_seq": 0, "hello_seq": 0, "test_at_ms": 0}, "expect": expect}

def out(state, bar, line, overlay, vars=None, pulse=False, treats=False, bubble=None, wake=None):
    return {"state": state, "bar": bar, "line": line, "vars": vars or {}, "overlay": overlay, "pulse": pulse,
            "treats_visible": treats, "bubble": bubble, "wake_at_ms": wake}

T = NOW  # fixtures are generated at NOW (see fixtures/bar-v2)
priority["vectors"] = [
    pv("fresh_install_no_pet", "fresh-no-pet", T, out("no_pet", "egg", None, "none")),
    pv("off_wins_over_everything", "off", T, out("off", "off", "off.line", "none", {"name": "Ginger"})),
    pv("off_before_any_pet", "off-no-pet", T, out("off", "off", None, "none")),
    pv("idle_happy", "idle-happy", T, out("idle", "none", "mood.happy", "wander", {"name": "Ginger"})),
    pv("idle_with_treats_shows_star", "idle-treats", T, out("idle", "star", "mood.happy", "wander", {"name": "Ginger"}, treats=True)),
    pv("need_soon", "need-food-soon", T, out("need_soon", "none", "mood.food.soon", "needs_care_frame", {"name": "Ginger"}, bubble={"need": "food"})),
    pv("need_now", "need-water-now", T, out("need_now", "need_water", "mood.water.now", "needs_care", {"name": "Ginger"}, bubble={"need": "water"})),
    pv("critical_food", "need-food-critical", T, out("need_now", "need_food", "mood.food.critical", "needs_care", {"name": "Ginger"}, bubble={"need": "food"})),
    pv("several_needs_priority_order", "several-needs", T, out("need_now", "need_water", "mood.water.now", "needs_care", {"name": "Ginger"}, bubble={"need": "water"})),
    pv("asleep", "asleep", T, out("asleep", "sleep", "mood.asleep", "sleep", {"name": "Ginger"})),
    pv("fainted_beats_waiting", "fainted", T, out("fainted", "sleep_bandage", "mood.fainted", "sleep", {"name": "Ginger"}, bubble={"bandage": True}, wake=T + 1790000)),
    pv("working_one", "working-one", T, out("working", "working", "act.working.one", "working", {"name": "Ginger", "app": "claude-code"}, bubble={"apps": ["claude-code"]}, wake=T + 540000)),
    pv("working_two", "working-two", T, out("working", "working", "act.working.two", "working", {"name": "Ginger", "app": "claude-code", "app2": "grok-cli"}, bubble={"apps": ["claude-code", "grok-cli"]}, wake=T + 540000)),
    pv("working_many", "working-many", T, out("working", "working", "act.working.many", "working", {"name": "Ginger"}, bubble={"apps": ["claude-code", "opencode", "grok-cli"]}, wake=T + 540000)),
    pv("waiting_beats_need_now_and_working", "waiting", T, out("waiting", "hand", "act.waiting", "waiting", {"app": "codex"}, pulse=True, bubble={"apps": ["codex"]}, wake=T + 540000)),
    pv("working_beats_celebration", "finished-while-working", T, out("working", "working", "act.working.one", "working", {"name": "Ginger", "app": "claude-code"}, bubble={"apps": ["claude-code"]}, wake=T + 20000)),
    pv("finished_inside_grace_waits", "finished-treat", T - 25000 + 1000, out("idle", "star", "mood.happy", "wander", {"name": "Ginger"}, treats=True, wake=T - 25000 + 2000)),
    pv("finished_treat_celebrates", "finished-treat", T, out("celebrate", "star", "act.done_treat", "celebrate", {"name": "Ginger"}, treats=True, bubble={"star": True}, wake=T + 5000)),
    pv("finished_already_seen", "finished-treat", T, out("idle", "star", "mood.happy", "wander", {"name": "Ginger"}, treats=True), seen={"finished_seq": 5, "hello_seq": 0, "test_at_ms": 0}),
    pv("finished_window_over", "finished-treat", T + 5000, out("idle", "star", "mood.happy", "wander", {"name": "Ginger"}, treats=True)),
    pv("finished_jar_full", "jar-full", T, out("celebrate", "star", "act.done_full", "celebrate", {}, treats=True, bubble={"star": True}, wake=T + 20000)),
    pv("hello_first_event", "hello", T, out("celebrate", "star", "act.hello", "celebrate", {"name": "Ginger", "app": "claude-code"}, bubble={"star": True}, wake=T + 25000)),
    pv("ready_check_done", "ready-check-done", T, out("celebrate", "star", "act.ready", "celebrate", {"name": "Ginger"}, bubble={"star": True}, wake=T + 28500)),
    pv("oops_frame", "oops", T, out("oops", "none", "act.oops", "error", {"app": "grok-cli"}, wake=T + 2000)),
    pv("oops_after_frame", "oops", T + 2000, out("oops", "none", "act.oops", "idle", {"app": "grok-cli"}, wake=T + 119000)),
    pv("oops_over", "oops", T + 119000, out("idle", "none", "mood.happy", "wander", {"name": "Ginger"})),
    pv("stale_app_no_longer_counts", "stale-working", T, out("idle", "none", "mood.happy", "wander", {"name": "Ginger"})),
    pv("helpers_busy_bubble_only", "helpers-busy", T, out("idle", "none", "mood.happy", "wander", {"name": "Ginger"}, bubble={"helpers": True})),
    pv("owner_goldfish_ginger_aquatic_need", "goldfish-ginger-owner", T, out("need_soon", "none", "mood.wash.soon", "needs_care_frame", {"name": "Ginger"}, bubble={"need": "wash"})),
    pv("robot_needs_charge", "robot-bolt", T, out("need_now", "need_food", "mood.food.now", "needs_care", {"name": "Bolt"}, bubble={"need": "food"})),
]

doc = OrderedDict([
    ("schema", "codepet.agent_vectors.v1"),
    ("spec", "SPEC-v0.3 §5.8, §6.4, §8, §9.1"),
    ("consumers", "Rust codepet-core::agent (WP2) and the generated petstate.js via test_state_machine.mjs / test_bar_model.mjs (WP6); replayed by docs/v0.3/contracts/check_contracts.py"),
    ("clients", clients),
    ("kinds", kinds),
    ("session_states", ["idle", "working", "waiting", "error", "ended"]),
    ("app_states_rising", ["idle", "error", "working", "waiting"]),
    ("constants", constants),
    ("normalization", normalization),
    ("event_ids", event_ids),
    ("interactivity", interactivity),
    ("turn_qualifies", turn_qualifies),
    ("caps", caps),
    ("sessions", sessions),
    ("fold", fold),
    ("priority", priority),
])
with open(OUT, "w") as f:
    json.dump(doc, f, indent=1, ensure_ascii=False)
    f.write("\n")
