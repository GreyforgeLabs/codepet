#!/usr/bin/env python3
"""Writes docs/v0.3/contracts/agent_vectors.json (WP0). Expectations are hand-written here;
check_contracts.py replays them through an independent reference model. Run from anywhere:
    python3 docs/v0.3/contracts/tools/gen_agent_vectors.py
"""
import hashlib
import hmac
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
NONCE = "0f1e2d3c4b5a69788796a5b4c3d2e1f0"
READY = "codepet-ready-check"
normalization = {
    "rules": [
        "argv client id is authoritative; the payload only passes a per-client schema sanity check",
        "only top-level keys are read (depth 1); nested values are skipped unbuffered",
        "id fields: strings <= 128 bytes of [A-Za-z0-9._:-], else the event is dropped (bad_id)",
        "agent_id / subagentType present -> dropped before any write (child); this check runs first",
        "codepet_test present -> the HOOK compares it (constant time) with $STATE/test.nonce (read with O_NOFOLLOW, regular file owned by the user, <= 64 bytes, trailing newline stripped). Equal -> the inbox record carries test:true. Missing nonce file, a different value, or a value that is not 32 lowercase hex -> the event is dropped (test_nonce) and never folded as a real event. Without codepet_test -> test:false. The fold never sees the nonce; it trusts the 0700 inbox",
        "unmapped event names or notification types -> dropped (unmapped)",
        "Grok: prefer hook_event_name (PascalCase); when only hookEventName (snake_case) is present it maps through grok_snake_event_names; camelCase ids (sessionId, promptId, toolUseId, notificationType, toolName)",
        "error: one of rate_limit|authentication_failed|invalid_request|server_error|max_output_tokens|overloaded|billing_error|unknown, else unknown",
        "synthesized payloads (OpenCode, Hermes; plugin-payload.schema.json): codepet_v must be the integer 1 and event one of the nine kinds, else unmapped. tools: an integer 0..=1000 (not a boolean), anything else is ignored (tools 0) and the event is kept. interactive: a boolean; anything else, or absent, is true. approval_key is read only for hermes, only with event waiting or activity and only without session_id (else bad_id / unmapped); OpenCode's approval_key is never read. completed/failed/interrupted are not on the allow-list: the Hermes plugin maps on_session_end itself and sends the kind",
    ],
    "grok_snake_event_names": {"session_start": "SessionStart", "user_prompt_submit": "UserPromptSubmit",
                               "pre_tool_use": "PreToolUse", "notification": "Notification", "stop": "Stop",
                               "stop_failure": "StopFailure", "stop_cancelled": "StopCancelled",
                               "session_end": "SessionEnd"},
    "drop_reasons": ["child", "test_nonce", "unmapped", "bad_id"],
    "nonce_context": "each vector's 'nonce' is the content of $STATE/test.nonce when the hook runs (absent key or null = no file)",
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
        # ---- Claude: every waiting notification type
        n("claude-code", {"hook_event_name": "Notification", "session_id": S, "notification_type": "elicitation_dialog"},
          {"kind": "waiting", "session": S, "turn": None, "tool": None, "class": "elicitation_dialog"}),
        n("claude-code", {"hook_event_name": "Notification", "session_id": S,
                          "notification_type": "elicitation_url_dialog"},
          {"kind": "waiting", "session": S, "turn": None, "tool": None, "class": "elicitation_url_dialog"}),
        n("claude-code", {"hook_event_name": "Notification", "session_id": S, "notification_type": "agent_needs_input"},
          {"kind": "waiting", "session": S, "turn": None, "tool": None, "class": "agent_needs_input"}),
        # ---- Grok: the rest of §5.4
        n("grok-cli", {"hookEventName": "session_start", "hook_event_name": "SessionStart", "sessionId": S,
                       "source": "new"},
          {"kind": "session_start", "session": S, "turn": None, "tool": None, "class": None}),
        n("grok-cli", {"hookEventName": "notification", "hook_event_name": "Notification", "sessionId": S,
                       "notificationType": "idle_prompt"},
          {"kind": "idle_prompt", "session": S, "turn": None, "tool": None, "class": "idle_prompt"}),
        n("grok-cli", {"hookEventName": "stop", "hook_event_name": "Stop", "sessionId": S, "reason": "shutdown"},
          {"kind": "session_end", "session": S, "turn": None, "tool": None, "class": None}),
        n("grok-cli", {"hookEventName": "stop_failure", "hook_event_name": "StopFailure", "sessionId": S,
                       "promptId": "g-1", "error": "server_error"},
          {"kind": "turn_failed", "session": S, "turn": "g-1", "tool": None, "class": "server_error"}),
        n("grok-cli", {"hookEventName": "session_end", "hook_event_name": "SessionEnd", "sessionId": S,
                       "reason": "exit"},
          {"kind": "session_end", "session": S, "turn": None, "tool": None, "class": None}),
        n("grok-cli", {"hookEventName": "pre_tool_use", "hook_event_name": "PreToolUse", "sessionId": S,
                       "promptId": "g-1", "toolName": "ask_user_question", "toolUseId": "tu3"},
          {"kind": "waiting", "session": S, "turn": "g-1", "tool": "tu3", "class": "ask_user"}),
        n("grok-cli", {"hookEventName": "stop_failure", "sessionId": S, "promptId": "g-2", "error": "rate_limit"},
          {"kind": "turn_failed", "session": S, "turn": "g-2", "tool": None, "class": "rate_limit"}),
        n("grok-cli", {"hookEventName": "user_prompt_submit", "sessionId": S, "promptId": "g-3"},
          {"kind": "turn_start", "session": S, "turn": "g-3", "tool": None, "class": None}),
        n("grok-cli", {"hookEventName": "teleport", "sessionId": S}, {"drop": "unmapped"}),
        # ---- OpenCode
        n("opencode", {"codepet_v": 1, "event": "turn_start", "session_id": "ses_1", "turn_id": "1792-4",
                       "interactive": True},
          {"kind": "turn_start", "session": "ses_1", "turn": "1792-4", "tool": None, "class": None, "tools": 0,
           "interactive": True, "test": False}),
        n("opencode", {"codepet_v": 1, "event": "turn_start", "session_id": "ses_2", "turn_id": "1792-1",
                       "interactive": False},
          {"kind": "turn_start", "session": "ses_2", "turn": "1792-1", "interactive": False}),
        n("opencode", {"codepet_v": 1, "event": "turn_done", "session_id": "ses_1", "turn_id": "1792-4", "tools": "4"},
          {"kind": "turn_done", "tools": 0, "interactive": True}),
        n("opencode", {"codepet_v": 1, "event": "turn_done", "session_id": "ses_1", "turn_id": "1792-4", "tools": -1},
          {"kind": "turn_done", "tools": 0}),
        n("opencode", {"codepet_v": 1, "event": "turn_done", "session_id": "ses_1", "turn_id": "1792-4",
                       "tools": 1001},
          {"kind": "turn_done", "tools": 0}),
        n("opencode", {"codepet_v": 1, "event": "turn_done", "session_id": "ses_1", "turn_id": "1792-4",
                       "tools": 1000},
          {"kind": "turn_done", "tools": 1000}),
        n("opencode", {"codepet_v": 1, "event": "turn_done", "session_id": "ses_1", "turn_id": "1792-4", "tools": True},
          {"kind": "turn_done", "tools": 0}),
        n("opencode", {"codepet_v": 1, "event": "turn_start", "session_id": "ses_1", "interactive": "no"},
          {"kind": "turn_start", "interactive": True}),
        n("opencode", {"codepet_v": 2, "event": "turn_done", "session_id": "ses_1"}, {"drop": "unmapped"}),
        n("opencode", {"codepet_v": "1", "event": "turn_done", "session_id": "ses_1"}, {"drop": "unmapped"}),
        n("opencode", {"codepet_v": True, "event": "turn_done", "session_id": "ses_1"}, {"drop": "unmapped"}),
        n("opencode", {"event": "turn_done", "session_id": "ses_1"}, {"drop": "unmapped"}),
        n("opencode", {"codepet_v": 1, "event": "waiting", "session_id": "ses_1", "approval_key": "k1"},
          {"kind": "waiting", "session": "ses_1", "akey": None}),
        n("opencode", {"codepet_v": 1, "event": "activity", "approval_key": "k1"}, {"drop": "bad_id"}),
        # ---- Hermes
        n("hermes", {"codepet_v": 1, "event": "activity", "approval_key": "sk-cli-1"},
          {"kind": "activity", "session": None, "turn": None, "tool": None, "class": None, "akey": "sk-cli-1"}),
        n("hermes", {"codepet_v": 1, "event": "turn_done", "session_id": "h_1", "turn_id": "h-2", "tools": 3},
          {"kind": "turn_done", "session": "h_1", "turn": "h-2", "tools": 3, "akey": None}),
        n("hermes", {"codepet_v": 1, "event": "turn_failed", "session_id": "h_1", "turn_id": "h-2"},
          {"kind": "turn_failed", "session": "h_1", "turn": "h-2", "tool": None, "class": None}),
        n("hermes", {"codepet_v": 1, "event": "turn_start", "session_id": "h_1", "turn_id": "h-3", "completed": True,
                     "failed": True},
          {"kind": "turn_start", "session": "h_1", "turn": "h-3"}),
        n("hermes", {"codepet_v": 1, "event": "turn_done", "approval_key": "sk-cli-1"}, {"drop": "unmapped"}),
        n("hermes", {"codepet_v": 1, "event": "waiting", "session_id": "h_1", "approval_key": "sk-cli-1"},
          {"drop": "bad_id"}),
        n("hermes", {"codepet_v": 1, "event": "waiting"}, {"drop": "bad_id"}),
        # ---- the ready check (codepet_test; commands.json "ready_check" payloads)
        dict(n("claude-code", {"hook_event_name": "UserPromptSubmit", "session_id": READY, "prompt_id": READY,
                               "codepet_test": NONCE},
               {"kind": "turn_start", "session": READY, "turn": READY, "test": True}), nonce=NONCE),
        dict(n("claude-code", {"hook_event_name": "Stop", "session_id": READY, "prompt_id": READY,
                               "codepet_test": "ffffffffffffffffffffffffffffffff"},
               {"drop": "test_nonce"}), nonce=NONCE),
        dict(n("claude-code", {"hook_event_name": "Stop", "session_id": READY, "prompt_id": READY,
                               "codepet_test": NONCE},
               {"drop": "test_nonce"}), nonce=None),
        dict(n("codex", {"hook_event_name": "Stop", "session_id": READY, "turn_id": READY, "codepet_test": "../x"},
               {"drop": "test_nonce"}), nonce=NONCE),
        dict(n("codex", {"hook_event_name": "Stop", "session_id": READY, "turn_id": READY,
                         "codepet_test": NONCE.upper()},
               {"drop": "test_nonce"}), nonce=NONCE),
        dict(n("opencode", {"codepet_v": 1, "event": "turn_done", "session_id": READY, "turn_id": READY,
                            "codepet_test": NONCE},
               {"kind": "turn_done", "session": READY, "test": True, "tools": 0}), nonce=NONCE),
        dict(n("grok-cli", {"hook_event_name": "Stop", "sessionId": READY, "promptId": READY, "reason": "end_turn",
                            "codepet_test": NONCE},
               {"kind": "turn_done", "test": True}), nonce=NONCE),
        dict(n("claude-code", {"hook_event_name": "Stop", "session_id": S, "prompt_id": "p-9"},
               {"kind": "turn_done", "test": False}), nonce=NONCE),
        dict(n("claude-code", {"hook_event_name": "PreToolUse", "session_id": S, "agent_id": "a1",
                               "codepet_test": "bad"},
               {"drop": "child"}), nonce=NONCE),
    ],
}
# Payloads the OpenCode/Hermes plugins may emit validate against plugin-payload.schema.json;
# the lenient-reader vectors (bad tools, bad codepet_v, ...) deliberately do not.
for vec in normalization["vectors"]:
    if vec["client"] in ("opencode", "hermes"):
        p = vec["payload"]
        t = p.get("tools", 0)
        vec["plugin_valid"] = (
            "drop" not in vec["expect"] and type(t) is int and 0 <= t <= 1000
            and isinstance(p.get("interactive", True), bool)
            and set(p) <= {"codepet_v", "event", "session_id", "turn_id", "tools", "interactive", "approval_key",
                           "codepet_test"}
            and not (vec["client"] == "opencode" and "approval_key" in p))


# Idempotent state changes: a repeat is never a duplicate (a second permission prompt of the
# same tool has byte-identical payload), so their bucket is the millisecond itself.
STATE_KINDS = ["session_start", "waiting", "idle_prompt", "session_end"]


def event_bucket(kind, at_ms):
    return at_ms if kind in STATE_KINDS else at_ms // 3000


def event_id(client, kind, session, body, at_ms):
    dig = hashlib.sha256(body).hexdigest()[:16]
    key = f"codepet-ev1|{client}|{kind}|{session}|{dig}|{event_bucket(kind, at_ms)}"
    return dig, hashlib.sha256(key.encode()).hexdigest()[:32]


eid_cases = []
for client, kind, session, body, at in [
    ("claude-code", "turn_done", S, b'{"hook_event_name":"Stop","session_id":"' + S.encode() + b'","prompt_id":"p-1"}\n', 1792123876111),
    ("claude-code", "turn_done", S, b'{"hook_event_name":"Stop","session_id":"' + S.encode() + b'","prompt_id":"p-1"}\n', 1792123877999),
    ("claude-code", "turn_done", S, b'{"hook_event_name":"Stop","session_id":"' + S.encode() + b'","prompt_id":"p-1"}\n', 1792123878000),
    ("codex", "session_end", S, b'{"hook_event_name":"SessionEnd","session_id":"' + S.encode() + b'"}', 1792123800000),
    ("hermes", "waiting", "~ak:sk-cli-1", b'{"codepet_v":1,"event":"waiting","approval_key":"sk-cli-1"}\n', 1792123800000),
    ("claude-code", "waiting", S, b'{"hook_event_name":"Notification","session_id":"' + S.encode() + b'","notification_type":"permission_prompt","message":"Claude needs your permission to use Bash"}\n', 1792123876111),
    ("claude-code", "waiting", S, b'{"hook_event_name":"Notification","session_id":"' + S.encode() + b'","notification_type":"permission_prompt","message":"Claude needs your permission to use Bash"}\n', 1792123876112),
]:
    dig, eid = event_id(client, kind, session, body, at)
    eid_cases.append({"client": client, "kind": kind, "session": session, "payload_utf8": body.decode(),
                      "at_ms": at, "expect": {"dig": dig, "event_id": eid}})
event_ids = {
    "formula": "dig = hex(sha256(bytes read))[..16]; event_id = hex(sha256('codepet-ev1|' + client + '|' + kind + '|' + session + '|' + dig + '|' + bucket))[..32], bucket in decimal",
    "bucket": "floor(at_ms / 3000) for every kind except state_kinds, whose bucket is at_ms itself",
    "state_kinds": STATE_KINDS,
    "state_kinds_reason": "idempotent state changes: a real duplicate delivery of one does no harm, but a legitimate repeat can be byte-identical (Claude Code's permission Notification carries no turn or tool id and a generic message, Codex PermissionRequest repeats the same command), so a 3 s bucket would swallow the second 'needs you'. Amended after the WP2 review (notes.md §5)",
    "session_field": "the raw session id; for a Hermes approval event (no session_id) the literal '~ak:' + approval_key ('~' never occurs in an id, so it cannot collide)",
    "notes": ["cases 1 and 2 share a 3 s bucket and dedupe; case 3 is the next bucket and does not",
              "case 5 is a Hermes approval event (session field '~ak:<approval_key>')",
              "cases 6 and 7 are two byte-identical permission prompts 1 ms apart in one 3 s bucket: distinct ids, both fold"],
    "vectors": eid_cases,
}
assert eid_cases[0]["expect"]["event_id"] == eid_cases[1]["expect"]["event_id"]
assert eid_cases[1]["expect"]["event_id"] != eid_cases[2]["expect"]["event_id"]
assert eid_cases[5]["expect"]["dig"] == eid_cases[6]["expect"]["dig"]
assert eid_cases[5]["expect"]["event_id"] != eid_cases[6]["expect"]["event_id"]

# ---------------------------------------------------------------- interactivity (§6.4)
interactivity = {
    "policy": "positive headless evidence -> non-interactive; no evidence or /proc unreadable -> interactive",
    "hermes_platforms": ["cli", "tui", "desktop", "acp"],
    "rules": {
        "claude-code": "CLAUDE_CODE_ENTRYPOINT starts with 'sdk' -> false; else if the variable is absent, the nearest ancestor (depth <= 4) whose comm is 'claude' has an argv token exactly '-p' or '--print' -> false; else true",
        "codex": "nearest 'codex' ancestor (depth <= 4): argv[1] == 'e', or any argv token exactly 'exec' or 'review' -> false",
        "opencode": "the plugin's process.argv has the exact token 'run' or 'serve' after the binary -> false (sent as payload 'interactive')",
        "grok-cli": "session-mode field once WP7 confirms it; until then the nearest 'grok' ancestor (depth <= 4) has an exact token in headless_flags, a token starting with one of headless_prefixes, or the adjacent pair 'agent' 'headless' -> false. 'agent stdio' (IDE/ACP), 'agent serve' and 'agent leader' stay interactive",
        "hermes": "the plugin decides: platform (remembered per session_id) in hermes_platforms -> sent (the hook and fold treat it as interactive); otherwise, or unknown -> not sent at all. Approval events carry no platform: they are sent only for surface 'cli' or 'transport:*' and are interactive. Consequence (notes.md): Hermes background or gateway work never reaches CodePet, so it can never set helpers_busy. Background-review agents per WP7",
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
        {"client": "hermes", "platform": "cli", "expect": True},
        {"client": "hermes", "platform": "acp", "expect": True},
        {"client": "hermes", "platform": "telegram", "expect": "not_sent"},
        {"client": "hermes", "platform": "cron", "expect": "not_sent"},
        {"client": "hermes", "platform": None, "expect": "not_sent"},
        {"client": "hermes", "surface": "cli", "expect": True},
        {"client": "hermes", "surface": "transport:acp", "expect": True},
        {"client": "hermes", "surface": "smart", "expect": "not_sent"},
        {"client": "hermes", "surface": "gateway:telegram", "expect": "not_sent"},
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
            "activity": "(a) a turn is open and (key == open turn, or the event has no turn key): waiting/idle -> working; a tool id not yet counted in that turn counts as a tool. An activity without a tool id (tool null: Hermes clarify/approval replies, OpenCode permission replies, a Claude activity without tool_use_id) never counts as a tool; it only changes the state. (b) key == last done turn, at <= last_done_ms + late_tool_ms and a tool id not counted in that turn: count it toward the finished turn; if that turn earned nothing and was not re-checked, re-run the treat check once (duration = last_done_ms - its start). A tool-less or already-counted late activity is (d) and never spends the re-check. (c) key == last done turn, at > last_done_ms + late_tool_ms and a tool id not counted in that turn: continuation - reopen the turn (same start, its tools + 1), state working. (d) otherwise only last_event_ms, except that a session in state waiting with no open turn returns to idle (the question was answered and nothing known is running)",
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
        {"name": "keyless_permission_prompts_like_real_claude", "client": "claude-code",
         "events": [ev(0, "turn_start", "t1", expect="working"),
                    ev(1000, "waiting", None, expect="waiting", note="Claude Notification has no prompt_id"),
                    ev(2000, "activity", "t1", "toolu_1", expect="working"),
                    ev(3000, "waiting", None, expect="waiting", note="a second prompt in the same turn"),
                    ev(4000, "activity", "t1", "toolu_2", expect="working"),
                    ev(6000, "turn_done", "t1", expect="idle", earned=True)],
         "final": {"treats": 1, "finished_seq": 1, "open_turn": None, "last_done_tools": 2}},
        {"name": "toolless_activity_changes_state_but_is_not_a_tool", "client": "hermes",
         "events": [ev(0, "turn_start", "h1", expect="working"),
                    ev(1000, "waiting", "h1", expect="waiting", note="clarify pre_tool_call"),
                    ev(2000, "activity", None, None, expect="working", note="clarify answered: keyless, tool-less"),
                    ev(5000, "turn_done", "h1", expect="idle", note="0 tools, 5 s: no treat")],
         "final": {"treats": 0, "finished_seq": 1, "open_turn": None, "last_done_tools": 0}},
        {"name": "toolless_late_activity_does_not_spend_the_recheck", "client": "codex",
         "events": [ev(0, "turn_start", "7", expect="working"),
                    ev(5000, "turn_done", "7", expect="idle", note="0 tools, 5 s: no treat yet"),
                    ev(5100, "activity", "7", None, expect="idle", note="(d): no tool id"),
                    ev(5200, "activity", "7", "call_1", expect="idle", earned=True, note="(b): first real late tool")],
         "final": {"treats": 1, "finished_seq": 1, "open_turn": None, "last_done_tools": 1}},
        {"name": "activity_without_an_open_turn_ends_waiting", "client": "hermes",
         "events": [ev(0, "session_start", expect="idle"),
                    ev(1000, "waiting", None, expect="waiting"),
                    ev(2000, "activity", None, None, expect="idle", note="(d): no open turn; waiting -> idle")],
         "final": {"treats": 0, "finished_seq": 0, "open_turn": None, "last_done_tools": 0}},
        {"name": "full_jar_celebrates_without_a_treat", "client": "codex", "jar": 10,
         "events": [ev(0, "turn_start", "7", expect="working"), ev(1000, "activity", "7", "c1", expect="working"),
                    ev(2000, "turn_done", "7", expect="idle", note="jar_full -> finished_jar_full")],
         "final": {"treats": 0, "finished_seq": 1, "open_turn": None, "last_done_tools": 1, "finished_jar_full": True}},
        {"name": "jar_full_describes_only_the_last_finished_turn", "client": "codex", "jar": 10,
         "events": [ev(0, "turn_start", "7", expect="working"), ev(1000, "activity", "7", "c1", expect="working"),
                    ev(2000, "turn_done", "7", expect="idle", note="jar_full -> finished_jar_full"),
                    ev(40000, "turn_start", "8", expect="working"),
                    ev(41000, "turn_done", "8", expect="idle", note="0 tools, 1 s: no treat check; the flag clears (act.done, not act.done_full)")],
         "final": {"treats": 0, "finished_seq": 2, "open_turn": None, "last_done_tools": 0, "finished_jar_full": False}},
        {"name": "late_tool_recheck_on_a_full_jar_sets_jar_full", "client": "codex", "jar": 10,
         "events": [ev(0, "turn_start", "7", expect="working"),
                    ev(5000, "turn_done", "7", expect="idle", note="0 tools, 5 s: no treat check yet"),
                    ev(5200, "activity", "7", "call_1", expect="idle", note="(b): the one re-check meets a full jar")],
         "final": {"treats": 0, "finished_seq": 1, "open_turn": None, "last_done_tools": 1, "finished_jar_full": True}},
    ],
}

# ---------------------------------------------------------------- fold drops (§6.4 steps 2-6)
NOW = 1792123900000
fold = {
    "rule": "outcomes in order: duplicate (dedupe_key exists: unlink, no row change) -> dropped_off (meta.enabled = 0) -> dropped_old (at < now - max_event_age_ms) -> clamp (at > now + future_clamp_ms -> at = now, still folded) -> dropped_child -> test records: outcome test while meta.test_state == 'running', else dropped_test (a late or replayed ready-check record) -> client record (+hello) -> no_life (stop) -> folded",
    "test_records": "the fold never changes meta.test_state/test_at_ms/test_client: it only keeps the agent_events row (test = 1, outcome test). The clients job owns the check: it sets test_state running (test_at_ms = now, test_client = the first tested client in client order) before writing the nonce and running the stored commands, then waits until every tested client has an agent_events row (client, test = 1, kind turn_done, outcome test, at_ms >= the job start) or test_done_delay_ms + test_arrival_ms passed, and sets done (all arrived) or failed (test_at_ms = now), removes $STATE/test.nonce and exports. Clients without a row get last_outcome failed, last_reason ready_check",
    "events_7d": "COUNT(*) FROM agent_events WHERE client = ? AND test = 0 AND outcome IN ('folded','no_life') AND at_ms > now - agent_events_keep_ms. Dropped (off, old, child, test) and duplicate deliveries never count",
    "now": NOW,
    "vectors": [
        {"name": "off_drops", "enabled": False, "life": True, "event": {"at": NOW, "kind": "turn_start", "turn": "t"},
         "expect": {"outcome": "dropped_off", "client_record": False, "hello": False}},
        {"name": "off_drops_test_records_too", "enabled": False, "life": True, "test_state": "running",
         "event": {"at": NOW, "kind": "turn_done", "turn": "t", "test": True},
         "expect": {"outcome": "dropped_off", "client_record": False, "hello": False, "test_state": "running"}},
        {"name": "too_old_drops", "enabled": True, "life": True, "event": {"at": NOW - 86400001, "kind": "turn_start", "turn": "t"},
         "expect": {"outcome": "dropped_old", "client_record": False, "hello": False}},
        {"name": "exactly_24h_is_kept", "enabled": True, "life": True, "event": {"at": NOW - 86400000, "kind": "turn_start", "turn": "t"},
         "expect": {"outcome": "folded", "client_record": True, "hello": True, "at": NOW - 86400000}},
        {"name": "future_is_clamped", "enabled": True, "life": True, "event": {"at": NOW + 120001, "kind": "turn_start", "turn": "t"},
         "expect": {"outcome": "folded", "client_record": True, "hello": True, "at": NOW}},
        {"name": "near_future_is_kept", "enabled": True, "life": True, "event": {"at": NOW + 120000, "kind": "turn_start", "turn": "t"},
         "expect": {"outcome": "folded", "client_record": True, "hello": True, "at": NOW + 120000}},
        {"name": "test_record_while_the_check_runs_is_kept", "enabled": True, "life": True, "test_state": "running",
         "event": {"at": NOW, "kind": "turn_done", "turn": "t", "test": True},
         "expect": {"outcome": "test", "client_record": False, "hello": False, "test_state": "running"}},
        {"name": "test_turn_start_while_running_is_kept", "enabled": True, "life": True, "test_state": "running",
         "event": {"at": NOW, "kind": "turn_start", "turn": "t", "test": True},
         "expect": {"outcome": "test", "client_record": False, "hello": False, "test_state": "running"}},
        {"name": "test_record_with_no_check_running_is_dropped", "enabled": True, "life": True, "test_state": None,
         "event": {"at": NOW, "kind": "turn_done", "turn": "t", "test": True},
         "expect": {"outcome": "dropped_test", "client_record": False, "hello": False, "test_state": None}},
        {"name": "test_record_after_the_verdict_is_dropped", "enabled": True, "life": True, "test_state": "done",
         "event": {"at": NOW, "kind": "turn_done", "turn": "t", "test": True},
         "expect": {"outcome": "dropped_test", "client_record": False, "hello": False, "test_state": "done"}},
        {"name": "test_record_never_greets_even_before_first_seen", "enabled": True, "life": False,
         "test_state": "running", "event": {"at": NOW, "kind": "turn_done", "turn": "t", "test": True},
         "expect": {"outcome": "test", "client_record": False, "hello": False, "test_state": "running"}},
        {"name": "no_life_still_greets", "enabled": True, "life": False, "event": {"at": NOW, "kind": "session_start"},
         "expect": {"outcome": "no_life", "client_record": True, "hello": True}},
        {"name": "second_event_does_not_greet_again", "enabled": True, "life": True, "first_seen": True,
         "event": {"at": NOW, "kind": "activity", "turn": "t", "tool": "x"},
         "expect": {"outcome": "folded", "client_record": True, "hello": False}},
    ],
    "hook_nonce_cases": "nonce matches / wrong / file missing are decided by the hook (normalization vectors with 'nonce'); only a matching record reaches the fold, and even then only while a check is running",
}

# ---------------------------------------------------------------- aggregate (§6.4 agent::pet_state)
N = NOW


def row(client, state, since, last, interactive=1, approval=0, error_at=None):
    return {"client": client, "approval": approval, "interactive": interactive, "state": state,
            "state_since_ms": since, "last_event_ms": last, "error_at_ms": error_at}


def agg(name, sessions, expect, count_helpers=False, now=N):
    return {"name": name, "now": now, "count_helpers": count_helpers, "sessions": sessions, "expect": expect}


def agg_out(state="idle", apps=(), stale=None, error_at=None, error_app=None, helpers=False):
    return {"state": state, "apps": list(apps), "stale_at_ms": stale, "error_at_ms": error_at,
            "error_app": error_app, "helpers_busy": helpers}


def app_e(cid, state, since, stale):
    return {"id": cid, "state": state, "since_ms": since, "stale_at_ms": stale}


aggregate = {
    "function": "Rust agent::pet_state(session rows, meta.count_helpers, now) -> the export's agent.state, agent.apps, agent.stale_at_ms, agent.error_at_ms, agent.error_app and agent.helpers_busy (the seq/finished/hello/test fields come from meta). Pure; computed at every export from committed agent_sessions rows",
    "row_fields": "client, approval (1 = Hermes approval-key row), interactive, state, state_since_ms, last_event_ms, error_at_ms",
    "rules": [
        "a row counts for its app only when interactive = 1 and: state working and now < last_event_ms + stale_working_ms (expiry = that sum); state waiting and now < last_event_ms + stale_waiting_ms; state error and now < error_at_ms + error_recent_ms (expiry = error_at_ms + error_recent_ms). idle and ended rows never count. The edges are half-open: exactly stale_working_ms after the last event the row no longer counts",
        "Hermes approval rows (approval = 1) are interactive and count like any row of client hermes (waiting only)",
        "per app: the highest state over its counting rows (waiting > working > error); since_ms = the smallest state_since_ms among its counting rows in that state; stale_at_ms = the largest expiry among them (the app keeps that state until its last such row expires)",
        "apps: one entry per app with a counting row, sorted by since_ms, then client order",
        "state: the highest app state, idle when apps is empty",
        "stale_at_ms: the smallest apps[].stale_at_ms, null when apps is empty. A lower state of another row of the same app reappears at the next export (that row's next event), not by a timer; this is the accepted bound of the no-polling rule",
        "error_at_ms / error_app: the largest error_at_ms over interactive non-approval rows of any state with now < error_at_ms + error_recent_ms (ties: client order); else both null",
        "helpers_busy: count_helpers and some row with interactive = 0, state working and now < last_event_ms + stale_working_ms. Non-interactive rows never reach apps, state, error_* or a face",
        "the aggregate ignores meta.enabled; the shared priority shows Off first",
    ],
    "vectors": [
        agg("nothing_counts", [], agg_out()),
        agg("idle_rows_do_not_count", [row("claude-code", "idle", N - 5000, N - 1000)], agg_out()),
        agg("working_stale_edge_is_half_open",
            [row("claude-code", "working", N - 700000, N - 600000), row("grok-cli", "working", N - 650000, N - 599999)],
            agg_out("working", [app_e("grok-cli", "working", N - 650000, N + 1)], N + 1)),
        agg("waiting_stale_edge_is_half_open",
            [row("codex", "waiting", N - 1900000, N - 1800000), row("opencode", "waiting", N - 1900000, N - 1799999)],
            agg_out("waiting", [app_e("opencode", "waiting", N - 1900000, N + 1)], N + 1)),
        agg("error_window_is_two_minutes",
            [row("claude-code", "error", N - 120000, N - 120000, error_at=N - 120000),
             row("hermes", "error", N - 119999, N - 119999, error_at=N - 119999)],
            agg_out("error", [app_e("hermes", "error", N - 119999, N + 1)], N + 1, N - 119999, "hermes")),
        agg("ended_and_non_interactive_rows_are_left_out",
            [row("claude-code", "ended", N - 1000, N - 1000), row("codex", "working", N - 5000, N - 1000, interactive=0)],
            agg_out()),
        agg("helpers_busy_needs_count_helpers",
            [row("codex", "working", N - 5000, N - 1000, interactive=0)], agg_out(helpers=True), count_helpers=True),
        agg("stale_helper_is_not_busy",
            [row("codex", "working", N - 700000, N - 600000, interactive=0)], agg_out(), count_helpers=True),
        agg("waiting_helper_is_not_busy",
            [row("claude-code", "waiting", N - 5000, N - 1000, interactive=0)], agg_out(), count_helpers=True),
        agg("hermes_approval_row_makes_hermes_wait",
            [row("hermes", "working", N - 60000, N - 30000), row("hermes", "waiting", N - 5000, N - 5000, approval=1)],
            agg_out("waiting", [app_e("hermes", "waiting", N - 5000, N - 5000 + 1800000)], N - 5000 + 1800000)),
        agg("one_app_several_sessions_waiting_beats_working",
            [row("claude-code", "working", N - 60000, N - 1000), row("claude-code", "waiting", N - 20000, N - 20000),
             row("claude-code", "waiting", N - 30000, N - 5000)],
            agg_out("waiting", [app_e("claude-code", "waiting", N - 30000, N + 1795000)], N + 1795000)),
        agg("apps_sorted_by_since_then_client_order",
            [row("grok-cli", "working", N - 60000, N - 1000), row("codex", "working", N - 60000, N - 2000),
             row("opencode", "error", N - 30000, N - 30000, error_at=N - 30000),
             row("claude-code", "working", N - 90000, N - 500)],
            agg_out("working", [app_e("claude-code", "working", N - 90000, N + 599500),
                                app_e("codex", "working", N - 60000, N + 598000),
                                app_e("grok-cli", "working", N - 60000, N + 599000),
                                app_e("opencode", "error", N - 30000, N + 90000)],
                    N + 90000, N - 30000, "opencode")),
        agg("recent_error_stays_exported_after_the_session_moved_on",
            [row("claude-code", "working", N - 1000, N - 1000, error_at=N - 50000),
             row("grok-cli", "idle", N - 90000, N - 90000, error_at=N - 100000)],
            agg_out("working", [app_e("claude-code", "working", N - 1000, N + 599000)], N + 599000, N - 50000,
                    "claude-code")),
        agg("error_tie_breaks_by_client_order",
            [row("codex", "error", N - 1000, N - 1000, error_at=N - 1000),
             row("claude-code", "error", N - 1000, N - 1000, error_at=N - 1000)],
            agg_out("error", [app_e("claude-code", "error", N - 1000, N + 119000),
                              app_e("codex", "error", N - 1000, N + 119000)], N + 119000, N - 1000, "claude-code")),
    ],
}

# ---------------------------------------------------------------- HMAC keys (§5.10, §6.4)
SALT = bytes(range(32))
SEP = "\x1f"


def hkey(domain, client, *parts):
    msg = SEP.join([domain, client, *parts]).encode()
    return hmac.new(SALT, msg, hashlib.sha256).hexdigest()[:32]


hmac_keys = {
    "formula": "key = lowercase hex(HMAC-SHA256(meta.hook_salt, domain + US + client + US + part1 [+ US + part2 ...]))[..32], US = the byte 0x1f. Parts are the raw ids from the inbox record (ids are [A-Za-z0-9._:-], so neither US nor '~' can occur in them)",
    "domains": {
        "session": {"domain": "codepet-session-v1", "parts": ["session"], "column": "agent_sessions.session_key (approval = 0), activity_turns.session_key, agent_events.session_hmac"},
        "approval": {"domain": "codepet-approval-v1", "parts": ["approval_key"], "column": "agent_sessions.session_key (approval = 1); its own domain, so it never collides with a session key"},
        "turn": {"domain": "codepet-turn-v1", "parts": ["session", "turn"], "column": "open_turn_key, last_done_turn_key, activity_turns.turn_key, agent_events.turn_hmac. A turn_start without a turn key uses turn = '~ev:' + event_id"},
        "tool": {"domain": "codepet-tool-v1", "parts": ["session", "tool"], "column": "agent_events.tool_hmac (distinct-tool counting per session_hmac + turn_hmac)"},
        "dedupe": {"domain": "codepet-dedupe-v1", "parts": ["kind", "session field of the event id", "dig", "the event id's bucket in decimal (event_ids.bucket)"], "column": "agent_events.dedupe_key"},
    },
    "salt_hex": SALT.hex(),
    "vectors": [],
}
for kind, client, parts in [
    ("session", "claude-code", [S]),
    ("session", "grok-cli", [S]),
    ("approval", "hermes", ["sk-cli-1"]),
    ("session", "hermes", ["sk-cli-1"]),
    ("turn", "claude-code", [S, "p-1"]),
    ("turn", "opencode", ["ses_1", "~ev:" + eid_cases[0]["expect"]["event_id"]]),
    ("tool", "claude-code", [S, "toolu_01"]),
    ("dedupe", "claude-code", ["turn_done", S, eid_cases[0]["expect"]["dig"], str(1792123876111 // 3000)]),
    ("dedupe", "claude-code", ["waiting", S, eid_cases[5]["expect"]["dig"], str(1792123876111)]),
]:
    hmac_keys["vectors"].append({"use": kind, "client": client, "parts": parts,
                                 "expect": hkey(hmac_keys["domains"][kind]["domain"], client, *parts)})
_v = hmac_keys["vectors"]
assert _v[0]["expect"] != _v[1]["expect"], "two clients sharing a session id must not collide"
assert _v[2]["expect"] != _v[3]["expect"], "an approval key must not collide with a session key"

# ---------------------------------------------------------------- pet state (§6.4 shared priority)
priority = {
    "function": "shared_priority(export, now, seen) -> face. Generated into petstate.js as sharedPriority (WP6) and used by the bar cell, Home and the overlay. Not Rust agent::pet_state, which is the aggregate that builds export.agent (see 'aggregate')",
    "order": [
        {"row": 1, "state": "off", "when": "enabled == false", "bar": "off", "line": "off.line (null without a life)", "overlay": "none"},
        {"row": 1.5, "state": "no_pet", "when": "life == null (first run, or after a goodbye/death; memorial.count does not matter)", "bar": "egg", "line": None, "overlay": "none"},
        {"row": 1.6, "state": "welcome", "when": "life.armed == false (the owner's upgraded Ginger before Let's go!)", "bar": "hi (thumb + sparkle)", "line": "picker.welcome", "overlay": "wander"},
        {"row": 2, "state": "fainted|paused|asleep", "when": "life.fainted, else life.paused (pet.pause; no UI sets it in v0.3), else care.sleeping", "bar": "sleep (+bandage when fainted)", "line": "mood.fainted|mood.asleep|mood.asleep", "overlay": "sleep"},
        {"row": 3, "state": "waiting", "when": "a counting app is waiting", "bar": "hand (pulse)", "line": "act.waiting", "overlay": "waiting"},
        {"row": 4, "state": "need_now", "when": "a need is empty (now) or critical", "bar": "need badge", "line": "mood.<need>.now|critical", "overlay": "needs_care"},
        {"row": 5, "state": "working|testing", "when": "a counting app is working; else agent.test.state == 'running' and now < test.at_ms + test_done_delay_ms + test_arrival_ms (the ready check types about 1.5 s)", "bar": "working frame", "line": "act.working.one|two|many ({app} = test.client while testing)", "overlay": "working"},
        {"row": 6, "state": "celebrate", "when": "grew up (unseen_events), a new test done, hello or finished_seq inside its window", "bar": "star", "line": "grew|act.ready|act.hello|act.done*", "overlay": "celebrate"},
        {"row": 7, "state": "oops", "when": "error_at_ms within error_recent_ms", "bar": "none (star if treats wait)", "line": "act.oops", "overlay": "error for error_frame_ms, then idle"},
        {"row": 8, "state": "need_soon", "when": "a need is low (soon)", "bar": "none (star if treats wait)", "line": "mood.<need>.soon", "overlay": "needs_care_frame"},
        {"row": 9, "state": "idle", "when": "otherwise", "bar": "none (star if treats wait)", "line": "mood.happy", "overlay": "wander"},
    ],
    "home_controls": {"off": "Turn on only", "no_pet": "not Home (popup_routing)", "welcome": "not Home (popup_routing)", "fainted": "only the two tiles that revive", "paused": "one big Wake up (pet.pause {paused:false})", "asleep": "one big Wake up (pet.wake)", "other": "4 tiles, 3 round buttons, Treats when treats_visible"},
    "need_levels": {"ok": "happy", "low": "soon", "empty": "now", "critical": "critical"},
    "need_priority": ["food", "water", "play", "wash"],
    "need_pick": "among needs at the highest present group (now/critical first, then soon), pick by need_priority; the line uses that need's own level",
    "counting_app": "an agent.apps entry whose stale_at_ms > now",
    "app_order": "agent.apps order (since_ms ascending, then client order); {app} is the first app in the row's set, {app2} the second",
    "waiting_app": "the waiting app with the latest since_ms (ties: client order)",
    "celebrate": {
        "candidates_in_order": ["grew", "test", "hello", "finished"],
        "grew": "unseen_events is not empty (stage journal entries not yet marked seen). No time window and no wake: it stays until Home has shown it, then the popup calls pet.events_seen {life_id, through_seq: the largest seq} (the bar and the overlay never call it). Bubble: star + confetti",
        "test": "agent.test.state == 'done' and test.at_ms > seen.test_at_ms and now < at_ms + celebrate_window_ms",
        "hello": "hello_seq > seen.hello_seq and now < hello_at_ms + celebrate_window_ms",
        "finished": "finished_seq > seen.finished_seq and now >= finished_at_ms + celebrate_grace_ms and now < finished_at_ms + celebrate_window_ms and finished_app is not a counting working app",
        "lines": {"grew": "grew", "test": "act.ready", "hello": "act.hello", "finished": "act.done_full if finished_jar_full, else act.done_treat if finished_treat, else act.done"},
        "seen": "the UI records the celebrated seq/at and passes it back as `seen`",
    },
    "treats_visible": "enabled and treats.jar > 0 and the life is awake (not care.sleeping), not paused and not fainted",
    "star_badge": "rows 7-9 show the star badge when treats_visible",
    "wake_at_ms": "null when Off, without a life and for welcome. Otherwise the minimum of these future times (null when none), whatever row wins: stale_at_ms of each counting app; the testing end (test.at_ms + test_done_delay_ms + test_arrival_ms) while running and ahead; for each unseen time-windowed celebration candidate whose window is still open, its grace end if still ahead, else its window end; the error frame end and the oops end while ahead",
    "vars": "exactly the placeholders of the line template; {app}/{app2} are client ids (the UI maps them to display names)",
    "bubble": {"waiting": "apps: [waiting app]", "need_now": "need: <need>", "working": "apps: up to 3 working apps", "testing": "apps: [test.client]", "fainted": "bandage", "celebrate": "star (+ confetti for grew)", "need_soon": "need: <need>", "idle": "helpers when helpers_busy, else none"},
}

def pv(name, fixture, now, expect, seen=None):
    return {"name": name, "fixture": fixture, "now": now,
            "seen": seen or {"finished_seq": 0, "hello_seq": 0, "test_at_ms": 0}, "expect": expect}

def out(state, bar, line, overlay, vars=None, pulse=False, treats=False, bubble=None, wake=None):
    return {"state": state, "bar": bar, "line": line, "vars": vars or {}, "overlay": overlay, "pulse": pulse,
            "treats_visible": treats, "bubble": bubble, "wake_at_ms": wake}

T = NOW  # fixtures are generated at NOW (see fixtures/bar-v2)
G = {"name": "Ginger"}
priority["vectors"] = [
    pv("fresh_install_no_pet", "fresh-no-pet", T, out("no_pet", "egg", None, "none")),
    pv("memorial_is_still_no_pet_on_the_bar", "memorial", T, out("no_pet", "egg", None, "none")),
    pv("off_wins_over_everything", "off", T, out("off", "off", "off.line", "none", G)),
    pv("off_hides_treats", "off-treats", T, out("off", "off", "off.line", "none", G)),
    pv("off_before_any_pet", "off-no-pet", T, out("off", "off", None, "none")),
    pv("owner_unarmed_welcome", "owner-unarmed-welcome", T, out("welcome", "hi", "picker.welcome", "wander", G)),
    pv("idle_happy", "idle-happy", T, out("idle", "none", "mood.happy", "wander", G)),
    pv("idle_with_treats_shows_star", "idle-treats", T, out("idle", "star", "mood.happy", "wander", G, treats=True)),
    pv("need_soon", "need-food-soon", T, out("need_soon", "none", "mood.food.soon", "needs_care_frame", G, bubble={"need": "food"})),
    pv("need_now", "need-water-now", T, out("need_now", "need_water", "mood.water.now", "needs_care", G, bubble={"need": "water"})),
    pv("critical_food", "need-food-critical", T, out("need_now", "need_food", "mood.food.critical", "needs_care", G, bubble={"need": "food"})),
    pv("several_needs_priority_order", "several-needs", T, out("need_now", "need_water", "mood.water.now", "needs_care", G, bubble={"need": "water"})),
    pv("asleep", "asleep", T, out("asleep", "sleep", "mood.asleep", "sleep", G)),
    pv("paused_looks_asleep_and_hides_treats", "paused", T, out("paused", "sleep", "mood.asleep", "sleep", G)),
    pv("fainted_beats_waiting", "fainted", T, out("fainted", "sleep_bandage", "mood.fainted", "sleep", G, bubble={"bandage": True}, wake=T + 1790000)),
    pv("working_one", "working-one", T, out("working", "working", "act.working.one", "working", {"name": "Ginger", "app": "claude-code"}, bubble={"apps": ["claude-code"]}, wake=T + 540000)),
    pv("working_two", "working-two", T, out("working", "working", "act.working.two", "working", {"app": "claude-code", "app2": "grok-cli"}, bubble={"apps": ["claude-code", "grok-cli"]}, wake=T + 540000)),
    pv("working_many", "working-many", T, out("working", "working", "act.working.many", "working", G, bubble={"apps": ["claude-code", "opencode", "grok-cli"]}, wake=T + 540000)),
    pv("waiting_beats_need_now_and_working", "waiting", T, out("waiting", "hand", "act.waiting", "waiting", {"app": "codex"}, pulse=True, bubble={"apps": ["codex"]}, wake=T + 540000)),
    pv("working_beats_celebration", "finished-while-working", T, out("working", "working", "act.working.one", "working", {"name": "Ginger", "app": "claude-code"}, bubble={"apps": ["claude-code"]}, wake=T + 20000)),
    pv("ready_check_types_while_running", "test-running", T, out("testing", "working", "act.working.one", "working", {"name": "Ginger", "app": "claude-code"}, bubble={"apps": ["claude-code"]}, wake=T + 6000)),
    pv("ready_check_running_too_long_stops_typing", "test-running", T + 6000, out("idle", "none", "mood.happy", "wander", G)),
    pv("ready_check_failed_shows_no_face", "test-failed", T, out("idle", "none", "mood.happy", "wander", G)),
    pv("grew_up_celebrates_until_seen", "grew-up", T, out("celebrate", "star", "grew", "celebrate", G, treats=True, bubble={"star": True, "confetti": True})),
    pv("finished_inside_grace_waits", "finished-treat", T - 25000 + 1000, out("idle", "star", "mood.happy", "wander", G, treats=True, wake=T - 25000 + 2000)),
    pv("finished_treat_celebrates", "finished-treat", T, out("celebrate", "star", "act.done_treat", "celebrate", G, treats=True, bubble={"star": True}, wake=T + 5000)),
    pv("finished_already_seen", "finished-treat", T, out("idle", "star", "mood.happy", "wander", G, treats=True), seen={"finished_seq": 5, "hello_seq": 0, "test_at_ms": 0}),
    pv("finished_window_over", "finished-treat", T + 5000, out("idle", "star", "mood.happy", "wander", G, treats=True)),
    pv("finished_jar_full", "jar-full", T, out("celebrate", "star", "act.done_full", "celebrate", {}, treats=True, bubble={"star": True}, wake=T + 20000)),
    pv("hello_first_event", "hello", T, out("celebrate", "star", "act.hello", "celebrate", {"name": "Ginger", "app": "claude-code"}, bubble={"star": True}, wake=T + 25000)),
    pv("ready_check_done", "ready-check-done", T, out("celebrate", "star", "act.ready", "celebrate", G, bubble={"star": True}, wake=T + 28500)),
    pv("oops_frame", "oops", T, out("oops", "none", "act.oops", "error", {"app": "grok-cli"}, wake=T + 2000)),
    pv("oops_after_frame", "oops", T + 2000, out("oops", "none", "act.oops", "idle", {"app": "grok-cli"}, wake=T + 119000)),
    pv("oops_over", "oops", T + 119000, out("idle", "none", "mood.happy", "wander", G)),
    pv("stale_app_no_longer_counts", "stale-working", T, out("idle", "none", "mood.happy", "wander", G)),
    pv("helpers_busy_bubble_only", "helpers-busy", T, out("idle", "none", "mood.happy", "wander", G, bubble={"helpers": True})),
    pv("owner_goldfish_ginger_aquatic_need", "goldfish-ginger-owner", T, out("need_soon", "none", "mood.wash.soon", "needs_care_frame", G, bubble={"need": "wash"})),
    pv("robot_needs_charge", "robot-bolt", T, out("need_now", "need_food", "mood.food.now", "needs_care", {"name": "Bolt"}, bubble={"need": "food"})),
]

# ---------------------------------------------------------------- popup routing (§2, §3)
popup_routing = {
    "function": "route(export) -> the screen the popup shows when it opens or after any export while open",
    "rules_in_order": [
        "enabled == false and life != null -> off_look (Home variant: dimmed sleeping pet, off.line, Turn on)",
        "enabled == false and life == null -> first_run_off (the picker with the Turn on footer instead of Let's go!; Let's go! is never offered while Off)",
        "life == null and memorial.count == 0 -> first_run (picker, cat preselected, Let's go!)",
        "life == null and memorial.count > 0 -> memorial (memorial.line with memorial.latest_name, Meet a new pet -> first_run for this popup session only; UI-local, not stored)",
        "life.armed == false -> welcome_back (picker preselecting the life's look, name kept, Let's go! = pet.start on that life)",
        "otherwise -> home",
    ],
    "transitions": [
        "a left click on the Off bar cell calls app.set_enabled {enabled:true} first, then routes (one click turns CodePet on and opens Home)",
        "the power switch flipped to Off on any screen (Helpers, More, Help, Change, a row sheet) routes to off_look / first_run_off at once; flipped back On, the popup routes again (Home, not the sub-page it was on)",
        "after a successful pet.start the UI calls clients.status and shows connect_card when connect_card.offered is not empty, else home; closing the popup on the card counts as Not now",
        "after pet.retire (Say goodbye) the next route is memorial",
    ],
    "vectors": [
        {"fixture": "fresh-no-pet", "expect": "first_run"},
        {"fixture": "off-no-pet", "expect": "first_run_off"},
        {"fixture": "off", "expect": "off_look"},
        {"fixture": "off-treats", "expect": "off_look"},
        {"fixture": "memorial", "expect": "memorial"},
        {"fixture": "owner-unarmed-welcome", "expect": "welcome_back"},
        {"fixture": "goldfish-ginger-owner", "expect": "home"},
        {"fixture": "idle-happy", "expect": "home"},
        {"fixture": "paused", "expect": "home"},
        {"fixture": "fainted", "expect": "home"},
        {"fixture": "asleep", "expect": "home"},
        {"fixture": "grew-up", "expect": "home"},
    ],
}

connect_card = {
    "function": "offered(clients.status result) -> the apps on the first-run Connect card, all pre-ticked",
    "rule": "every client in client order except codex with installed and not connected and job == null (a client whose last Connect failed is offered again). Empty -> the card is skipped",
    "vectors": [
        {"status_fixture": "nothing-installed", "expect": []},
        {"status_fixture": "owner-first-run", "expect": ["claude-code", "opencode", "hermes", "grok-cli"]},
        {"status_fixture": "after-connect", "expect": ["hermes"]},
        {"status_fixture": "all-connected", "expect": []},
    ],
}

# ---------------------------------------------------------------- Helpers rows (§2.3, §3.5)
DAY = 86400000
MIDNIGHT = NOW // DAY * DAY  # NOW is 04:11:40 UTC


def hr(row, visible=True, hint=None, sheet_hints=None, reason=None, step=None, stop=False, spinner=False, guide=None):
    return {"visible": visible, "row": row, "hint": hint, "sheet_hints": sheet_hints or ([] if hint is None else [hint]),
            "reason": reason, "step": step, "stop_watching": stop, "spinner": spinner, "guide": guide}


HIDDEN = {"visible": False}
helpers_rows = {
    "function": "helper_row(client id, export client record or null, now, tz_offset_min) -> how its Helpers row and row sheet look",
    "rules_in_order": [
        "no record, or installed == false and connected == false -> hidden (no row)",
        "job running: kind connect -> row.connecting; disconnect -> row.disconnecting; test -> row.testing. spinner, no step, no Stop watching",
        "failed_reason != null -> row.failed + reason.<reason_to_copy>, step btn.retry (clients.connect), Stop watching when connected",
        "needs_step == codex_hooks_off -> row.codex.off, step btn.showme (the Codex guide's codex.off page), Stop watching when connected",
        "needs_step == codex_trust -> row.step, step btn.showme (the Codex trust guide codex.step1..5), Stop watching when connected",
        "connected and first_seen_ms == null -> row.waiting with hint.open shown at once on the list; the sheet adds hint.claude2 for claude-code. Stop watching",
        "connected -> row.seen.<bucket> of last_seen_ms (first_seen_ms when last_seen_ms is null). Stop watching",
        "not connected, codex -> row.codex.new, step btn.connect",
        "not connected -> row.connect, step btn.connect",
    ],
    "seen_buckets": {
        "now": "now - seen < seen_just_now_ms (including a seen time in the future)",
        "today": "otherwise, the same local calendar day as now",
        "yesterday": "the local calendar day before now's",
        "while": "anything older",
        "local_day": "floor((ms + tz_offset_min * 60000) / 86400000); the UI uses Qt's local date, so DST is the system's",
        "seen_just_now_ms": 300000,
    },
    "connect_all": "the ids of visible rows that are not connected, have no running job and are not codex (a failed row counts); the button is hidden when the list is empty",
    "fixture_vectors": [
        {"fixture": "clients-mixed", "now": NOW, "tz_offset_min": 0, "expect": {
            "rows": {"claude-code": hr("row.seen.now", stop=True),
                     "codex": hr("row.step", step="btn.showme", stop=True, guide="codex.trust"),
                     "opencode": hr("row.connecting", spinner=True),
                     "hermes": hr("row.failed", reason="reason.too_old", step="btn.retry"),
                     "grok-cli": hr("row.waiting", hint="hint.open", stop=True)},
            "connect_all": ["hermes"]}},
        {"fixture": "clients-codex-hooks-off", "now": NOW, "tz_offset_min": 0, "expect": {
            "rows": {"claude-code": hr("row.seen.today", stop=True),
                     "codex": hr("row.codex.off", step="btn.showme", guide="codex.off"),
                     "opencode": HIDDEN, "hermes": HIDDEN, "grok-cli": HIDDEN},
            "connect_all": []}},
        {"fixture": "clients-codex-new", "now": NOW, "tz_offset_min": 0, "expect": {
            "rows": {"claude-code": HIDDEN, "codex": hr("row.codex.new", step="btn.connect"),
                     "opencode": hr("row.seen.while", stop=True), "hermes": HIDDEN, "grok-cli": HIDDEN},
            "connect_all": []}},
        {"fixture": "clients-disconnect-running", "now": NOW, "tz_offset_min": 0, "expect": {
            "rows": {"claude-code": HIDDEN, "codex": HIDDEN, "opencode": HIDDEN,
                     "hermes": hr("row.disconnecting", spinner=True), "grok-cli": hr("row.testing", spinner=True)},
            "connect_all": []}},
        {"fixture": "clients-failed-while-connected", "now": NOW, "tz_offset_min": 0, "expect": {
            "rows": {"claude-code": hr("row.failed", reason="reason.blocked", step="btn.retry", stop=True),
                     "codex": HIDDEN, "opencode": hr("row.seen.yesterday", stop=True), "hermes": HIDDEN,
                     "grok-cli": HIDDEN},
            "connect_all": []}},
        {"fixture": "hello", "now": NOW, "tz_offset_min": 0, "expect": {
            "rows": {"claude-code": hr("row.seen.now", stop=True)}, "connect_all": []}},
        {"fixture": "ready-check-done", "now": NOW, "tz_offset_min": 0, "expect": {
            "rows": {"claude-code": hr("row.waiting", hint="hint.open", sheet_hints=["hint.open", "hint.claude2"], stop=True)},
            "connect_all": []}},
        {"fixture": "test-running", "now": NOW, "tz_offset_min": 0, "expect": {
            "rows": {"claude-code": hr("row.connecting", spinner=True)}, "connect_all": []}},
        {"fixture": "test-failed", "now": NOW, "tz_offset_min": 0, "expect": {
            "rows": {"claude-code": hr("row.failed", reason="reason.blocked", step="btn.retry", stop=True)},
            "connect_all": []}},
    ],
    "bucket_vectors": [
        {"name": "four_minutes_is_just_now", "now": NOW, "tz_offset_min": 0, "last_seen_ms": NOW - 240000, "first_seen_ms": NOW - DAY, "expect": "row.seen.now"},
        {"name": "five_minutes_is_today", "now": NOW, "tz_offset_min": 0, "last_seen_ms": NOW - 300000, "first_seen_ms": NOW - DAY, "expect": "row.seen.today"},
        {"name": "future_is_just_now", "now": NOW, "tz_offset_min": 0, "last_seen_ms": NOW + 60000, "first_seen_ms": NOW - DAY, "expect": "row.seen.now"},
        {"name": "just_after_midnight_today", "now": NOW, "tz_offset_min": 0, "last_seen_ms": MIDNIGHT + 1, "first_seen_ms": NOW - DAY, "expect": "row.seen.today"},
        {"name": "just_before_midnight_yesterday", "now": NOW, "tz_offset_min": 0, "last_seen_ms": MIDNIGHT - 1, "first_seen_ms": NOW - DAY, "expect": "row.seen.yesterday"},
        {"name": "start_of_yesterday", "now": NOW, "tz_offset_min": 0, "last_seen_ms": MIDNIGHT - DAY, "first_seen_ms": NOW - 2 * DAY, "expect": "row.seen.yesterday"},
        {"name": "before_yesterday_is_a_while", "now": NOW, "tz_offset_min": 0, "last_seen_ms": MIDNIGHT - DAY - 1, "first_seen_ms": NOW - 2 * DAY, "expect": "row.seen.while"},
        {"name": "eleven_minutes_across_utc_midnight", "now": MIDNIGHT + 60000, "tz_offset_min": 0, "last_seen_ms": MIDNIGHT - 600000, "first_seen_ms": MIDNIGHT - DAY, "expect": "row.seen.yesterday"},
        {"name": "same_moment_is_today_one_hour_east", "now": MIDNIGHT + 60000, "tz_offset_min": 60, "last_seen_ms": MIDNIGHT - 600000, "first_seen_ms": MIDNIGHT - DAY, "expect": "row.seen.today"},
        {"name": "west_of_utc_moves_the_day_back", "now": NOW, "tz_offset_min": -300, "last_seen_ms": MIDNIGHT - 3600000, "first_seen_ms": NOW - DAY, "expect": "row.seen.today"},
        {"name": "falls_back_to_first_seen", "now": NOW, "tz_offset_min": 0, "last_seen_ms": None, "first_seen_ms": NOW - 60000, "expect": "row.seen.now"},
    ],
}

doc = OrderedDict([
    ("schema", "codepet.agent_vectors.v1"),
    ("spec", "SPEC-v0.3 §2, §3.5, §5.8, §5.10, §6.4, §8, §9.1"),
    ("consumers", "Rust codepet-core::agent (WP2: normalization, event ids, hmac_keys, interactivity, turn_qualifies, caps, sessions, fold, aggregate) and the generated petstate.js via test_state_machine.mjs / test_bar_model.mjs (WP6: priority) plus the WP5 popup (popup_routing, connect_card, helpers_rows); replayed by docs/v0.3/contracts/check_contracts.py"),
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
    ("aggregate", aggregate),
    ("hmac_keys", hmac_keys),
    ("priority", priority),
    ("popup_routing", popup_routing),
    ("connect_card", connect_card),
    ("helpers_rows", helpers_rows),
])
with open(OUT, "w") as f:
    json.dump(doc, f, indent=1, ensure_ascii=False)
    f.write("\n")
