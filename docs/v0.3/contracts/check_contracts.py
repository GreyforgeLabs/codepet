#!/usr/bin/env python3
"""Self-consistency check for the v0.3 WP0 contracts (docs/v0.3/contracts).

Standard library only. Checks:
  * every contract file parses;
  * strings.json: placeholders, overrides, calm-copy list, K1 (no digits), K2 (no jargon),
    K8 (short lines) - Help is exempt from K1/K2/K8, the Codex guide from K1/K2;
  * pixel-budgets.json: parts sum to totals, totals fit the card;
  * icons.json: unique ids, careIconSet for all 15 species against the pet registry;
  * schema-v5.sql: applies on a stub v4 schema and its history triggers fire;
  * bar.v2.schema.json: every fixture validates (a small draft 2020-12 subset);
  * protocol.json: valid param examples match their schemas; result schemas against
    fixtures/clients-status; the v0.3 snapshot additions; the Off table; receipts; errors;
  * agent_vectors.json: replayed through an independent reference model (normalization,
    event ids, HMAC keys, interactivity, turn_qualifies, caps, sessions, fold outcomes, the
    agent::pet_state aggregate, the shared priority, popup routing, the Connect card and the
    Helpers rows over the fixtures), and every line key / placeholder exists in strings.json;
  * commands.json, shim.sh.template, shim-tombstone.sh, plugin-payload.schema.json: hashes,
    Codex trust hashes, ready-check payloads, plugin payloads, and a shim smoke under sh.

Run: python3 docs/v0.3/contracts/check_contracts.py   (exit 1 on any failure)
The reference model is a test oracle for the contract, not product code.
"""
from __future__ import annotations

import hashlib
import hmac
import json
import re
import shutil
import sqlite3
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
FAILS: list[str] = []


def fail(msg: str) -> None:
    FAILS.append(msg)


def load(name: str):
    return json.loads((HERE / name).read_text(encoding="utf-8"))


# ------------------------------------------------------------------ minimal JSON Schema
def _type_ok(v, t) -> bool:
    if isinstance(t, list):
        return any(_type_ok(v, x) for x in t)
    return {
        "null": v is None,
        "boolean": isinstance(v, bool),
        "integer": isinstance(v, int) and not isinstance(v, bool),
        "number": isinstance(v, (int, float)) and not isinstance(v, bool),
        "string": isinstance(v, str),
        "array": isinstance(v, list),
        "object": isinstance(v, dict),
    }[t]


def validate(v, s, root, path="$") -> list[str]:
    errs: list[str] = []
    if "$ref" in s:
        ref = s["$ref"]
        if ref.startswith("bar.v2.schema.json#/$defs/"):
            bar = load("bar.v2.schema.json")
            return validate(v, bar["$defs"][ref.split("/")[-1]], bar, path)
        assert ref.startswith("#/$defs/"), ref
        return validate(v, root["$defs"][ref[len("#/$defs/"):]], root, path)
    if "oneOf" in s:
        ok = [x for x in s["oneOf"] if not validate(v, x, root, path)]
        if len(ok) != 1:
            errs.append(f"{path}: matches {len(ok)} oneOf branches")
        return errs
    if "type" in s and not _type_ok(v, s["type"]):
        return [f"{path}: expected {s['type']}, got {type(v).__name__}"]
    if "const" in s and (v != s["const"] or type(v) is not type(s["const"])):
        errs.append(f"{path}: expected const {s['const']!r}")
    if "enum" in s and v not in s["enum"]:
        errs.append(f"{path}: {v!r} not in enum")
    if isinstance(v, (int, float)) and not isinstance(v, bool):
        if "minimum" in s and v < s["minimum"]:
            errs.append(f"{path}: {v} < minimum")
        if "maximum" in s and v > s["maximum"]:
            errs.append(f"{path}: {v} > maximum")
    if isinstance(v, str):
        if "minLength" in s and len(v) < s["minLength"]:
            errs.append(f"{path}: too short")
        if "maxLength" in s and len(v) > s["maxLength"]:
            errs.append(f"{path}: too long")
        if "pattern" in s and not re.search(s["pattern"], v):
            errs.append(f"{path}: {v!r} does not match {s['pattern']}")
    if isinstance(v, list):
        if "minItems" in s and len(v) < s["minItems"]:
            errs.append(f"{path}: too few items")
        if "maxItems" in s and len(v) > s["maxItems"]:
            errs.append(f"{path}: too many items")
        if s.get("uniqueItems") and len({json.dumps(x, sort_keys=True) for x in v}) != len(v):
            errs.append(f"{path}: items not unique")
        if "items" in s:
            for i, x in enumerate(v):
                errs += validate(x, s["items"], root, f"{path}[{i}]")
    if isinstance(v, dict):
        props = s.get("properties", {})
        for k in s.get("required", []):
            if k not in v:
                errs.append(f"{path}: missing {k}")
        for k, x in v.items():
            if "propertyNames" in s:
                errs += validate(k, s["propertyNames"], root, f"{path}.<{k}>")
            if k in props:
                errs += validate(x, props[k], root, f"{path}.{k}")
            else:
                extra = s.get("additionalProperties", True)
                if extra is False:
                    errs.append(f"{path}: unexpected key {k}")
                elif isinstance(extra, dict):
                    errs += validate(x, extra, root, f"{path}.{k}")
    return errs


# ------------------------------------------------------------------ strings
PLACEHOLDERS = {"name", "app", "app2", "word"}
JARGON = ["session", "hook", "turn", "provider", "xp", "level", "percent", "simulation", "preflight",
          "token", "armed", "attended", "presence", "daemon", "config", "rpc", "plugin", "marketplace",
          "client", "agent", "trust"]
# K1 lists this literal as its one exception; it is also the only K2 hit ("plugin").
EXEMPT_LITERALS = {"hint.claude2": "/reload-plugins"}
# K2 bans "turn" (an agent turn), but the spec's own copy uses the verb: "Turn on",
# "Turned off in Codex", "Turn {name} into…". These verb phrases are allowed (notes.md).
K2_VERB_PHRASES = [r"\bturn(ed)? (on|off)\b", r"\bturn \{name\} into"]


def placeholders(t: str) -> set[str]:
    return set(re.findall(r"\{([a-z0-9]+)\}", t))


def words(t: str) -> int:
    return len([w for w in re.split(r"\s+", t.strip()) if re.search(r"[A-Za-z{]", w)])


def calm_banned() -> list[str]:
    src = (REPO / "qml/tests/copy_lint.py").read_text(encoding="utf-8")
    m = re.search(r"^BANNED = (\[.*?\])", src, re.S | re.M)
    return json.loads(m.group(1).replace("'", '"')) if m else []


def check_strings(strings: dict) -> None:
    banned = calm_banned()
    if not banned:
        fail("strings: could not read BANNED from qml/tests/copy_lint.py")
    kid_lengths = []
    for key, text in strings.items():
        if not isinstance(text, str) or not text:
            fail(f"strings: {key} is empty")
            continue
        bad = placeholders(text) - PLACEHOLDERS
        if bad:
            fail(f"strings: {key} has unknown placeholders {bad}")
        if "{" in re.sub(r"\{[a-z0-9]+\}", "", text) or "}" in re.sub(r"\{[a-z0-9]+\}", "", text):
            fail(f"strings: {key} has a stray brace")
        low = text.lower()
        for term in banned:
            if term == "!!!" and term in low or term != "!!!" and re.search(r"\b" + re.escape(term), low):
                fail(f"strings: {key} uses calm-copy banned term {term!r}")
        for prefix in ("aquatic.", "robot."):
            if key.startswith(prefix) and key[len(prefix):] not in strings:
                fail(f"strings: override {key} has no base key")
        adult_help = key.startswith("help.")
        codex = key.startswith("codex.")
        if adult_help:
            continue
        n = words(text)
        if n > 8:
            fail(f"strings: K8 {key} has {n} words: {text!r}")
        if codex:
            continue
        kid_lengths.append(n)
        scan = text.replace(EXEMPT_LITERALS.get(key, "\0"), "")
        if re.search(r"[0-9%]", re.sub(r"\{[a-z0-9]+\}", "", scan)):
            fail(f"strings: K1 {key} has a digit or %: {text!r}")
        scan_low = scan.lower()
        for phrase in K2_VERB_PHRASES:
            scan_low = re.sub(phrase, " ", scan_low)
        scan_low = re.sub(r"\{[a-z0-9]+\}", "", scan_low)
        for term in JARGON:
            if re.search(r"\b" + term, scan_low):
                fail(f"strings: K2 {key} uses {term!r}: {text!r}")
    short = sum(1 for n in kid_lengths if n <= 6)
    if kid_lengths and short / len(kid_lengths) < 0.9:
        fail(f"strings: K8 only {short}/{len(kid_lengths)} kid lines have <= 6 words")
    required = ["mood.happy", "mood.fainted", "mood.asleep", "off.line", "btn.turn_on", "btn.go", "btn.connect",
                "btn.notnow", "btn.connect_all", "header.on", "header.off", "react.treat", "treats.full",
                "act.ready", "act.hello", "act.oops", "act.waiting", "codex.step5", "reason.blocked", "btn.cancel",
                "goodbye.ask", "goodbye.type", "btn.goodbye", "memories.title", "memories.empty", "picker.welcome",
                "row.disconnecting", "row.testing", "codex.off1", "codex.off2", "help.rules.body", "help.apps.body",
                "help.privacy.body", "help.hardcore.body", "help.helpers.body", "help.forget.body", "grew",
                "when.now", "when.today", "when.yesterday", "when.while", "bar.hi"]
    for k in required:
        if k not in strings:
            fail(f"strings: missing {k}")
    for need in ("food", "water", "play", "wash"):
        for lvl in ("soon", "now", "critical"):
            if f"mood.{need}.{lvl}" not in strings:
                fail(f"strings: missing mood.{need}.{lvl}")


# ------------------------------------------------------------------ budgets, icons
def check_budgets(b: dict) -> None:
    cap = b["card"]["max_height_px"]
    for name, sc in b["screens"].items():
        total = sum(p["px"] for p in sc["parts"])
        if total != sc["total_px"]:
            fail(f"budgets: {name} parts sum {total} != {sc['total_px']}")
        if sc["total_px"] > cap:
            fail(f"budgets: {name} {sc['total_px']} > card cap {cap}")
    if b["card"]["inner_width_px"] != b["card"]["width_px"] - 32:
        fail("budgets: inner width must be width minus 2 x 16 padding")


def check_icons(icons: dict, vectors: dict) -> None:
    for sheet in icons["sheets"]:
        ids = sheet["ids"]
        if len(set(ids)) != len(ids):
            fail(f"icons: duplicate ids in {sheet['id']}")
        cols, rows = sheet["grid"]
        if cols * rows != len(ids):
            fail(f"icons: {sheet['id']} grid {cols}x{rows} != {len(ids)} ids")
    apps = next(s for s in icons["sheets"] if s["id"] == "apps")
    if apps["client_ids"] != [c["id"] for c in vectors["clients"]]:
        fail("icons: apps sheet client order differs from agent_vectors clients")
    reg = json.loads((REPO / "qml/assets/pets/registry.json").read_text(encoding="utf-8"))["species"]
    resolved = icons["care_icon_set"]["resolved"]
    if sorted(resolved) != sorted(reg):
        fail("icons: careIconSet does not cover exactly the registry species")
    for sid, sp in reg.items():
        want = sp.get("careIconSet") or ("robot" if sid == "robot" else
                                         "aquatic" if sp.get("habitat") == "aquatic" else "floor")
        if resolved.get(sid) != want:
            fail(f"icons: careIconSet {sid} = {resolved.get(sid)} expected {want}")
    bar_ids = next(s for s in icons["sheets"] if s["id"] == "bar")["ids"]
    for what, badge in icons["bar_badges"].items():
        if badge not in bar_ids:
            fail(f"icons: bar badge {badge} for {what} is not on the bar sheet")
    all_ids = {i for sh in icons["sheets"] for i in sh["ids"]}
    for need in ("egg", "pencil"):
        if need not in all_ids:
            fail(f"icons: {need} missing")
    cell = icons["bar_cell"]
    bars = {v["expect"]["bar"] for v in vectors["priority"]["vectors"]}
    for b in sorted(bars - set(cell)):
        fail(f"icons: bar_cell has no entry for priority bar value {b!r}")
    for name, spec in cell.items():
        if name == "description":
            continue
        for k in ("thumb", "badge"):
            val = spec.get(k)
            if isinstance(val, str) and val.startswith("badge_") and val not in bar_ids:
                fail(f"icons: bar_cell {name}.{k} names unknown badge {val}")


# ------------------------------------------------------------------ DDL
STUB_V4 = """
CREATE TABLE meta (id INTEGER PRIMARY KEY CHECK (id = 1), state_version INTEGER NOT NULL,
  presence_rejections INTEGER NOT NULL DEFAULT 0);
INSERT INTO meta (id, state_version) VALUES (1, 7);
CREATE TABLE lives (life_id TEXT PRIMARY KEY NOT NULL);
INSERT INTO lives VALUES ('life-1');
"""


def check_ddl() -> None:
    db = sqlite3.connect(":memory:")
    db.execute("PRAGMA foreign_keys = ON")
    try:
        db.executescript(STUB_V4)
        db.executescript((HERE / "schema-v5.sql").read_text(encoding="utf-8"))
    except sqlite3.Error as e:
        fail(f"ddl: schema-v5.sql does not apply: {e}")
        return
    row = db.execute("SELECT enabled, count_helpers, finished_seq, hello_seq FROM meta").fetchone()
    if row != (1, 0, 0, 0):
        fail(f"ddl: meta defaults {row}")
    k = "a" * 32
    db.execute("INSERT INTO activity_turns (session_key, turn_key, client, life_id, earned_at_ms) "
               "VALUES (?, ?, 'claude-code', 'life-1', 5)", (k, k))
    for sql, why in [
        ("INSERT INTO activity_turns (session_key, turn_key, client, life_id, earned_at_ms) "
         f"VALUES ('{k}', '{k}', 'codex', 'life-1', 6)", "X1 unique (session, turn)"),
        ("DELETE FROM activity_turns", "turns are never deleted"),
        ("INSERT INTO activity_xp (life_id, turn_row_id, amount, granted_at_ms) VALUES ('life-1', 1, 6, 1)",
         "treat XP is exactly 5"),
        ("INSERT INTO agent_events (dedupe_key, client, kind, at_ms, outcome, interactive) "
         f"VALUES ('{k}', 'crush', 'activity', 1, 'folded', 1)", "unsupported client"),
        ("INSERT INTO client_hooks (client, needs_step) VALUES ('codex', 'trust_me')", "needs_step enum"),
    ]:
        try:
            db.execute(sql)
            fail(f"ddl: accepted a row that must fail ({why})")
        except sqlite3.Error:
            pass
    db.execute("UPDATE activity_turns SET given_at_ms = 9")
    try:
        db.execute("UPDATE activity_turns SET given_at_ms = NULL")
        fail("ddl: a given treat returned to the jar")
    except sqlite3.Error:
        pass
    db.execute("INSERT INTO activity_xp (life_id, turn_row_id, amount, granted_at_ms) VALUES ('life-1', 1, 5, 9)")
    try:
        db.execute("DELETE FROM activity_xp")
        fail("ddl: xp rows deletable")
    except sqlite3.Error:
        pass
    db.execute("INSERT INTO client_hooks (client) VALUES ('hermes')")
    if db.execute("SELECT installed, detected_version, checked_at_ms, connected FROM client_hooks").fetchone() != (0, None, None, 0):
        fail("ddl: client_hooks defaults")
    db.execute("UPDATE client_hooks SET job_id = 'job-7f3a0c19e2d84b6a5c1e9f02', job_kind = 'connect'")
    for bad in ("job-7f3a", "job-7F3A0C19E2D84B6A5C1E9F02", "../7f3a0c19e2d84b6a5c1e9f0", "job-7f3a0c19e2d84b6a5c1e9f0/"):
        try:
            db.execute("UPDATE client_hooks SET job_id = ?", (bad,))
            fail(f"ddl: accepted job id {bad!r}")
        except sqlite3.Error:
            pass
    db.execute("INSERT INTO agent_events (dedupe_key, client, kind, at_ms, outcome, interactive, test) "
               f"VALUES ('{'b' * 32}', 'codex', 'turn_done', 1, 'dropped_test', 1, 1)")
    db.execute("UPDATE meta SET test_state = 'running', test_client = 'codex', test_at_ms = 1")


# ------------------------------------------------------------------ reference model
ID_RE = re.compile(r"^[A-Za-z0-9._:-]{1,128}$")
NONCE_RE = re.compile(r"^[0-9a-f]{32}$")
ERRORS = {"rate_limit", "authentication_failed", "invalid_request", "server_error", "max_output_tokens",
          "overloaded", "billing_error", "unknown"}
GROK_SNAKE = {"session_start": "SessionStart", "user_prompt_submit": "UserPromptSubmit", "pre_tool_use": "PreToolUse",
              "notification": "Notification", "stop": "Stop", "stop_failure": "StopFailure",
              "stop_cancelled": "StopCancelled", "session_end": "SessionEnd"}


def normalize(client: str, p: dict, kinds: list[str], nonce=None) -> dict:
    if "agent_id" in p or "subagentType" in p:
        return {"drop": "child"}
    test = False
    if "codepet_test" in p:
        t = p["codepet_test"]
        if not isinstance(t, str) or not NONCE_RE.match(t) or nonce is None or t != nonce:
            return {"drop": "test_nonce"}
        test = True

    def ident(*keys):
        for k in keys:
            if k in p:
                v = p[k]
                if not isinstance(v, str) or not ID_RE.match(v):
                    raise ValueError(k)
                return v
        return None

    try:
        if client in ("opencode", "hermes"):
            kind = p.get("event")
            v = p.get("codepet_v")
            if type(v) is not int or v != 1 or kind not in kinds:
                return {"drop": "unmapped"}
            session, turn = ident("session_id"), ident("turn_id")
            akey = ident("approval_key") if client == "hermes" else None
            if akey is not None:
                if session is not None:
                    return {"drop": "bad_id"}
                if kind not in ("waiting", "activity"):
                    return {"drop": "unmapped"}
            elif session is None:
                return {"drop": "bad_id"}
            tools = p.get("tools", 0)
            if type(tools) is not int or not 0 <= tools <= 1000:
                tools = 0
            inter = p.get("interactive", True)
            return {"kind": kind, "session": session, "turn": turn, "tool": None, "class": None, "akey": akey,
                    "tools": tools, "interactive": inter if isinstance(inter, bool) else True, "test": test}
        if client == "grok-cli":
            name = p.get("hook_event_name")
            if name is None:
                name = GROK_SNAKE.get(p.get("hookEventName"))
        else:
            name = p.get("hook_event_name")
        session = ident("session_id", "sessionId")
        turn = ident("prompt_id", "promptId", "turn_id")
        tool = ident("tool_use_id", "toolUseId")
    except ValueError:
        return {"drop": "bad_id"}
    if session is None:
        return {"drop": "bad_id"}
    note = p.get("notification_type", p.get("notificationType"))
    tool_name = p.get("tool_name", p.get("toolName"))
    base = {"session": session, "turn": turn, "tool": None, "class": None, "test": test}

    def out(kind, **kw):
        d = dict(base, kind=kind)
        d.update(kw)
        return d

    if client == "claude-code":
        if name == "SessionStart":
            return out("session_start", turn=None)
        if name == "UserPromptSubmit":
            return out("turn_start")
        if name == "PreToolUse":
            return out("activity", tool=tool)
        if name == "Notification":
            if note in ("permission_prompt", "elicitation_dialog", "elicitation_url_dialog", "agent_needs_input"):
                return out("waiting", turn=None, **{"class": note})
            if note == "idle_prompt":
                return out("idle_prompt", turn=None, **{"class": "idle_prompt"})
            return {"drop": "unmapped"}
        if name == "Stop":
            return out("turn_done")
        if name == "StopFailure":
            err = p.get("error")
            return out("turn_failed", **{"class": err if err in ERRORS else "unknown"})
        if name == "SessionEnd":
            return out("session_end", turn=None)
        return {"drop": "unmapped"}
    if client == "codex":
        m = {"UserPromptSubmit": "turn_start", "PermissionRequest": "waiting", "PostToolUse": "activity",
             "Stop": "turn_done", "Interrupt": "turn_cancelled", "SessionEnd": "session_end"}
        if name not in m:
            return {"drop": "unmapped"}
        return out(m[name], tool=tool if name == "PostToolUse" else None)
    if client == "grok-cli":
        if name == "SessionStart":
            return out("session_start", turn=None)
        if name == "UserPromptSubmit":
            return out("turn_start")
        if name == "PreToolUse":
            if tool_name in ("AskUserQuestion", "ask_user_question"):
                return out("waiting", tool=tool, **{"class": "ask_user"})
            return out("activity", tool=tool)
        if name == "Notification":
            if note == "permission_prompt":
                return out("waiting", turn=None, **{"class": note})
            if note == "idle_prompt":
                return out("idle_prompt", turn=None, **{"class": note})
            return {"drop": "unmapped"}
        if name == "Stop":
            r = p.get("reason")
            if r == "end_turn":
                return out("turn_done")
            if r in ("channel_closed", "shutdown"):
                return out("session_end", turn=None)
            return {"drop": "unmapped"}
        if name == "StopFailure":
            err = p.get("error")
            return out("turn_failed", **{"class": err if err in ERRORS else "unknown"})
        if name == "StopCancelled":
            return out("turn_cancelled")
        if name == "SessionEnd":
            return out("session_end", turn=None)
        return {"drop": "unmapped"}
    return {"drop": "unmapped"}


def interactive(v: dict, rules: dict):
    c = v["client"]
    argv = v.get("ancestor_argv")
    if c == "hermes":
        if "surface" in v:
            sf = v["surface"]
            return True if sf == "cli" or (isinstance(sf, str) and sf.startswith("transport:")) else "not_sent"
        return True if v.get("platform") in rules["hermes_platforms"] else "not_sent"
    if c == "opencode":
        return v.get("payload_interactive") is not False
    if c == "claude-code":
        ep = v.get("env", {}).get("CLAUDE_CODE_ENTRYPOINT")
        if ep is not None:
            return not ep.startswith("sdk")
        return not (argv and any(t in ("-p", "--print") for t in argv[1:]))
    if c == "codex":
        if not argv:
            return True
        return not ((len(argv) > 1 and argv[1] == "e") or any(t in ("exec", "review") for t in argv[1:]))
    if c == "grok-cli":
        if not argv:
            return True
        toks = argv[1:]
        if any(t in rules["grok_headless_flags"] for t in toks):
            return False
        if any(t.startswith(pfx) for t in toks for pfx in rules["grok_headless_prefixes"]):
            return False
        return not any(a == "agent" and b == "headless" for a, b in zip(toks, toks[1:]))
    return True


def qualifies(open_, tools, duration, inter, test, C) -> bool:
    return bool(open_ and (tools >= 1 or duration >= C["min_turn_ms"]) and inter and not test)


def caps(at, jar, session_earned, all_earned, C) -> str:
    if jar >= C["jar_max"]:
        return "jar_full"
    if any(e > 0 and e > at - C["cap_session_ms"] for e in session_earned):
        return "capped_session"
    if sum(1 for e in all_earned if e > 0 and e > at - 3600000) >= C["cap_per_hour"]:
        return "capped_hour"
    if sum(1 for e in all_earned if e > 0 and e > at - 86400000) >= C["cap_per_day"]:
        return "capped_day"
    return "allow"


class World:
    def __init__(self, C, jar=0):
        self.C = C
        self.jar = jar
        self.earned: list[tuple[str, int]] = []  # (session, earned_at)
        self.earned_keys: set[tuple[str, str]] = set()
        self.finished_seq = 0
        self.finished_jar_full = False
        self.sessions: dict[str, dict] = {}
        self.approvals: dict[str, str] = {}

    def treat_check(self, sid, key, tools, duration, inter, test, at) -> bool:
        if (sid, key) in self.earned_keys or not qualifies(True, tools, duration, inter, test, self.C):
            return False
        verdict = caps(at, self.jar, [e for s, e in self.earned if s == sid], [e for _, e in self.earned], self.C)
        self.finished_jar_full = verdict == "jar_full"
        if verdict != "allow":
            return False
        self.earned.append((sid, at))
        self.earned_keys.add((sid, key))
        self.jar += 1
        return True

    def apply(self, sid, inter, e) -> bool:
        """Applies one event; returns True when it dropped a treat."""
        if e.get("akey"):
            if e["kind"] == "waiting":
                self.approvals[e["akey"]] = "waiting"
            elif e["kind"] == "activity":
                self.approvals[e["akey"]] = "idle"
            return False
        s = self.sessions.setdefault(sid, {"state": "idle", "open": None, "done": None, "error_at": None})
        kind, at, key, tool = e["kind"], e["at"], e.get("turn"), e.get("tool")
        late = self.C["late_tool_ms"]
        o, d = s["open"], s["done"]
        earned = False
        if kind == "session_start":
            s["state"] = "idle"
        elif kind == "turn_start":
            s["open"] = {"key": key, "start": at, "tools": set(), "extra": 0}
            s["state"] = "working"
        elif kind == "activity":
            new_late_tool = bool(d and tool and key == d["key"] and tool not in d["tools"])
            if o and (key == o["key"] or key is None):
                if tool:
                    o["tools"].add(tool)
                if s["state"] in ("waiting", "idle"):
                    s["state"] = "working"
            elif new_late_tool and at <= d["at"] + late:
                d["tools"].add(tool)
                if not d["earned"] and not d["rechecked"]:
                    d["rechecked"] = True
                    n = len(d["tools"]) + d["extra"]
                    if self.treat_check(sid, d["key"], n, d["at"] - d["start"], inter, False, d["at"]):
                        d["earned"] = earned = True
            elif new_late_tool:
                s["open"] = {"key": key, "start": d["start"], "tools": set(d["tools"]) | {tool}, "extra": d["extra"]}
                s["state"] = "working"
            elif o is None and s["state"] == "waiting":
                s["state"] = "idle"
        elif kind == "waiting":
            s["state"] = "waiting"
        elif kind == "idle_prompt":
            s["state"] = "idle"
        elif kind in ("turn_done", "turn_failed", "turn_cancelled"):
            match = o is not None and (key == o["key"] if key is not None else o["start"] < at)
            if match:
                s["open"] = None
                if kind == "turn_done":
                    s["state"] = "idle"
                    if inter:
                        # finished_jar_full describes this finished turn only (notes.md §5).
                        self.finished_seq += 1
                        self.finished_jar_full = False
                    extra = o["extra"] + (e.get("tools") or 0)
                    n = len(o["tools"]) + extra
                    prev = d if d and d["key"] == o["key"] else None
                    got = self.treat_check(sid, o["key"], n, at - o["start"], inter, False, at)
                    s["done"] = {"key": o["key"], "at": at, "start": o["start"], "tools": set(o["tools"]),
                                 "extra": extra, "earned": got or bool(prev and prev["earned"]),
                                 "rechecked": False}
                    earned = got
                elif kind == "turn_failed":
                    s["state"] = "error"
                    s["error_at"] = at
                else:
                    s["state"] = "idle"
        elif kind == "session_end":
            s["open"] = None
            s["state"] = "ended"
        return earned


def check_sessions(V: dict) -> None:
    C = V["constants"]
    for vec in V["sessions"]["vectors"]:
        w = World(C, jar=vec.get("jar", 0))
        sid = vec.get("session", "s1")
        inter = vec.get("interactive", True)
        for i, e in enumerate(vec["events"]):
            got = w.apply(sid, inter, e)
            st = w.sessions.get(sid, {"state": "idle"})["state"]
            where = f"sessions: {vec['name']} event {i} ({e['kind']}@{e['at']})"
            if e.get("expect") and st != e["expect"]:
                fail(f"{where}: state {st} != {e['expect']}")
            if got != bool(e.get("earned")):
                fail(f"{where}: earned {got} != {bool(e.get('earned'))}")
            if "expect_approval" in e and w.approvals.get(e["akey"]) != e["expect_approval"]:
                fail(f"{where}: approval row {w.approvals.get(e['akey'])} != {e['expect_approval']}")
        f = vec["final"]
        s = w.sessions.get(sid, {"open": None, "done": None, "error_at": None})
        open_key = s["open"]["key"] if s["open"] else None
        done_tools = (len(s["done"]["tools"]) + s["done"]["extra"]) if s["done"] else 0
        checks = {"treats": len(w.earned), "finished_seq": w.finished_seq, "open_turn": open_key,
                  "last_done_tools": done_tools}
        if "error_at_ms" in f:
            checks["error_at_ms"] = s["error_at"]
        if "approval_rows" in f:
            checks["approval_rows"] = w.approvals
        if "finished_jar_full" in f:
            checks["finished_jar_full"] = w.finished_jar_full
        for k, got in checks.items():
            if f.get(k) != got:
                fail(f"sessions: {vec['name']} final {k} = {got!r}, expected {f.get(k)!r}")


def check_fold(V: dict) -> None:
    C = V["constants"]
    now = V["fold"]["now"]
    for vec in V["fold"]["vectors"]:
        e = vec["event"]
        at = e["at"]
        hello = record = False
        test_state = vec.get("test_state")
        if not vec["enabled"]:
            outcome = "dropped_off"
        elif at < now - C["max_event_age_ms"]:
            outcome = "dropped_old"
        else:
            if at > now + C["future_clamp_ms"]:
                at = now
            if e.get("test"):
                outcome = "test" if test_state == "running" else "dropped_test"
            else:
                record = True
                hello = not vec.get("first_seen", False)
                outcome = "folded" if vec["life"] else "no_life"
        x = vec["expect"]
        got = {"outcome": outcome, "client_record": record, "hello": hello}
        if "at" in x:
            got["at"] = at
        if "test_state" in x:
            got["test_state"] = test_state
        for k, v in got.items():
            if x.get(k) != v:
                fail(f"fold: {vec['name']} {k} = {v!r}, expected {x.get(k)!r}")


ORDER = ["claude-code", "codex", "opencode", "hermes", "grok-cli"]
RANK = {"error": 1, "working": 2, "waiting": 3}


def pet_state(rows: list, count_helpers: bool, now: int, C: dict) -> dict:
    """Reference for Rust agent::pet_state (the aggregate behind export.agent)."""
    def expiry(r):
        if r["state"] == "working":
            return r["last_event_ms"] + C["stale_working_ms"]
        if r["state"] == "waiting":
            return r["last_event_ms"] + C["stale_waiting_ms"]
        if r["state"] == "error" and r["error_at_ms"] is not None:
            return r["error_at_ms"] + C["error_recent_ms"]
        return None

    per_app: dict[str, list] = {}
    for r in rows:
        x = expiry(r)
        if r["interactive"] == 1 and x is not None and now < x:
            per_app.setdefault(r["client"], []).append((r, x))
    apps = []
    for cid, rs in per_app.items():
        top = max(RANK[r["state"]] for r, _ in rs)
        win = [(r, x) for r, x in rs if RANK[r["state"]] == top]
        apps.append({"id": cid, "state": win[0][0]["state"], "since_ms": min(r["state_since_ms"] for r, _ in win),
                     "stale_at_ms": max(x for _, x in win)})
    apps.sort(key=lambda a: (a["since_ms"], ORDER.index(a["id"])))
    errs = [r for r in rows if r["interactive"] == 1 and r["approval"] == 0 and r["error_at_ms"] is not None
            and now < r["error_at_ms"] + C["error_recent_ms"]]
    errs.sort(key=lambda r: (-r["error_at_ms"], ORDER.index(r["client"])))
    helpers = count_helpers and any(r["interactive"] == 0 and r["state"] == "working"
                                    and now < r["last_event_ms"] + C["stale_working_ms"] for r in rows)
    return {"state": max((a["state"] for a in apps), key=RANK.get, default="idle"), "apps": apps,
            "stale_at_ms": min((a["stale_at_ms"] for a in apps), default=None),
            "error_at_ms": errs[0]["error_at_ms"] if errs else None, "error_app": errs[0]["client"] if errs else None,
            "helpers_busy": bool(helpers)}


def check_aggregate(V: dict, bar_schema: dict) -> None:
    C = V["constants"]
    for vec in V["aggregate"]["vectors"]:
        got = pet_state(vec["sessions"], vec["count_helpers"], vec["now"], C)
        if got != vec["expect"]:
            fail(f"aggregate: {vec['name']} = {got}, expected {vec['expect']}")
        for i, a in enumerate(vec["expect"]["apps"]):
            for e in validate(a, bar_schema["$defs"]["app"], bar_schema, f"aggregate {vec['name']} apps[{i}]"):
                fail(e)


def check_hmac(V: dict) -> None:
    H = V["hmac_keys"]
    salt = bytes.fromhex(H["salt_hex"])
    for vec in H["vectors"]:
        dom = H["domains"][vec["use"]]["domain"]
        msg = "\x1f".join([dom, vec["client"], *vec["parts"]]).encode()
        got = hmac.new(salt, msg, hashlib.sha256).hexdigest()[:32]
        if got != vec["expect"]:
            fail(f"hmac_keys: {vec['use']} {vec['client']} {vec['parts']}")
    keys = [v["expect"] for v in H["vectors"]]
    if len(set(keys)) != len(keys):
        fail("hmac_keys: two different inputs share a key")


def shared_priority(doc: dict, now: int, seen: dict, P: dict, C: dict) -> dict:
    """Reference for petstate.js sharedPriority (one order for the bar, Home and the overlay)."""
    life = doc["life"]
    res = {"state": None, "bar": None, "line": None, "vars": {}, "overlay": "none", "pulse": False,
           "treats_visible": False, "bubble": None, "wake_at_ms": None}
    if not doc["enabled"]:
        res.update(state="off", bar="off", line="off.line" if life else None,
                   vars={"name": life["name"]} if life else {})
        return res
    if life is None:
        res.update(state="no_pet", bar="egg")
        return res
    name = life["name"]
    if not life["armed"]:
        res.update(state="welcome", bar="hi", line="picker.welcome", vars={"name": name}, overlay="wander")
        return res
    ag = doc["agent"]
    counting = [a for a in ag["apps"] if a["stale_at_ms"] > now]
    working = [a["id"] for a in counting if a["state"] == "working"]
    waiting = sorted((a for a in counting if a["state"] == "waiting"),
                     key=lambda a: (-a["since_ms"], ORDER.index(a["id"])))
    jar = doc["treats"]["jar"]
    sleeping = doc["care"]["sleeping"]
    tv = jar > 0 and not life["fainted"] and not life["paused"] and not sleeping
    res["treats_visible"] = tv
    wakes = [a["stale_at_ms"] for a in counting]
    W, G = C["celebrate_window_ms"], C["celebrate_grace_ms"]
    t = ag["test"]
    testing = False
    if t and t["state"] == "running":
        end = t["at_ms"] + C["test_done_delay_ms"] + C["test_arrival_ms"]
        if now < end:
            testing = True
            wakes.append(end)
    cands = []
    if doc["unseen_events"]:
        cands.append(("grew", "grew", {"name": name}))
    if t and t["state"] == "done" and t["at_ms"] > seen["test_at_ms"] and now < t["at_ms"] + W:
        wakes.append(t["at_ms"] + W)
        cands.append(("test", "act.ready", {"name": name}))
    if ag["hello_seq"] > seen["hello_seq"] and ag["hello_at_ms"] is not None and now < ag["hello_at_ms"] + W:
        wakes.append(ag["hello_at_ms"] + W)
        cands.append(("hello", "act.hello", {"name": name, "app": ag["hello_app"]}))
    fa = ag["finished_at_ms"]
    if ag["finished_seq"] > seen["finished_seq"] and fa is not None and now < fa + W:
        if now < fa + G:
            wakes.append(fa + G)
        else:
            wakes.append(fa + W)
            if ag["finished_app"] not in working:
                if ag["finished_jar_full"]:
                    cands.append(("finished", "act.done_full", {}))
                elif ag["finished_treat"]:
                    cands.append(("finished", "act.done_treat", {"name": name}))
                else:
                    cands.append(("finished", "act.done", {}))
    ea = ag["error_at_ms"]
    oops = ea is not None and now < ea + C["error_recent_ms"]
    if ea is not None:
        for end in (ea + C["error_frame_ms"], ea + C["error_recent_ms"]):
            if end > now:
                wakes.append(end)
    res["wake_at_ms"] = min(wakes) if wakes else None
    level = P["need_levels"]
    groups = {"now": [], "soon": []}
    for n in doc["needs"]:
        lv = level[n["state"]]
        if lv in ("now", "critical"):
            groups["now"].append((n["action"], lv))
        elif lv == "soon":
            groups["soon"].append((n["action"], lv))
    for g in groups.values():
        g.sort(key=lambda x: P["need_priority"].index(x[0]))
    star = "star" if tv else "none"
    if life["fainted"]:
        res.update(state="fainted", bar="sleep_bandage", line="mood.fainted", vars={"name": name},
                   overlay="sleep", bubble={"bandage": True})
    elif life["paused"]:
        res.update(state="paused", bar="sleep", line="mood.asleep", vars={"name": name}, overlay="sleep")
    elif sleeping:
        res.update(state="asleep", bar="sleep", line="mood.asleep", vars={"name": name}, overlay="sleep")
    elif waiting:
        res.update(state="waiting", bar="hand", line="act.waiting", vars={"app": waiting[0]["id"]},
                   overlay="waiting", pulse=True, bubble={"apps": [waiting[0]["id"]]})
    elif groups["now"]:
        need, lv = groups["now"][0]
        res.update(state="need_now", bar=f"need_{need}", line=f"mood.{need}.{lv}", vars={"name": name},
                   overlay="needs_care", bubble={"need": need})
    elif working:
        if len(working) == 1:
            line, vars_ = "act.working.one", {"name": name, "app": working[0]}
        elif len(working) == 2:
            line, vars_ = "act.working.two", {"name": name, "app": working[0], "app2": working[1]}
        else:
            line, vars_ = "act.working.many", {"name": name}
        res.update(state="working", bar="working", line=line, vars=vars_, overlay="working",
                   bubble={"apps": working[:3]})
    elif testing:
        res.update(state="testing", bar="working", line="act.working.one", vars={"name": name, "app": t["client"]},
                   overlay="working", bubble={"apps": [t["client"]]})
    elif cands:
        kind, line, vars_ = cands[0]
        res.update(state="celebrate", bar="star", line=line, vars=vars_, overlay="celebrate",
                   bubble={"star": True, "confetti": True} if kind == "grew" else {"star": True})
    elif oops:
        res.update(state="oops", bar=star, line="act.oops", vars={"app": ag["error_app"]},
                   overlay="error" if now < ea + C["error_frame_ms"] else "idle")
    elif groups["soon"]:
        need, _ = groups["soon"][0]
        res.update(state="need_soon", bar=star, line=f"mood.{need}.soon", vars={"name": name},
                   overlay="needs_care_frame", bubble={"need": need})
    else:
        res.update(state="idle", bar=star, line="mood.happy", vars={"name": name}, overlay="wander",
                   bubble={"helpers": True} if ag["helpers_busy"] else None)
    return res


def route(doc: dict) -> str:
    life = doc["life"]
    if not doc["enabled"]:
        return "off_look" if life else "first_run_off"
    if life is None:
        return "memorial" if doc["memorial"]["count"] > 0 else "first_run"
    return "welcome_back" if not life["armed"] else "home"


REASON = {"too_old": "reason.too_old", "not_installed": "reason.not_installed", "blocked": "reason.blocked",
          "settings_changed": "reason.blocked", "ready_check": "reason.blocked"}


def bucket(seen: int, now: int, tz: int, just_now: int) -> str:
    if now - seen < just_now:
        return "row.seen.now"
    day = lambda ms: (ms + tz * 60000) // 86400000  # noqa: E731
    if day(seen) == day(now):
        return "row.seen.today"
    if day(seen) == day(now) - 1:
        return "row.seen.yesterday"
    return "row.seen.while"


def helper_row(cid: str, rec, now: int, tz: int, just_now: int) -> dict:
    if rec is None or (not rec["installed"] and not rec["connected"]):
        return {"visible": False}
    r = {"visible": True, "row": None, "hint": None, "sheet_hints": [], "reason": None, "step": None,
         "stop_watching": False, "spinner": False, "guide": None}
    job = rec["job"]
    if job:
        r.update(row={"connect": "row.connecting", "disconnect": "row.disconnecting", "test": "row.testing"}[job["kind"]],
                 spinner=True)
    elif rec["failed_reason"]:
        r.update(row="row.failed", reason=REASON[rec["failed_reason"]], step="btn.retry", stop_watching=rec["connected"])
    elif rec["needs_step"] == "codex_hooks_off":
        r.update(row="row.codex.off", step="btn.showme", guide="codex.off", stop_watching=rec["connected"])
    elif rec["needs_step"] == "codex_trust":
        r.update(row="row.step", step="btn.showme", guide="codex.trust", stop_watching=rec["connected"])
    elif rec["connected"] and rec["first_seen_ms"] is None:
        hints = ["hint.open"] + (["hint.claude2"] if cid == "claude-code" else [])
        r.update(row="row.waiting", hint="hint.open", sheet_hints=hints, stop_watching=True)
    elif rec["connected"]:
        seen = rec["last_seen_ms"] if rec["last_seen_ms"] is not None else rec["first_seen_ms"]
        r.update(row=bucket(seen, now, tz, just_now), stop_watching=True)
    elif cid == "codex":
        r.update(row="row.codex.new", step="btn.connect")
    else:
        r.update(row="row.connect", step="btn.connect")
    return r


def connect_all(clients: dict) -> list:
    return [c for c in ORDER if c in clients and c != "codex" and clients[c]["installed"]
            and not clients[c]["connected"] and clients[c]["job"] is None]


def check_ui_vectors(V: dict, strings: dict, fixtures: dict, used: set) -> None:
    for vec in V["popup_routing"]["vectors"]:
        used.add(vec["fixture"])
        got = route(fixtures[vec["fixture"]])
        if got != vec["expect"]:
            fail(f"popup_routing: {vec['fixture']} -> {got}, expected {vec['expect']}")
    H = V["helpers_rows"]
    just_now = H["seen_buckets"]["seen_just_now_ms"]
    for vec in H["fixture_vectors"]:
        used.add(vec["fixture"])
        clients = fixtures[vec["fixture"]]["clients"]
        for cid, want in vec["expect"]["rows"].items():
            got = helper_row(cid, clients.get(cid), vec["now"], vec["tz_offset_min"], just_now)
            if got != want:
                fail(f"helpers_rows: {vec['fixture']} {cid} = {got}, expected {want}")
            for k in ("row", "hint", "reason", "step"):
                if want.get(k) and want[k] not in strings:
                    fail(f"helpers_rows: {vec['fixture']} {cid} {k} {want[k]} not in strings.json")
        if connect_all(clients) != vec["expect"]["connect_all"]:
            fail(f"helpers_rows: {vec['fixture']} connect_all {connect_all(clients)}")
    for vec in H["bucket_vectors"]:
        rec = {"installed": True, "connected": True, "needs_step": None, "failed_reason": None, "job": None,
               "last_seen_ms": vec["last_seen_ms"], "first_seen_ms": vec["first_seen_ms"]}
        got = helper_row("grok-cli", rec, vec["now"], vec["tz_offset_min"], just_now)["row"]
        if got != vec["expect"]:
            fail(f"helpers_rows: bucket {vec['name']} = {got}, expected {vec['expect']}")
    status_dir = HERE / "fixtures/clients-status"
    for vec in V["connect_card"]["vectors"]:
        doc = json.loads((status_dir / f"{vec['status_fixture']}.json").read_text())
        clients = {c["id"]: c for c in doc["clients"]}
        if connect_all(clients) != vec["expect"]:
            fail(f"connect_card: {vec['status_fixture']} offers {connect_all(clients)}, expected {vec['expect']}")


def check_priority(V: dict, strings: dict, schema: dict, fixtures: dict, used: set) -> None:
    P, C = V["priority"], V["constants"]
    for vec in P["vectors"]:
        doc = fixtures.get(vec["fixture"])
        if doc is None:
            fail(f"priority: {vec['name']} names missing fixture {vec['fixture']}")
            continue
        used.add(vec["fixture"])
        got = shared_priority(doc, vec["now"], vec["seen"], P, C)
        for k, v in vec["expect"].items():
            if got.get(k) != v:
                fail(f"priority: {vec['name']} {k} = {got.get(k)!r}, expected {v!r}")
        line = vec["expect"]["line"]
        if line is not None:
            if line not in strings:
                fail(f"priority: {vec['name']} line {line} not in strings.json")
            elif placeholders(strings[line]) != set(vec["expect"]["vars"]):
                fail(f"priority: {vec['name']} vars {sorted(vec['expect']['vars'])} != placeholders of {line}")
    for name, doc in fixtures.items():
        errs = validate(doc, schema, schema)
        for e in errs:
            fail(f"fixture {name}: {e}")
        if len((HERE / "fixtures/bar-v2" / f"{name}.json").read_bytes()) > 16384:
            fail(f"fixture {name}: over 16 KiB")
        ag = doc["agent"]
        # The export is internally consistent with the aggregate's rules.
        if ag["apps"]:
            want = max((a["state"] for a in ag["apps"]), key=RANK.get)
            if ag["state"] != want or ag["stale_at_ms"] != min(a["stale_at_ms"] for a in ag["apps"]):
                fail(f"fixture {name}: agent.state/stale_at_ms disagree with apps")
        if doc["treats"]["full"] != (doc["treats"]["jar"] >= C["jar_max"]):
            fail(f"fixture {name}: treats.full disagrees with jar")


def check_vectors(V: dict, strings: dict, schema: dict) -> None:
    C = V["constants"]
    kinds = V["kinds"]
    for vec in V["normalization"]["vectors"]:
        got = normalize(vec["client"], vec["payload"], kinds, vec.get("nonce"))
        for k, v in vec["expect"].items():
            if got.get(k) != v:
                fail(f"normalization: {vec['client']} {json.dumps(vec['payload'])[:60]} {k} = {got.get(k)!r}, expected {v!r}")
        if "drop" in got and got["drop"] not in V["normalization"]["drop_reasons"]:
            fail(f"normalization: unknown drop reason {got['drop']}")
    if not set(V["event_ids"]["state_kinds"]) <= set(kinds):
        fail("event_ids: state_kinds must be event kinds")
    for vec in V["event_ids"]["vectors"]:
        body = vec["payload_utf8"].encode()
        dig = hashlib.sha256(body).hexdigest()[:16]
        at = vec["at_ms"]
        bucket = at if vec["kind"] in V["event_ids"]["state_kinds"] else at // C["event_bucket_ms"]
        key = f"codepet-ev1|{vec['client']}|{vec['kind']}|{vec['session']}|{dig}|{bucket}"
        eid = hashlib.sha256(key.encode()).hexdigest()[:32]
        if {"dig": dig, "event_id": eid} != vec["expect"]:
            fail(f"event_ids: {vec['kind']}@{vec['at_ms']}")
    rules = V["interactivity"]
    for vec in rules["vectors"]:
        if interactive(vec, rules) != vec["expect"]:
            fail(f"interactivity: {vec}")
    for vec in V["turn_qualifies"]["vectors"]:
        got = qualifies(vec["open"], vec["tools"], vec["duration_ms"], vec["interactive"], vec["test"], C)
        if got != vec["expect"]:
            fail(f"turn_qualifies: {vec}")
    for vec in V["caps"]["vectors"]:
        got = caps(vec["at"], vec["jar"], vec["session_earned"], vec["all_earned"], C)
        if got != vec["expect"]:
            fail(f"caps: {vec['name']} = {got}, expected {vec['expect']}")
    check_sessions(V)
    check_fold(V)
    check_aggregate(V, schema)
    check_hmac(V)
    fixtures = {p.stem: json.loads(p.read_text()) for p in (HERE / "fixtures/bar-v2").glob("*.json")}
    used: set[str] = set()
    check_priority(V, strings, schema, fixtures, used)
    check_ui_vectors(V, strings, fixtures, used)
    unused = set(fixtures) - used
    if unused:
        fail(f"vectors: bar fixtures no vector uses: {sorted(unused)}")
    if [c["id"] for c in V["clients"]] != ORDER:
        fail("vectors: client order")


def check_commands(V: dict) -> None:
    cmd = load("commands.json")
    ref = cmd["reference_paths"]
    shim_t = (HERE / "shim.sh.template").read_text()
    tomb = (HERE / "shim-tombstone.sh").read_bytes()
    if shim_t.count("@CODEPET_STATE_DIR@") != 1 or shim_t.count("@CODEPET_BIN@") != 1:
        fail("commands: the shim template needs each placeholder exactly once")
    shim_ref = shim_t.replace("@CODEPET_STATE_DIR@", ref["state_dir"]).replace("@CODEPET_BIN@", ref["bin"])
    if hashlib.sha256(shim_ref.encode()).hexdigest() != cmd["shim"]["reference_sha256"]:
        fail("commands: shim reference_sha256")
    if hashlib.sha256(tomb).hexdigest() != cmd["shim"]["tombstone_sha256"]:
        fail("commands: tombstone_sha256")
    if "# managed by codepet" not in shim_t or b"# managed by codepet: tombstone" not in tomb:
        fail("commands: shim/tombstone marker")
    for cid in ("claude-code", "codex", "grok-cli"):
        c = cmd["clients"][cid]
        want = f"{ref['shim']} {cid} >/dev/null 2>&1 || true"
        if c["command"] != want:
            fail(f"commands: {cid} command")
        handlers = []
        if "file" in c:
            raw = (json.dumps(c["file"], indent=2, ensure_ascii=False) + "\n").encode()
            if hashlib.sha256(raw).hexdigest() != c["file_sha256_at_reference_paths"]:
                fail(f"commands: {cid} file sha256")
            for groups in c["file"]["hooks"].values():
                for g in groups:
                    handlers += g["hooks"]
            events = set(c["file"]["hooks"])
            mapped = set(V["normalization"]["event_names"][cid])
            if events - mapped:
                fail(f"commands: {cid} registers events the normalizer drops: {sorted(events - mapped)}")
        else:
            for g in c["groups"]:
                handlers += g["group"]["hooks"]
                h = g["group"]["hooks"][0]
                trust = {"event_name": g["snake_event"], "hooks": [{"async": h.get("async", False),
                         "command": h["command"], "timeout": h["timeout"], "type": h["type"]}]}
                canon = json.dumps(trust, sort_keys=True, separators=(",", ":")).encode()
                if "sha256:" + hashlib.sha256(canon).hexdigest() != g["trusted_hash"]:
                    fail(f"commands: codex {g['event']} trusted_hash")
                if g["event"] == "SessionEnd" and "async" in h:
                    fail("commands: codex SessionEnd must not carry async")
                if "matcher" in g["group"]:
                    fail("commands: codex groups carry no matcher")
            if [g["event"] for g in c["groups"]] != list(V["normalization"]["event_names"]["codex"]):
                fail("commands: codex groups differ from the normalizer's events")
        for h in handlers:
            if h["command"] != want or h["type"] != "command" or not isinstance(h["timeout"], int):
                fail(f"commands: {cid} handler {h}")
            if cid == "grok-cli" and ("async" in h or "args" in h):
                fail("commands: grok handlers carry no async/args")
    for cid in ("opencode", "hermes"):
        if cmd["clients"][cid]["argv"] != [ref["shim"], cid]:
            fail(f"commands: {cid} argv")
    nonce = "0123456789abcdef0123456789abcdef"
    plugin_schema = load("plugin-payload.schema.json")
    for cid, (start, done) in cmd["ready_check"]["payloads"].items():
        for payload, kind in ((start, "turn_start"), (done, "turn_done")):
            p = json.loads(json.dumps(payload).replace("@CODEPET_TEST_NONCE@", nonce))
            got = normalize(cid, p, V["kinds"], nonce)
            if got.get("kind") != kind or got.get("test") is not True:
                fail(f"commands: {cid} ready-check {kind} normalizes to {got}")
            if len(json.dumps(p).encode()) + 1 > 512:
                fail(f"commands: {cid} ready-check payload over 512 bytes")
            if cid in ("opencode", "hermes"):
                for e in validate(p, plugin_schema, plugin_schema) + plugin_rules(cid, p):
                    fail(f"commands: {cid} ready-check payload: {e}")
    for vec in V["normalization"]["vectors"]:
        if vec["client"] in ("opencode", "hermes"):
            errs = validate(vec["payload"], plugin_schema, plugin_schema) + plugin_rules(vec["client"], vec["payload"])
            if (not errs) != vec["plugin_valid"]:
                fail(f"plugin payload: {vec['client']} {vec['payload']} schema-valid={not errs}, plugin_valid={vec['plugin_valid']}")
    smoke_shim(shim_t)


def plugin_rules(client: str, p: dict) -> list[str]:
    """The plugin-payload.schema.json description rules a JSON Schema subset cannot say."""
    errs = []
    if ("session_id" in p) == ("approval_key" in p):
        errs.append("exactly one of session_id or approval_key")
    if "approval_key" in p and (client != "hermes" or p.get("event") not in ("waiting", "activity")):
        errs.append("approval_key only from hermes with event waiting or activity")
    return errs


def smoke_shim(template: str) -> None:
    """Runs the shim under sh in a throwaway dir: it prints nothing, always exits 0, and calls
    the binary only when it should."""
    sh = shutil.which("sh")
    timeout_bin = shutil.which("timeout")
    if not sh or not timeout_bin:
        print("note: sh or timeout missing; shim smoke skipped")
        return
    with tempfile.TemporaryDirectory(prefix="codepet-shim-") as tmp:
        t = Path(tmp)
        state, binp, log = t / "state", t / "codepet", t / "calls"
        (state / "inbox").mkdir(parents=True)
        (state / "clients").mkdir()
        binp.write_text(f"#!/bin/sh\necho \"$@\" >> '{log}'\ncat >/dev/null\necho noise\necho noise >&2\nexit 3\n")
        binp.chmod(0o755)
        shim = t / "codepet-hook"
        shim.write_text(template.replace("@CODEPET_STATE_DIR@", str(state)).replace("@CODEPET_BIN@", str(binp)))
        shim.chmod(0o755)

        def run(client, env=None):
            e = {"PATH": "/usr/bin:/bin"}
            e.update(env or {})
            r = subprocess.run([str(shim), client], input=b'{"x":1}\n', capture_output=True, env=e, timeout=5)
            if r.returncode != 0 or r.stdout or r.stderr:
                fail(f"shim smoke: {client} {env} -> rc {r.returncode} out {r.stdout!r} err {r.stderr!r}")
            calls = log.read_text().splitlines() if log.exists() else []
            if log.exists():
                log.unlink()
            return calls

        if run("codex") != [f"hook --state-dir {state} codex"]:
            fail("shim smoke: a normal event must call `codepet hook --state-dir S codex`")
        (state / "off").touch()
        if run("codex"):
            fail("shim smoke: the off flag must stop before the binary")
        (state / "off").unlink()
        (state / "clients" / "hermes.disconnected").touch()
        if run("hermes"):
            fail("shim smoke: a disconnected client must stop before the binary")
        if run("claude-code", {"GROK_HOOK_EVENT": "Stop"}):
            fail("shim smoke: a Claude-format hook inside Grok must stop in the shim")
        if run("claude-code", {"CLAUDE_CODE_ENTRYPOINT": "sdk-cli"}):
            fail("shim smoke: headless Claude without the helpers flag must stop in the shim")
        (state / "helpers").touch()
        if len(run("claude-code", {"CLAUDE_CODE_ENTRYPOINT": "sdk-cli"})) != 1:
            fail("shim smoke: headless Claude with the helpers flag must reach the binary")
        if len(run("grok-cli", {"GROK_HOOK_EVENT": "Stop"})) != 1:
            fail("shim smoke: GROK_HOOK_EVENT only stops the claude-code id")
        (state / "inbox").rmdir()
        if run("codex"):
            fail("shim smoke: no inbox dir must stop before the binary")
        (state / "inbox").mkdir()
        binp.unlink()
        run("codex")
        tomb = t / "tomb"
        tomb.write_bytes((HERE / "shim-tombstone.sh").read_bytes())
        tomb.chmod(0o755)
        r = subprocess.run([str(tomb), "codex"], input=b"x" * 100000, capture_output=True, timeout=5)
        if r.returncode != 0 or r.stdout or r.stderr:
            fail("shim smoke: the tombstone must exit 0 silently")

        def open_stdin(path, client):
            # The argv form (OpenCode, Hermes) with stdin never closed: the 0.3 s drain is
            # cut by SIGKILL, and the shell's job report must not reach the client.
            p = subprocess.Popen([str(path), client], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                 stderr=subprocess.PIPE, env={"PATH": "/usr/bin:/bin"})
            p.stdin.write(b'{"x":1}\n')
            p.stdin.flush()
            try:
                rc = p.wait(timeout=5)
                out, err = p.stdout.read(), p.stderr.read()
            finally:
                p.stdin.close()
                p.stdout.close()
                p.stderr.close()
            if rc != 0 or out or err:
                fail(f"shim smoke: {path.name} with stdin never closed -> rc {rc} out {out!r} err {err!r}")

        (state / "off").touch()
        open_stdin(shim, "opencode")
        open_stdin(tomb, "hermes")


def check_protocol(proto: dict, bar: dict) -> None:
    for m in proto["added"]:
        for ex in m["examples"]["valid"]:
            for e in validate(ex, m["params"], m["params"]):
                fail(f"protocol: {m['method']} valid example {ex}: {e}")
        for ex in m["examples"]["invalid"]:
            if not validate(ex, m["params"], m["params"]) and m["method"] not in ("pet.start", "pet.rename"):
                fail(f"protocol: {m['method']} invalid example {ex} passes its schema")
        if m["receipt"] is not True and "receipt_note" not in m:
            fail(f"protocol: {m['method']} has no receipt and no recorded reason")
        if len(set(m["errors"])) != len(m["errors"]):
            fail(f"protocol: {m['method']} lists an error twice")
    names = [m["method"] for m in proto["added"]]
    if len(set(names) | set(proto["kept"]) | set(proto["removed"])) != len(names) + len(proto["kept"]) + len(proto["removed"]):
        fail("protocol: method lists overlap")
    off = proto["off_behaviour"]
    live = set(proto["kept"]) | set(names)
    if set(off["allowed"]) & set(off["refused"]) or set(off["allowed"]) | set(off["refused"]) != live:
        fail("protocol: off_behaviour must split every kept and added method into allowed or refused")
    for m in proto["added"]:
        refused = m["method"] in off["refused"]
        if refused != ("INVALID_STATE" in m["errors"]):
            fail(f"protocol: {m['method']} INVALID_STATE in errors must match off_behaviour")
        if ("NOT_FOUND" in m["semantics"]) and "NOT_FOUND" not in m["errors"]:
            fail(f"protocol: {m['method']} semantics name NOT_FOUND but errors do not")
        if "BUSY" in m["semantics"] and "BUSY" not in m["errors"]:
            fail(f"protocol: {m['method']} semantics name BUSY but errors do not")
        if "TEST_STATE" in m["semantics"] and "TEST_STATE" not in m["errors"]:
            fail(f"protocol: {m['method']} semantics name TEST_STATE but errors do not")
    pat = proto["job_id_pattern"]
    if bar["$defs"]["client"]["properties"]["job"]["oneOf"][1]["properties"]["id"]["pattern"] != pat:
        fail("protocol: job_id_pattern differs from bar.v2 clients.<id>.job.id")
    rs = proto["result_schemas"]["clients.status"]
    names_by_id = {"claude-code": "Claude Code", "codex": "Codex", "opencode": "OpenCode", "hermes": "Hermes",
                   "grok-cli": "Grok"}
    status = sorted((HERE / "fixtures/clients-status").glob("*.json"))
    if not status:
        fail("protocol: no fixtures/clients-status")
    for p in status:
        doc = json.loads(p.read_text())
        for e in validate(doc, rs, rs):
            fail(f"clients-status {p.stem}: {e}")
        if [c["id"] for c in doc["clients"]] != ORDER or any(c["name"] != names_by_id[c["id"]] for c in doc["clients"]):
            fail(f"clients-status {p.stem}: order or names")
        for c in doc["clients"]:
            entry = {k: v for k, v in c.items() if k not in ("id", "name", "events_7d")}
            for e in validate(entry, bar["$defs"]["client"], bar, f"clients-status {p.stem} {c['id']}"):
                fail(e)
    jr = proto["result_schemas"]["clients.connect|clients.disconnect|clients.test"]
    if validate({"job_id": "job-7f3a0c19e2d84b6a5c1e9f02"}, jr, jr) or not validate({"job_id": "job-1"}, jr, jr):
        fail("protocol: job result schema")
    snap = proto["snapshot_v03"]
    for p in sorted((HERE / "fixtures/bar-v2").glob("*.json")):
        d = json.loads(p.read_text())
        life = d["life"]
        frozen = ("no_life" if life is None else "off" if not d["enabled"] else
                  "unarmed" if not life["armed"] else "paused" if life["paused"] else
                  "sleeping" if d["care"]["sleeping"] else "none")
        example = {"schema_version": 5, "protocol_version": 1, "enabled": d["enabled"], "count_helpers": False,
                   "agent": d["agent"], "treats": d["treats"], "clients": d["clients"],
                   "lifecycle": {"frozen_reason": frozen}}
        for e in validate(example, snap["schema"], snap["schema"]):
            fail(f"snapshot_v03 from {p.stem}: {e}")
    if snap["frozen_reason_precedence"]["order"][:2] != ["no_life", "off"]:
        fail("protocol: off must be the first frozen reason of a living life")


def main() -> int:
    files = ["strings.json", "icons.json", "pixel-budgets.json", "protocol.json", "agent_vectors.json",
             "bar.v2.schema.json", "commands.json", "plugin-payload.schema.json"]
    docs = {}
    for f in files:
        try:
            docs[f] = load(f)
        except (OSError, json.JSONDecodeError) as e:
            fail(f"{f}: {e}")
    if FAILS:
        print("\n".join(FAILS))
        return 1
    check_strings(docs["strings.json"])
    check_budgets(docs["pixel-budgets.json"])
    check_icons(docs["icons.json"], docs["agent_vectors.json"])
    check_ddl()
    check_protocol(docs["protocol.json"], docs["bar.v2.schema.json"])
    check_vectors(docs["agent_vectors.json"], docs["strings.json"], docs["bar.v2.schema.json"])
    check_commands(docs["agent_vectors.json"])
    try:  # Cross-check the subset validator with the real one when it happens to be installed.
        import jsonschema  # type: ignore
        schema = docs["bar.v2.schema.json"]
        for p in sorted((HERE / "fixtures/bar-v2").glob("*.json")):
            jsonschema.validate(json.loads(p.read_text()), schema)
        for m in docs["protocol.json"]["added"]:
            for ex in m["examples"]["valid"]:
                jsonschema.validate(ex, m["params"])
        rs = docs["protocol.json"]["result_schemas"]["clients.status"]
        for p in sorted((HERE / "fixtures/clients-status").glob("*.json")):
            jsonschema.validate(json.loads(p.read_text()), rs)
    except ImportError:
        pass
    except Exception as e:  # noqa: BLE001 - report any jsonschema failure as a check failure
        fail(f"jsonschema: {e}")
    if FAILS:
        print("\n".join(FAILS))
        print(f"FAIL: {len(FAILS)} contract problem(s)")
        return 1
    V = docs["agent_vectors.json"]
    n = sum(len(V[k]["vectors"]) for k in ("normalization", "event_ids", "hmac_keys", "interactivity",
                                             "turn_qualifies", "caps", "sessions", "fold", "aggregate", "priority",
                                             "popup_routing", "connect_card"))
    n += len(V["helpers_rows"]["fixture_vectors"]) + len(V["helpers_rows"]["bucket_vectors"])
    print(f"OK: {len(docs['strings.json'])} strings, {n} agent vectors, "
          f"{len(list((HERE / 'fixtures/bar-v2').glob('*.json')))} bar fixtures, "
          f"{len(list((HERE / 'fixtures/clients-status').glob('*.json')))} clients.status fixtures, "
          "schema v5 DDL, commands + shim smoke")
    return 0


if __name__ == "__main__":
    sys.exit(main())
