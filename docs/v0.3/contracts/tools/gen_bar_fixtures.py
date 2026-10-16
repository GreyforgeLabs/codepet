#!/usr/bin/env python3
"""Writes docs/v0.3/contracts/fixtures/bar-v2/*.json, the shared-priority, popup-routing and
Helpers-row state fixtures (SPEC-v0.3 §9.3), and fixtures/clients-status/*.json, the
clients.status results for the WP5 smokes. Every fixture is generated at NOW; agent_vectors.json "priority"
vectors evaluate them at NOW plus an offset. Run from anywhere:
    python3 docs/v0.3/contracts/tools/gen_bar_fixtures.py
"""
import copy
import json
from pathlib import Path

OUT = Path(__file__).resolve().parents[1] / "fixtures" / "bar-v2"
T = 1792123900000
LIFE_ID = "life-30b6f1816d7b922ab5befc25"
STALE_WORK = 600000
STALE_WAIT = 1800000

NEEDS = [("hunger", "food"), ("thirst", "water"), ("mental_health", "play"), ("hygiene", "wash")]
OK_WINDOWS = {"hunger": (28800000, 46800000, 68400000), "thirst": (23760000, 39600000, 61200000),
              "mental_health": (45600000, None, None), "hygiene": (54000000, None, None)}


def needs(**states):
    out = []
    for need, action in NEEDS:
        st = states.get(action, "ok")
        w, c, e = OK_WINDOWS[need] if st == "ok" else (None, None, None)
        out.append({"need": need, "action": action, "state": st, "until_warning_ms": w,
                    "until_critical_ms": c, "until_end_ms": e})
    return out


def base(name="Ginger", appearance="cat", mode="gentle", armed=True, xp=24):
    meters = {"hunger": 10000, "thirst": 10000, "mental_health": 10000, "hygiene": 10000}
    return {
        "schema": "codepet.bar.v2", "state_version": 812, "db_id": "db-2c852a39b6a7be9815dbd45e",
        "generated_at_ms": T, "app_version": "0.3.0", "ruleset": "lifecycle-v1",
        "life": {"life_id": LIFE_ID, "ordinal": 1, "name": name, "appearance": appearance, "stage": 1,
                 "level": 1, "xp": xp, "next_stage_xp": 80, "status": "healthy", "mode": mode, "armed": armed,
                 "pace": "steady", "paused": False, "fainted": False, "death_pending": False},
        "meters": meters, "needs": needs(), "critical": None,
        "care": {"sleeping": False, "sleep_remaining_ms": 120000, "sleep_started_at_ms": None, "sleep_due": False,
                 "bathroom_due": False, "ready_to_work": False, "readiness_reasons": []},
        "task": None,
        "presence": {"wanted": armed, "life_id": LIFE_ID, "tick_ms": 60000, "idle_timeout_s": 180},
        "unseen_events": [], "memorial": {"count": 0, "latest_name": None},
        "enabled": True,
        "agent": {"state": "idle", "apps": [], "stale_at_ms": None, "finished_seq": 0, "finished_at_ms": None,
                  "finished_app": None, "finished_treat": False, "finished_jar_full": False, "hello_seq": 0,
                  "hello_at_ms": None, "hello_app": None, "test": None, "error_at_ms": None, "error_app": None,
                  "helpers_busy": False},
        "treats": {"jar": 0, "full": False},
        "clients": {},
    }


def app(cid, state, since):
    stale = since + (STALE_WAIT if state == "waiting" else 120000 if state == "error" else STALE_WORK)
    return {"id": cid, "state": state, "since_ms": since, "stale_at_ms": stale}


def with_apps(d, *apps):
    order = ["claude-code", "codex", "opencode", "hermes", "grok-cli"]
    d["agent"]["apps"] = sorted(apps, key=lambda a: (a["since_ms"], order.index(a["id"])))
    rank = {"idle": 0, "error": 1, "working": 2, "waiting": 3}
    d["agent"]["state"] = max((a["state"] for a in apps), key=rank.get, default="idle")
    d["agent"]["stale_at_ms"] = min((a["stale_at_ms"] for a in apps), default=None)
    return d


def no_pet(d):
    d["life"] = None
    d["presence"]["wanted"] = False
    d["presence"]["life_id"] = None
    return d


def meters_for(d):
    level = {"ok": 10000, "low": 2500, "empty": 0, "critical": 0}
    for n in d["needs"]:
        d["meters"][n["need"]] = level[n["state"]]
    return d


VERSIONS = {"claude-code": "2.1.294", "codex": "0.163.0", "opencode": "1.19.0", "hermes": "0.21.3",
            "grok-cli": "1.0.52"}
JOB = "job-7f3a0c19e2d84b6a5c1e9f02"  # job ids: "job-" + 24 lowercase hex (protocol.json job_id_pattern)


def client(cid, connected=False, needs_step=None, last=None, first=None, failed=None, job=None, installed=True):
    return {"installed": installed, "version": VERSIONS[cid] if installed else None, "connected": connected,
            "needs_step": needs_step, "last_seen_ms": last, "first_seen_ms": first, "failed_reason": failed,
            "job": None if job is None else {"id": JOB, "kind": job, "state": "running"}}


F = {}
F["fresh-no-pet"] = no_pet(base())
d = no_pet(base()); d["memorial"] = {"count": 1, "latest_name": "Ginger"}; F["memorial"] = d
d = base(); d["enabled"] = False; d["presence"]["wanted"] = False; F["off"] = d
d = base(); d["enabled"] = False; d["presence"]["wanted"] = False; d["treats"] = {"jar": 4, "full": False}
F["off-treats"] = d
d = no_pet(base()); d["enabled"] = False; F["off-no-pet"] = d
F["idle-happy"] = base()
d = base(); d["treats"] = {"jar": 3, "full": False}; F["idle-treats"] = d
d = base(xp=80); d["life"].update({"stage": 2, "level": 2, "next_stage_xp": 400})
d["unseen_events"] = [{"seq": 7, "stage": 2}]; d["treats"] = {"jar": 1, "full": False}; F["grew-up"] = d
d = base(); d["life"]["paused"] = True; d["presence"]["wanted"] = False; d["treats"] = {"jar": 2, "full": False}
F["paused"] = d
d = base(); d["needs"] = needs(food="low"); F["need-food-soon"] = meters_for(d)
d = base(); d["needs"] = needs(water="empty"); F["need-water-now"] = meters_for(d)
d = base(); d["needs"] = needs(food="critical"); d["life"]["status"] = "critical"
d["critical"] = {"need": "hunger", "action": "food", "grace_ms": 43200000, "elapsed_ms": 600000,
                 "remaining_ms": 42600000, "since_ms": T - 600000}
F["need-food-critical"] = meters_for(d)
d = base(); d["needs"] = needs(food="low", water="empty", play="empty"); F["several-needs"] = meters_for(d)
d = base(); d["care"].update({"sleeping": True, "sleep_remaining_ms": 60000, "sleep_started_at_ms": T - 60000})
d["treats"] = {"jar": 2, "full": False}; F["asleep"] = d
d = base(); d["life"]["fainted"] = True; d["life"]["status"] = "critical"; d["needs"] = needs(food="empty", water="empty")
d["treats"] = {"jar": 2, "full": False}
F["fainted"] = with_apps(meters_for(d), app("codex", "waiting", T - 10000))
F["working-one"] = with_apps(base(), app("claude-code", "working", T - 60000))
F["working-two"] = with_apps(base(), app("claude-code", "working", T - 60000), app("grok-cli", "working", T - 30000))
F["working-many"] = with_apps(base(), app("claude-code", "working", T - 60000), app("opencode", "working", T - 50000),
                              app("grok-cli", "working", T - 40000))
d = base(); d["needs"] = needs(water="empty")
F["waiting"] = with_apps(meters_for(d), app("claude-code", "working", T - 60000), app("codex", "waiting", T - 10000))
d = base(); d["agent"].update({"finished_seq": 5, "finished_at_ms": T - 10000, "finished_app": "claude-code",
                               "finished_treat": True}); d["treats"] = {"jar": 1, "full": False}
F["finished-while-working"] = with_apps(d, app("claude-code", "working", T - 5000))
F["finished-while-working"]["treats"] = {"jar": 0, "full": False}
F["finished-while-working"]["agent"]["finished_treat"] = False
d = base(); d["agent"].update({"finished_seq": 5, "finished_at_ms": T - 25000, "finished_app": "claude-code",
                               "finished_treat": True}); d["treats"] = {"jar": 3, "full": False}
F["finished-treat"] = d
d = base(); d["agent"].update({"finished_seq": 2, "finished_at_ms": T - 10000, "finished_app": "codex",
                               "finished_jar_full": True}); d["treats"] = {"jar": 10, "full": True}
F["jar-full"] = d
d = base(); d["agent"].update({"hello_seq": 1, "hello_at_ms": T - 5000, "hello_app": "claude-code"})
d["clients"] = {"claude-code": client("claude-code", True, last=T - 5000, first=T - 5000)}
F["hello"] = d
d = base(); d["agent"]["test"] = {"state": "running", "at_ms": T - 500, "client": "claude-code"}
d["clients"] = {"claude-code": client("claude-code", True, job="connect")}
F["test-running"] = d
d = base(); d["agent"]["test"] = {"state": "done", "at_ms": T - 1500, "client": "claude-code"}
d["clients"] = {"claude-code": client("claude-code", True)}
F["ready-check-done"] = d
d = base(); d["agent"]["test"] = {"state": "failed", "at_ms": T - 1000, "client": "claude-code"}
d["clients"] = {"claude-code": client("claude-code", True, failed="ready_check")}
F["test-failed"] = d
d = base(); d["agent"].update({"error_at_ms": T - 1000, "error_app": "grok-cli"})
F["oops"] = with_apps(d, app("grok-cli", "error", T - 1000))
d = base(); d["agent"]["state"] = "working"
d["agent"]["apps"] = [{"id": "claude-code", "state": "working", "since_ms": T - 700000, "stale_at_ms": T - 100000}]
d["agent"]["stale_at_ms"] = T - 100000
F["stale-working"] = d
d = base(); d["agent"]["helpers_busy"] = True
F["helpers-busy"] = d
# The owner after the upgrade: Ginger the goldfish, never armed, stored as permadeath.
d = base(name="Ginger", appearance="goldfish", mode="permadeath", armed=False, xp=0)
F["owner-unarmed-welcome"] = d
# The owner after Let's go!: armed in gentle mode, aquatic copy.
d = base(name="Ginger", appearance="goldfish", mode="gentle", armed=True, xp=0)
d["needs"] = needs(wash="low"); F["goldfish-ginger-owner"] = meters_for(d)
d = base(name="Bolt", appearance="robot"); d["life"]["life_id"] = "life-0123456789abcdef01234567"
d["presence"]["life_id"] = d["life"]["life_id"]; d["needs"] = needs(food="empty"); F["robot-bolt"] = meters_for(d)
d = base()
d["clients"] = {
    "claude-code": client("claude-code", True, last=T - 30000, first=T - 86400000),
    "grok-cli": client("grok-cli", True),
    "opencode": client("opencode", job="connect"),
    "hermes": client("hermes", failed="too_old"),
    "codex": client("codex", True, needs_step="codex_trust"),
}
F["clients-mixed"] = d
d = base()
d["clients"] = {
    "codex": client("codex", needs_step="codex_hooks_off"),
    "claude-code": client("claude-code", True, last=T - 3 * 3600000, first=T - 86400000),
    "grok-cli": client("grok-cli", installed=False),
}
F["clients-codex-hooks-off"] = d
d = base()
d["clients"] = {
    "codex": client("codex"),
    "opencode": client("opencode", True, last=T - 2 * 86400000, first=T - 3 * 86400000),
    "hermes": client("hermes", installed=False),
}
F["clients-codex-new"] = d
d = base()
d["clients"] = {
    "hermes": client("hermes", True, last=T - 60000, first=T - 86400000, job="disconnect"),
    "grok-cli": client("grok-cli", True, job="test"),
}
F["clients-disconnect-running"] = d
d = base()
d["clients"] = {
    "claude-code": client("claude-code", True, last=T - 3600000, first=T - 86400000, failed="blocked"),
    "opencode": client("opencode", installed=False, connected=True, last=T - 86400000, first=T - 2 * 86400000),
}
F["clients-failed-while-connected"] = d

# clients.status results (protocol.json "result_schemas"."clients.status"), for the WP5 smokes.
NAMES = {"claude-code": "Claude Code", "codex": "Codex", "opencode": "OpenCode", "hermes": "Hermes", "grok-cli": "Grok"}
ORDER = ["claude-code", "codex", "opencode", "hermes", "grok-cli"]


def status(records, events=None):
    out = []
    for cid in ORDER:
        rec = records.get(cid) or client(cid, installed=False)
        out.append({"id": cid, "name": NAMES[cid], **{k: rec[k] for k in ("installed", "version", "connected",
                    "needs_step", "last_seen_ms", "first_seen_ms")}, "events_7d": (events or {}).get(cid, 0),
                    "failed_reason": rec["failed_reason"], "job": rec["job"]})
    return {"clients": out}


S = {}
S["nothing-installed"] = status({})
S["owner-first-run"] = status({c: client(c) for c in ORDER})
S["after-connect"] = status({
    "claude-code": client("claude-code", True, last=T - 30000, first=T - 600000),
    "codex": client("codex", True, needs_step="codex_trust"),
    "opencode": client("opencode", job="connect"),
    "hermes": client("hermes", failed="too_old"),
    "grok-cli": client("grok-cli", True),
}, events={"claude-code": 42})
S["all-connected"] = status({c: client(c, True, last=T - 60000, first=T - 86400000) for c in ORDER},
                            events={c: 3 for c in ORDER})

SOUT = OUT.parent / "clients-status"
SOUT.mkdir(parents=True, exist_ok=True)
for old in SOUT.glob("*.json"):
    if old.stem not in S:
        old.unlink()
for name, doc in S.items():
    (SOUT / f"{name}.json").write_text(json.dumps(doc, indent=1) + "\n")

OUT.mkdir(parents=True, exist_ok=True)
for old in OUT.glob("*.json"):
    if old.stem not in F:
        old.unlink()
for name, doc in F.items():
    text = json.dumps(doc, indent=1) + "\n"
    assert len(text.encode()) <= 16384, name
    (OUT / f"{name}.json").write_text(text)
print(f"wrote {len(F)} bar fixtures to {OUT} and {len(S)} clients.status fixtures to {SOUT}")
