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
  * protocol.json: valid param examples match their schemas;
  * agent_vectors.json: replayed through an independent reference model (normalization,
    event ids, interactivity, turn_qualifies, caps, sessions, fold outcomes, shared
    priority over the fixtures), and every line key / placeholder exists in strings.json.

Run: python3 docs/v0.3/contracts/check_contracts.py   (exit 1 on any failure)
The reference model is a test oracle for the contract, not product code.
"""
from __future__ import annotations

import hashlib
import json
import re
import sqlite3
import sys
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
        assert ref.startswith("#/$defs/"), ref
        return validate(v, root["$defs"][ref[len("#/$defs/"):]], root, path)
    if "oneOf" in s:
        ok = [x for x in s["oneOf"] if not validate(v, x, root, path)]
        if len(ok) != 1:
            errs.append(f"{path}: matches {len(ok)} oneOf branches")
        return errs
    if "type" in s and not _type_ok(v, s["type"]):
        return [f"{path}: expected {s['type']}, got {type(v).__name__}"]
    if "const" in s and v != s["const"]:
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
                "act.ready", "act.hello", "act.oops", "act.waiting", "codex.step5", "reason.blocked"]
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
    for need in ("need.food", "need.water", "need.play", "need.wash"):
        if icons["bar_badges"][need] not in next(s for s in icons["sheets"] if s["id"] == "bar")["ids"]:
            fail(f"icons: bar badge for {need} missing")


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


# ------------------------------------------------------------------ reference model
ID_RE = re.compile(r"^[A-Za-z0-9._:-]{1,128}$")
ERRORS = {"rate_limit", "authentication_failed", "invalid_request", "server_error", "max_output_tokens",
          "overloaded", "billing_error", "unknown"}


def normalize(client: str, p: dict, kinds: list[str]) -> dict:
    if "agent_id" in p or "subagentType" in p:
        return {"drop": "child"}

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
            if p.get("codepet_v") != 1 or kind not in kinds:
                return {"drop": "unmapped"}
            out = {"kind": kind, "session": ident("session_id"), "turn": ident("turn_id"), "tool": None,
                   "class": None, "akey": ident("approval_key")}
            if out["session"] is None and out["akey"] is None:
                return {"drop": "bad_id"}
            if isinstance(p.get("tools"), int):
                out["tools"] = p["tools"]
            if isinstance(p.get("interactive"), bool):
                out["interactive"] = p["interactive"]
            return out
        name = p.get("hook_event_name") or p.get("hookEventName")
        session = ident("session_id", "sessionId")
        turn = ident("prompt_id", "promptId", "turn_id")
        tool = ident("tool_use_id", "toolUseId")
    except ValueError:
        return {"drop": "bad_id"}
    if session is None:
        return {"drop": "bad_id"}
    note = p.get("notification_type", p.get("notificationType"))
    tool_name = p.get("tool_name", p.get("toolName"))
    base = {"session": session, "turn": turn, "tool": None, "class": None}

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


def interactive(v: dict, rules: dict) -> bool:
    c = v["client"]
    argv = v.get("ancestor_argv")
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
            if o and key == o["key"]:
                if tool:
                    o["tools"].add(tool)
                if s["state"] in ("waiting", "idle"):
                    s["state"] = "working"
            elif d and key == d["key"] and at <= d["at"] + late:
                if tool and tool not in d["tools"]:
                    d["tools"].add(tool)
                if not d["earned"] and not d["rechecked"]:
                    d["rechecked"] = True
                    n = len(d["tools"]) + d["extra"]
                    if self.treat_check(sid, d["key"], n, d["at"] - d["start"], inter, False, d["at"]):
                        d["earned"] = earned = True
            elif d and key == d["key"] and at > d["at"] + late and tool and tool not in d["tools"]:
                s["open"] = {"key": key, "start": d["start"], "tools": set(d["tools"]) | {tool}, "extra": d["extra"]}
                s["state"] = "working"
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
                        self.finished_seq += 1
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
        before = 0
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
            before += got
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
        test_state = None
        if not vec["enabled"]:
            outcome = "dropped_off"
        elif at < now - C["max_event_age_ms"]:
            outcome = "dropped_old"
        else:
            if at > now + C["future_clamp_ms"]:
                at = now
            if e.get("test"):
                outcome = "test"
                test_state = "running" if e["kind"] == "turn_start" else "done"
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


def pet_state(doc: dict, now: int, seen: dict, P: dict, C: dict) -> dict:
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
    ag = doc["agent"]
    counting = [a for a in ag["apps"] if a["stale_at_ms"] > now]
    working = [a["id"] for a in counting if a["state"] == "working"]
    waiting = sorted((a for a in counting if a["state"] == "waiting"),
                     key=lambda a: (-a["since_ms"], ORDER.index(a["id"])))
    jar = doc["treats"]["jar"]
    sleeping = doc["care"]["sleeping"]
    tv = jar > 0 and not life["fainted"] and not sleeping
    res["treats_visible"] = tv
    wakes = [a["stale_at_ms"] for a in counting]
    W, G = C["celebrate_window_ms"], C["celebrate_grace_ms"]
    cands = []
    t = ag["test"]
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
    elif cands:
        _, line, vars_ = cands[0]
        res.update(state="celebrate", bar="star", line=line, vars=vars_, overlay="celebrate",
                   bubble={"star": True})
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


def check_priority(V: dict, strings: dict, schema: dict) -> None:
    P, C = V["priority"], V["constants"]
    fixtures = {p.stem: json.loads(p.read_text()) for p in (HERE / "fixtures/bar-v2").glob("*.json")}
    used = set()
    for vec in P["vectors"]:
        doc = fixtures.get(vec["fixture"])
        if doc is None:
            fail(f"priority: {vec['name']} names missing fixture {vec['fixture']}")
            continue
        used.add(vec["fixture"])
        got = pet_state(doc, vec["now"], vec["seen"], P, C)
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
    unused = set(fixtures) - used - {"clients-mixed"}
    if unused:
        fail(f"priority: fixtures without a vector: {sorted(unused)}")


def check_vectors(V: dict, strings: dict, schema: dict) -> None:
    C = V["constants"]
    kinds = V["kinds"]
    for vec in V["normalization"]["vectors"]:
        got = normalize(vec["client"], vec["payload"], kinds)
        for k, v in vec["expect"].items():
            if got.get(k) != v:
                fail(f"normalization: {vec['client']} {json.dumps(vec['payload'])[:60]} {k} = {got.get(k)!r}, expected {v!r}")
    for vec in V["event_ids"]["vectors"]:
        body = vec["payload_utf8"].encode()
        dig = hashlib.sha256(body).hexdigest()[:16]
        key = f"codepet-ev1|{vec['client']}|{vec['kind']}|{vec['session']}|{dig}|{vec['at_ms'] // C['event_bucket_ms']}"
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
    check_priority(V, strings, schema)
    if [c["id"] for c in V["clients"]] != ORDER:
        fail("vectors: client order")


def check_protocol(proto: dict) -> None:
    for m in proto["added"]:
        for ex in m["examples"]["valid"]:
            for e in validate(ex, m["params"], m["params"]):
                fail(f"protocol: {m['method']} valid example {ex}: {e}")
    names = [m["method"] for m in proto["added"]]
    if len(set(names) | set(proto["kept"]) | set(proto["removed"])) != len(names) + len(proto["kept"]) + len(proto["removed"]):
        fail("protocol: method lists overlap")


def main() -> int:
    files = ["strings.json", "icons.json", "pixel-budgets.json", "protocol.json", "agent_vectors.json",
             "bar.v2.schema.json"]
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
    check_protocol(docs["protocol.json"])
    check_vectors(docs["agent_vectors.json"], docs["strings.json"], docs["bar.v2.schema.json"])
    try:  # Cross-check the subset validator with the real one when it happens to be installed.
        import jsonschema  # type: ignore
        schema = docs["bar.v2.schema.json"]
        for p in sorted((HERE / "fixtures/bar-v2").glob("*.json")):
            jsonschema.validate(json.loads(p.read_text()), schema)
        for m in docs["protocol.json"]["added"]:
            for ex in m["examples"]["valid"]:
                jsonschema.validate(ex, m["params"])
    except ImportError:
        pass
    except Exception as e:  # noqa: BLE001 - report any jsonschema failure as a check failure
        fail(f"jsonschema: {e}")
    if FAILS:
        print("\n".join(FAILS))
        print(f"FAIL: {len(FAILS)} contract problem(s)")
        return 1
    V = docs["agent_vectors.json"]
    n = sum(len(V[k]["vectors"]) for k in ("normalization", "event_ids", "interactivity", "turn_qualifies",
                                             "caps", "sessions", "fold", "priority"))
    print(f"OK: {len(docs['strings.json'])} strings, {n} agent vectors, "
          f"{len(list((HERE / 'fixtures/bar-v2').glob('*.json')))} bar fixtures, schema v5 DDL")
    return 0


if __name__ == "__main__":
    sys.exit(main())
