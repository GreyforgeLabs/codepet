#!/usr/bin/env python3
"""Writes docs/v0.3/contracts/{shim.sh.template, shim-tombstone.sh, commands.json,
plugin-payload.schema.json} (WP0; SPEC-v0.3 §5.1-§5.8). WP2 owns the shim's behaviour and the
normalizer, WP3 installs the shim and writes every client artifact; both build the exact bytes
frozen here. Run from anywhere:
    python3 docs/v0.3/contracts/tools/gen_commands.py
"""
import hashlib
import json
from collections import OrderedDict as O
from pathlib import Path

HERE = Path(__file__).resolve().parents[1]
IDS = ["claude-code", "codex", "opencode", "hermes", "grok-cli"]
KINDS = ["session_start", "turn_start", "activity", "waiting", "idle_prompt", "turn_done", "turn_failed",
         "turn_cancelled", "session_end"]
STATE_PH, BIN_PH, NONCE_PH = "@CODEPET_STATE_DIR@", "@CODEPET_BIN@", "@CODEPET_TEST_NONCE@"

SHIM = f"""#!/bin/sh
# managed by codepet: observe-only bridge; prints nothing; always exits 0
exec >/dev/null 2>&1
S='{STATE_PH}'
B='{BIN_PH}'
d() {{ timeout -s KILL 0.3 cat >/dev/null 2>&1; exit 0; }}
[ -e "$S/off" ] && d
[ -e "$S/clients/$1.disconnected" ] && d
if [ "$1" = claude-code ]; then
  [ -n "$GROK_HOOK_EVENT" ] && d
  case "$CLAUDE_CODE_ENTRYPOINT" in sdk*) [ -e "$S/helpers" ] || d ;; esac
fi
[ -d "$S/inbox" ] && [ -x "$B" ] || d
"$B" hook --state-dir "$S" "$1" >/dev/null 2>&1
exit 0
"""
TOMBSTONE = """#!/bin/sh
# managed by codepet: tombstone
exec >/dev/null 2>&1
timeout -s KILL 0.3 cat >/dev/null 2>&1; exit 0
"""

REF = O([("home", "/home/u"), ("state_dir", "/home/u/.local/state/codepet"), ("bin", "/home/u/.local/bin/codepet"),
         ("data_dir", "/home/u/.local/share/codepet"), ("shim", "/home/u/.local/share/codepet/bin/codepet-hook")])


def sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def shell_command(client: str) -> str:
    return f"{REF['shim']} {client} >/dev/null 2>&1 || true"


def handler(client, timeout, is_async=False):
    h = O([("type", "command"), ("command", shell_command(client)), ("timeout", timeout)])
    if is_async:
        h["async"] = True
    return h


def group(client, timeout, is_async=False, matcher=None):
    g = O()
    if matcher is not None:
        g["matcher"] = matcher
    g["hooks"] = [handler(client, timeout, is_async)]
    return g


def file_bytes(obj) -> bytes:
    return (json.dumps(obj, indent=2, ensure_ascii=False) + "\n").encode()


CLAUDE_WAIT = "permission_prompt|elicitation_dialog|elicitation_url_dialog|agent_needs_input"
claude_hooks = O([
    ("description", "CodePet: observe only; prints nothing; always exits 0"),
    ("hooks", O([
        ("SessionStart", [group("claude-code", 2)]),
        ("UserPromptSubmit", [group("claude-code", 2)]),
        ("PreToolUse", [group("claude-code", 2, True)]),
        ("Notification", [group("claude-code", 2, matcher=CLAUDE_WAIT), group("claude-code", 2, matcher="idle_prompt")]),
        ("Stop", [group("claude-code", 2)]),
        ("StopFailure", [group("claude-code", 2)]),
        ("SessionEnd", [group("claude-code", 1)]),
    ])),
])
grok_hooks = O([("hooks", O([
    ("SessionStart", [group("grok-cli", 2)]),
    ("UserPromptSubmit", [group("grok-cli", 2)]),
    ("PreToolUse", [group("grok-cli", 2)]),
    ("Notification", [group("grok-cli", 2, matcher="permission_prompt"), group("grok-cli", 2, matcher="idle_prompt")]),
    ("Stop", [group("grok-cli", 2)]),
    ("StopFailure", [group("grok-cli", 2)]),
    ("StopCancelled", [group("grok-cli", 2)]),
    ("SessionEnd", [group("grok-cli", 2)]),
]))])
CODEX_EVENTS = [("UserPromptSubmit", "user_prompt_submit", False), ("PermissionRequest", "permission_request", True),
                ("PostToolUse", "post_tool_use", True), ("Stop", "stop", False), ("Interrupt", "interrupt", False),
                ("SessionEnd", "session_end", False)]
codex_groups = []
for event, snake, is_async in CODEX_EVENTS:
    g = group("codex", 2, is_async)
    trust = {"event_name": snake,
             "hooks": [{"async": is_async, "command": shell_command("codex"), "timeout": 2, "type": "command"}]}
    canon = json.dumps(trust, sort_keys=True, separators=(",", ":"))
    codex_groups.append(O([("event", event), ("snake_event", snake), ("group", g),
                           ("trusted_hash", "sha256:" + sha(canon.encode()))]))

READY = "codepet-ready-check"


def ready(client):
    if client == "claude-code":
        return [O([("hook_event_name", "UserPromptSubmit"), ("session_id", READY), ("prompt_id", READY),
                   ("codepet_test", NONCE_PH)]),
                O([("hook_event_name", "Stop"), ("session_id", READY), ("prompt_id", READY),
                   ("stop_hook_active", False), ("codepet_test", NONCE_PH)])]
    if client == "codex":
        return [O([("hook_event_name", "UserPromptSubmit"), ("session_id", READY), ("turn_id", READY),
                   ("codepet_test", NONCE_PH)]),
                O([("hook_event_name", "Stop"), ("session_id", READY), ("turn_id", READY),
                   ("codepet_test", NONCE_PH)])]
    if client == "grok-cli":
        return [O([("hookEventName", "user_prompt_submit"), ("hook_event_name", "UserPromptSubmit"),
                   ("sessionId", READY), ("promptId", READY), ("codepet_test", NONCE_PH)]),
                O([("hookEventName", "stop"), ("hook_event_name", "Stop"), ("sessionId", READY), ("promptId", READY),
                   ("reason", "end_turn"), ("codepet_test", NONCE_PH)])]
    return [O([("codepet_v", 1), ("event", "turn_start"), ("session_id", READY), ("turn_id", READY),
               ("interactive", True), ("codepet_test", NONCE_PH)]),
            O([("codepet_v", 1), ("event", "turn_done"), ("session_id", READY), ("turn_id", READY), ("tools", 0),
               ("interactive", True), ("codepet_test", NONCE_PH)])]


shim_ref = SHIM.replace(STATE_PH, REF["state_dir"]).replace(BIN_PH, REF["bin"]).encode()
commands = O([
    ("schema", "codepet.commands.v1"),
    ("spec", "SPEC-v0.3 §5.1, §5.3-§5.7, §6.2 clients.test"),
    ("consumers", "WP2 (shim behaviour, normalizer, ready-check payloads), WP3 (installers, clients job, doctor); checked by check_contracts.py"),
    ("reference_paths", REF),
    ("shim", O([
        ("path", "<data_dir>/bin/codepet-hook, data_dir = $XDG_DATA_HOME/codepet else $HOME/.local/share/codepet (StatePaths::data_dir)"),
        ("template", "shim.sh.template"),
        ("placeholders", O([(STATE_PH, "the connecting process's resolved absolute state dir (never the hook's environment)"),
                            (BIN_PH, "std::env::current_exe() of the connecting codepet, or the plugin's binary setting when the bar starts the Connect")])),
        ("substitution", "exactly one plain text replacement of each placeholder; nothing else in the file ever changes, whatever the CodePet version"),
        ("path_rule", "both values are absolute, contain none of ' (single quote), newline, NUL, and are <= 4096 bytes; otherwise the Connect fails with reason blocked and writes nothing"),
        ("mode", "0755, written as temp file in the same dir + fsync + rename"),
        ("tombstone", "shim-tombstone.sh, written in place of the shim when uninstall cannot disconnect every client"),
        ("reference_sha256", sha(shim_ref)),
        ("tombstone_sha256", sha(TOMBSTONE.encode())),
        ("doctor", "codepet doctor prints the installed shim's sha256 and checks its S/B against the bar's paths"),
        ("silence", "the first command of the shim and the tombstone is `exec >/dev/null 2>&1` (stdin stays for the drain): the shell's own messages, such as bash's 'Killed' job report when the 0.3 s drain is cut, never reach the argv-form clients (OpenCode, Hermes). Amended after the WP2 review (notes.md §5)"),
    ])),
    ("stored_command", O([
        ("shell", "<shim> <client-id> >/dev/null 2>&1 || true  (claude-code, codex, grok-cli: the client runs it through a shell)"),
        ("argv", "[<shim>, <client-id>]  (opencode, hermes: the plugin spawns it directly)"),
        ("literal", "the shim path is written expanded, as a literal absolute path; never $HOME, ~ or ${CLAUDE_PLUGIN_ROOT}"),
    ])),
    ("serialization", "CodePet-owned JSON files (Claude hooks.json, Grok codepet.json) are written as json with 2-space indent, the key order shown here and one trailing newline (serde_json::to_string_pretty + '\\n'); a rewrite happens only when the bytes differ. Codex groups are appended to the parsed ~/.codex/hooks.json (serde_json preserve_order) with exactly the keys and order shown, and the file is written with to_string_pretty + '\\n'"),
    ("clients", O([
        ("claude-code", O([
            ("transport", "shell"),
            ("artifact", "<data_dir>/integrations/claude-code/plugin/hooks/hooks.json (in the codepet-local marketplace; installed with `claude plugin install`, SPEC §5.3)"),
            ("command", shell_command("claude-code")),
            ("file", claude_hooks),
            ("file_sha256_at_reference_paths", sha(file_bytes(claude_hooks))),
            ("notes", "PreToolUse is async (Claude does not enforce its timeout, the shim's 0.3 s drain and the hook's alarm(1) bound it). No matcher = all. SessionEnd 1 s"),
        ])),
        ("codex", O([
            ("transport", "shell (Codex runs each handler with $SHELL -lc)"),
            ("artifact", "groups appended to ~/.codex/hooks.json (SPEC §5.7 install algorithm)"),
            ("command", shell_command("codex")),
            ("groups", codex_groups),
            ("trust_hash_formula", "sha256 of compact sorted-key JSON {event_name: <snake>, hooks: [{async, command, timeout (normalized; 2 stays 2), type}]}; no matcher key because CodePet groups have none. async is false when the handler omits it. Reproduces Codex config fingerprint.rs (verified on the owner's four existing entries)"),
            ("notes", "PermissionRequest and PostToolUse carry \"async\": true (discovery.rs keeps it; an async handler can never decide an approval). SessionEnd never carries async (Codex would force it synchronous and warn). The trust key is <abs hooks.json path>:<snake_event>:<group index>:0"),
        ])),
        ("opencode", O([
            ("transport", "argv"),
            ("artifact", "~/.config/opencode/plugins/codepet.js (header // managed by codepet)"),
            ("argv", [REF["shim"], "opencode"]),
            ("payload_schema", "plugin-payload.schema.json"),
        ])),
        ("hermes", O([
            ("transport", "argv"),
            ("artifact", "~/.hermes/plugins/codepet/ (plugin.yaml + __init__.py), enabled with `hermes plugins enable codepet`"),
            ("argv", [REF["shim"], "hermes"]),
            ("payload_schema", "plugin-payload.schema.json"),
        ])),
        ("grok-cli", O([
            ("transport", "shell (sh -c on Unix per Grok's hook guide; WP3 re-confirms, else the stored form is the bare `<shim> grok-cli`)"),
            ("artifact", "~/.grok/hooks/codepet.json (temp name .codepet.json.partial)"),
            ("command", shell_command("grok-cli")),
            ("file", grok_hooks),
            ("file_sha256_at_reference_paths", sha(file_bytes(grok_hooks))),
            ("notes", "no args and no async (Grok skips or ignores them); timeouts are seconds"),
        ])),
    ])),
    ("ready_check", O([
        ("rule", "clients.test runs, per tested client, its stored command (shell clients: /bin/sh -c '<command>'; argv clients: the argv) with a minimal environment (PATH, HOME; never GROK_HOOK_EVENT or CLAUDE_CODE_ENTRYPOINT), stdin = the payload line with " + NONCE_PH + " replaced by the nonce, plus a newline: first payloads[0] (turn_start), then payloads[1] (turn_done) test_done_delay_ms later"),
        ("payloads", O([(c, ready(c)) for c in IDS])),
    ])),
])

ID = O([("type", "string"), ("minLength", 1), ("maxLength", 128), ("pattern", "^[A-Za-z0-9._:-]{1,128}$")])
payload_schema = O([
    ("$schema", "https://json-schema.org/draft/2020-12/schema"),
    ("$id", "https://greyforge.tech/codepet/plugin-payload.schema.json"),
    ("title", "Synthesized hook payload of the OpenCode and Hermes plugins (codepet_v 1)"),
    ("description", "One JSON object plus a newline, < 512 bytes, written to the shim's stdin by the CodePet OpenCode plugin (codepet.js) or Hermes plugin. Plugins must only emit payloads that validate. The hook is a lenient reader (agent_vectors.json normalization rules): invalid tools -> 0, non-boolean interactive -> true, unknown keys are never read. Extra rules: exactly one of session_id or approval_key; approval_key only from hermes and only with event waiting or activity; opencode turn_id is <pluginStartMs>-<n>; completed/failed/interrupted are never sent (the Hermes plugin maps on_session_end to turn_done/turn_failed/turn_cancelled itself)."),
    ("type", "object"),
    ("additionalProperties", False),
    ("required", ["codepet_v", "event"]),
    ("properties", O([
        ("codepet_v", O([("const", 1)])),
        ("event", O([("enum", KINDS)])),
        ("session_id", O([("$ref", "#/$defs/id")])),
        ("turn_id", O([("$ref", "#/$defs/id")])),
        ("tools", O([("type", "integer"), ("minimum", 0), ("maximum", 1000),
                     ("description", "completed tool calls of the turn, counted in the plugin (turn_done only)")])),
        ("interactive", O([("type", "boolean"), ("description", "OpenCode: false when process.argv holds the exact token run or serve; Hermes never sends false (it drops non-interactive platforms)")])),
        ("approval_key", O([("$ref", "#/$defs/id")])),
        ("codepet_test", O([("type", "string"), ("pattern", "^[0-9a-f]{32}$"), ("description", "only in clients.test ready-check payloads")])),
    ])),
    ("$defs", O([("id", ID)])),
    ("x-per-client", O([
        ("opencode", O([("never", ["approval_key"]), ("turn_id_pattern", "^[0-9]+-[0-9]+$")])),
        ("hermes", O([("approval_events", "event waiting|activity with approval_key and no session_id, for surface cli or transport:* only")])),
    ])),
])

(HERE / "shim.sh.template").write_text(SHIM)
(HERE / "shim-tombstone.sh").write_text(TOMBSTONE)
(HERE / "commands.json").write_text(json.dumps(commands, indent=1, ensure_ascii=False) + "\n")
(HERE / "plugin-payload.schema.json").write_text(json.dumps(payload_schema, indent=1, ensure_ascii=False) + "\n")
print("wrote shim.sh.template, shim-tombstone.sh, commands.json, plugin-payload.schema.json")
