#!/usr/bin/env python3
"""Generate petstate.js, the shared-priority face of CodePet (SPEC-v0.3 §6.4, §8).

Dev-time tool, standard library only. One order drives the bar badge, the Home face and
line, and the desktop overlay. The tables (client order and names, need levels and
priority, the timing constants) come from `docs/v0.3/contracts/agent_vectors.json`; the
function body below mirrors the reference `shared_priority` in
`docs/v0.3/contracts/check_contracts.py` and is replayed against every `priority` vector
by `qml/tests/test_petstate.mjs`.

    python3 tools/petstate/gen_petstate.py           # write both copies
    python3 tools/petstate/gen_petstate.py --check   # exit 1 when a copy is missing or stale

The same bytes go to `qml/shared/petstate.js` (overlay) and `omarchy-plugin/petstate.js`
(bar plugin, which cannot import runtime QML). The output embeds the sha256 of the vectors
file, so changing the contract without regenerating fails `--check`.
"""

import argparse
import hashlib
import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
VECTORS = REPO / "docs/v0.3/contracts/agent_vectors.json"
OUTPUTS = (REPO / "qml/shared/petstate.js", REPO / "omarchy-plugin/petstate.js")
# Constants the face needs; every one must exist in agent_vectors.json `constants`.
CONSTANTS = ("celebrate_window_ms", "celebrate_grace_ms", "error_recent_ms", "error_frame_ms",
             "test_done_delay_ms", "test_arrival_ms", "jar_max")

BODY = r"""
function isObject(v) {
    return v !== null && typeof v === "object" && !Array.isArray(v)
}

function num(v) {
    return typeof v === "number" && isFinite(v) ? v : null
}

function clientRank(id) {
    var i = CLIENTS.indexOf(String(id))
    return i < 0 ? CLIENTS.length : i
}

// The kid-facing name of a client id ("claude-code" -> "Claude Code"); "" when unknown.
function appName(id) {
    return Object.prototype.hasOwnProperty.call(CLIENT_NAMES, String(id)) ? CLIENT_NAMES[String(id)] : ""
}

// "happy" | "soon" | "now" | "critical" for an export need state; "" when unknown.
function needLevel(state) {
    return Object.prototype.hasOwnProperty.call(NEED_LEVELS, String(state)) ? NEED_LEVELS[String(state)] : ""
}

// The empty face (no file read yet).
function emptyFace() {
    return { state: null, bar: null, line: null, vars: {}, overlay: "none", pulse: false,
             treats_visible: false, bubble: null, wake_at_ms: null, celebrate: null }
}

// sharedPriority(doc, now, seen) -> face. `doc` is a parsed bar-state.json (v1 or v2);
// `now` is ms since the epoch; `seen` is {finished_seq, hello_seq, test_at_ms}, what this
// UI already celebrated. Pure: no I/O, no clock, no timers. The face says which row wins
// (`state`), what the bar cell draws (`bar`, `pulse`), the strings.json key of the line and
// its placeholders (`line`, `vars`; {app}/{app2} are client ids), the overlay animation
// (`overlay`), the picture bubble, whether Treats may show, and `wake_at_ms`: the next time
// the face can change without a new export (one single-shot timer; null = none, as while
// Off). `celebrate` names the winning celebration candidate ("grew" | "test" | "hello" |
// "finished") with its `seq`/`at_ms`, so a UI can record it in `seen` once shown.
function sharedPriority(doc, now, seen) {
    var res = emptyFace()
    if (!isObject(doc))
        return res
    seen = isObject(seen) ? seen : {}
    var seenFinished = num(seen.finished_seq) === null ? 0 : seen.finished_seq
    var seenHello = num(seen.hello_seq) === null ? 0 : seen.hello_seq
    var seenTest = num(seen.test_at_ms) === null ? 0 : seen.test_at_ms
    var life = isObject(doc.life) ? doc.life : null
    // A v1 export has no `enabled`: it is on.
    if (doc.enabled === false) {
        res.state = "off"
        res.bar = "off"
        res.line = life ? "off.line" : null
        res.vars = life ? { name: String(life.name) } : {}
        return res
    }
    if (life === null) {
        res.state = "no_pet"
        res.bar = "egg"
        return res
    }
    var name = String(life.name)
    if (life.armed === false) {
        res.state = "welcome"
        res.bar = "hi"
        res.line = "picker.welcome"
        res.vars = { name: name }
        res.overlay = "wander"
        return res
    }
    var ag = isObject(doc.agent) ? doc.agent : {}
    var apps = Array.isArray(ag.apps) ? ag.apps : []
    var counting = []
    for (var i = 0; i < apps.length; i++) {
        var a = apps[i]
        if (isObject(a) && num(a.stale_at_ms) !== null && a.stale_at_ms > now && num(a.since_ms) !== null)
            counting.push(a)
    }
    var working = []
    var waiting = []
    for (var j = 0; j < counting.length; j++) {
        if (counting[j].state === "working") working.push(String(counting[j].id))
        else if (counting[j].state === "waiting") waiting.push(counting[j])
    }
    waiting.sort(function (x, y) {
        if (x.since_ms !== y.since_ms) return y.since_ms - x.since_ms
        return clientRank(x.id) - clientRank(y.id)
    })
    var treats = isObject(doc.treats) ? doc.treats : {}
    var jar = num(treats.jar) === null ? 0 : treats.jar
    var care = isObject(doc.care) ? doc.care : {}
    var sleeping = care.sleeping === true
    var fainted = life.fainted === true
    var paused = life.paused === true
    var tv = jar > 0 && !fainted && !paused && !sleeping
    res.treats_visible = tv
    var wakes = []
    for (var k = 0; k < counting.length; k++) wakes.push(counting[k].stale_at_ms)
    var W = C.celebrate_window_ms, G = C.celebrate_grace_ms
    var t = isObject(ag.test) ? ag.test : null
    var testing = false
    if (t && t.state === "running" && num(t.at_ms) !== null) {
        var end = t.at_ms + C.test_done_delay_ms + C.test_arrival_ms
        if (now < end) {
            testing = true
            wakes.push(end)
        }
    }
    var cands = []
    var unseen = Array.isArray(doc.unseen_events) ? doc.unseen_events : []
    if (unseen.length > 0) {
        var top = 0
        for (var u = 0; u < unseen.length; u++)
            if (isObject(unseen[u]) && num(unseen[u].seq) !== null && unseen[u].seq > top) top = unseen[u].seq
        cands.push({ kind: "grew", line: "grew", vars: { name: name }, seq: top, at_ms: null })
    }
    if (t && t.state === "done" && num(t.at_ms) !== null && t.at_ms > seenTest && now < t.at_ms + W) {
        wakes.push(t.at_ms + W)
        cands.push({ kind: "test", line: "act.ready", vars: { name: name }, seq: null, at_ms: t.at_ms })
    }
    var helloSeq = num(ag.hello_seq) === null ? 0 : ag.hello_seq
    var helloAt = num(ag.hello_at_ms)
    if (helloSeq > seenHello && helloAt !== null && now < helloAt + W) {
        wakes.push(helloAt + W)
        cands.push({ kind: "hello", line: "act.hello", vars: { name: name, app: ag.hello_app === undefined ? null : ag.hello_app },
                     seq: helloSeq, at_ms: helloAt })
    }
    var finishedSeq = num(ag.finished_seq) === null ? 0 : ag.finished_seq
    var fa = num(ag.finished_at_ms)
    if (finishedSeq > seenFinished && fa !== null && now < fa + W) {
        if (now < fa + G) {
            wakes.push(fa + G)
        } else {
            wakes.push(fa + W)
            if (working.indexOf(String(ag.finished_app)) < 0) {
                var fin = { kind: "finished", seq: finishedSeq, at_ms: fa }
                if (ag.finished_jar_full === true) { fin.line = "act.done_full"; fin.vars = {} }
                else if (ag.finished_treat === true) { fin.line = "act.done_treat"; fin.vars = { name: name } }
                else { fin.line = "act.done"; fin.vars = {} }
                cands.push(fin)
            }
        }
    }
    var ea = num(ag.error_at_ms)
    var oops = ea !== null && now < ea + C.error_recent_ms
    if (ea !== null) {
        var ends = [ea + C.error_frame_ms, ea + C.error_recent_ms]
        for (var e = 0; e < ends.length; e++)
            if (ends[e] > now) wakes.push(ends[e])
    }
    res.wake_at_ms = wakes.length > 0 ? Math.min.apply(null, wakes) : null
    var nowNeeds = [], soonNeeds = []
    var needs = Array.isArray(doc.needs) ? doc.needs : []
    for (var n = 0; n < needs.length; n++) {
        if (!isObject(needs[n]) || NEED_PRIORITY.indexOf(String(needs[n].action)) < 0) continue
        var lv = needLevel(needs[n].state)
        if (lv === "now" || lv === "critical") nowNeeds.push({ need: String(needs[n].action), level: lv })
        else if (lv === "soon") soonNeeds.push({ need: String(needs[n].action), level: lv })
    }
    var byPriority = function (x, y) { return NEED_PRIORITY.indexOf(x.need) - NEED_PRIORITY.indexOf(y.need) }
    nowNeeds.sort(byPriority)
    soonNeeds.sort(byPriority)
    var star = tv ? "star" : "none"
    if (fainted) {
        setFace(res, "fainted", "sleep_bandage", "mood.fainted", { name: name }, "sleep", { bandage: true })
    } else if (paused) {
        setFace(res, "paused", "sleep", "mood.asleep", { name: name }, "sleep", null)
    } else if (sleeping) {
        setFace(res, "asleep", "sleep", "mood.asleep", { name: name }, "sleep", null)
    } else if (waiting.length > 0) {
        var wid = String(waiting[0].id)
        setFace(res, "waiting", "hand", "act.waiting", { app: wid }, "waiting", { apps: [wid] })
        res.pulse = true
    } else if (nowNeeds.length > 0) {
        var nn = nowNeeds[0]
        setFace(res, "need_now", "need_" + nn.need, "mood." + nn.need + "." + nn.level, { name: name },
            "needs_care", { need: nn.need })
    } else if (working.length > 0) {
        var line = "act.working.many", vars = { name: name }
        if (working.length === 1) { line = "act.working.one"; vars = { name: name, app: working[0] } }
        else if (working.length === 2) { line = "act.working.two"; vars = { name: name, app: working[0], app2: working[1] } }
        setFace(res, "working", "working", line, vars, "working", { apps: working.slice(0, 3) })
    } else if (testing) {
        var tc = t.client === undefined ? null : t.client
        setFace(res, "testing", "working", "act.working.one", { name: name, app: tc }, "working", { apps: [tc] })
    } else if (cands.length > 0) {
        var c = cands[0]
        setFace(res, "celebrate", "star", c.line, c.vars, "celebrate",
            c.kind === "grew" ? { star: true, confetti: true } : { star: true })
        res.celebrate = { kind: c.kind, seq: c.seq, at_ms: c.at_ms }
    } else if (oops) {
        setFace(res, "oops", star, "act.oops", { app: ag.error_app === undefined ? null : ag.error_app },
            now < ea + C.error_frame_ms ? "error" : "idle", null)
    } else if (soonNeeds.length > 0) {
        var sn = soonNeeds[0]
        setFace(res, "need_soon", star, "mood." + sn.need + ".soon", { name: name }, "needs_care_frame", { need: sn.need })
    } else {
        setFace(res, "idle", star, "mood.happy", { name: name }, "wander", ag.helpers_busy === true ? { helpers: true } : null)
    }
    return res
}

function setFace(res, state, bar, line, vars, overlay, bubble) {
    res.state = state
    res.bar = bar
    res.line = line
    res.vars = vars
    res.overlay = overlay
    res.bubble = bubble
}
"""


def render(vectors_bytes: bytes) -> str:
    v = json.loads(vectors_bytes)
    p = v["priority"]
    clients = [c["id"] for c in v["clients"]]
    names = {c["id"]: c["name"] for c in v["clients"]}
    missing = [k for k in CONSTANTS if k not in v["constants"]]
    if missing:
        raise SystemExit(f"agent_vectors.json constants lack {missing}")
    consts = {k: v["constants"][k] for k in CONSTANTS}
    rows = [{"row": r["row"], "state": r["state"], "bar": r["bar"], "overlay": r["overlay"]} for r in p["order"]]
    digest = hashlib.sha256(vectors_bytes).hexdigest()

    def js(x):
        return json.dumps(x, ensure_ascii=True, sort_keys=False)

    head = [
        ".pragma library",
        "// GENERATED by tools/petstate/gen_petstate.py from docs/v0.3/contracts/agent_vectors.json;",
        "// do not edit. Regenerate with `python3 tools/petstate/gen_petstate.py`.",
        "// Identical copies: qml/shared/petstate.js (overlay) and omarchy-plugin/petstate.js (bar).",
        "// The shared priority (SPEC-v0.3 §6.4): one order for the bar cell, Home and the overlay.",
        "// Pure: no imports, no I/O, no timers. Tested by qml/tests/test_petstate.mjs.",
        "",
        f"var SOURCE_SHA256 = {js(digest)}",
        f"var CLIENTS = {js(clients)}",
        f"var CLIENT_NAMES = {js(names)}",
        f"var NEED_LEVELS = {js(p['need_levels'])}",
        f"var NEED_PRIORITY = {js(p['need_priority'])}",
        f"var C = {js(consts)}",
        "// The rows in order (documentation and tests; the function below is the authority).",
        f"var ROWS = {js(rows)}",
    ]
    return "\n".join(head) + "\n\n" + BODY.lstrip("\n")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--check", action="store_true", help="compare instead of writing")
    args = ap.parse_args()
    text = render(VECTORS.read_bytes())
    stale = []
    for out in OUTPUTS:
        if args.check:
            if not out.exists() or out.read_text() != text:
                stale.append(str(out.relative_to(REPO)))
        else:
            out.write_text(text)
    if stale:
        print("petstate.js is stale or missing (run python3 tools/petstate/gen_petstate.py): "
              + ", ".join(stale), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
