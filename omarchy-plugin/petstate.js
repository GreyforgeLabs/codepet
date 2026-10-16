.pragma library
// sharedPriority(export, now, seen) -> face: the one order that drives the bar badge, the
// Home face and line, and the desktop pet (SPEC-v0.3 §6.4; contract:
// docs/v0.3/contracts/agent_vectors.json `priority`, replayed by qml/tests/test_bar_model.mjs).
//
// Hand-written by WP5 so the popup can be built before WP6 lands its generator. WP6 owns
// petstate.js (generated from agent_vectors.json into qml/shared/ and omarchy-plugin/); its
// generated file replaces this one unchanged, so keep this API: sharedPriority(doc, now, seen)
// with the vector `expect` fields {state, bar, line, vars, overlay, pulse, treats_visible,
// bubble, wake_at_ms}.

var CLIENT_ORDER = ["claude-code", "codex", "opencode", "hermes", "grok-cli"]
var NEED_PRIORITY = ["food", "water", "play", "wash"]
var NEED_LEVELS = { ok: "happy", low: "soon", empty: "now", critical: "critical" }
var C = {
    celebrate_window_ms: 30000,
    celebrate_grace_ms: 2000,
    test_done_delay_ms: 1500,
    test_arrival_ms: 5000,
    error_recent_ms: 120000,
    error_frame_ms: 3000
}

function num(v) { return typeof v === "number" && isFinite(v) }

function sharedPriority(doc, now, seen) {
    seen = seen || {}
    var seenFinished = num(seen.finished_seq) ? seen.finished_seq : 0
    var seenHello = num(seen.hello_seq) ? seen.hello_seq : 0
    var seenTest = num(seen.test_at_ms) ? seen.test_at_ms : 0
    var res = { state: null, bar: null, line: null, vars: {}, overlay: "none", pulse: false,
                treats_visible: false, bubble: null, wake_at_ms: null }
    var life = doc && doc.life && typeof doc.life === "object" ? doc.life : null
    if (!doc || doc.enabled === false) {
        res.state = "off"; res.bar = "off"
        res.line = life ? "off.line" : null
        res.vars = life ? { name: life.name } : {}
        return res
    }
    if (!life) { res.state = "no_pet"; res.bar = "egg"; return res }
    var name = life.name
    if (life.armed !== true) {
        res.state = "welcome"; res.bar = "hi"; res.line = "picker.welcome"; res.vars = { name: name }; res.overlay = "wander"
        return res
    }
    var ag = doc.agent || {}
    var apps = Array.isArray(ag.apps) ? ag.apps : []
    var counting = apps.filter(function (a) { return a && num(a.stale_at_ms) && a.stale_at_ms > now })
    var working = counting.filter(function (a) { return a.state === "working" }).map(function (a) { return a.id })
    var waiting = counting.filter(function (a) { return a.state === "waiting" }).sort(function (a, b) {
        if (a.since_ms !== b.since_ms) return b.since_ms - a.since_ms
        return CLIENT_ORDER.indexOf(a.id) - CLIENT_ORDER.indexOf(b.id)
    })
    var jar = doc.treats && num(doc.treats.jar) ? doc.treats.jar : 0
    var sleeping = !!(doc.care && doc.care.sleeping === true)
    var tv = jar > 0 && life.fainted !== true && life.paused !== true && !sleeping
    res.treats_visible = tv
    var wakes = counting.map(function (a) { return a.stale_at_ms })
    var W = C.celebrate_window_ms, G = C.celebrate_grace_ms
    var t = ag.test && typeof ag.test === "object" ? ag.test : null
    var testing = false
    if (t && t.state === "running" && num(t.at_ms)) {
        var end = t.at_ms + C.test_done_delay_ms + C.test_arrival_ms
        if (now < end) { testing = true; wakes.push(end) }
    }
    var cands = []
    if (Array.isArray(doc.unseen_events) && doc.unseen_events.length > 0)
        cands.push({ kind: "grew", line: "grew", vars: { name: name } })
    if (t && t.state === "done" && num(t.at_ms) && t.at_ms > seenTest && now < t.at_ms + W) {
        wakes.push(t.at_ms + W)
        cands.push({ kind: "test", line: "act.ready", vars: { name: name } })
    }
    if (num(ag.hello_seq) && ag.hello_seq > seenHello && num(ag.hello_at_ms) && now < ag.hello_at_ms + W) {
        wakes.push(ag.hello_at_ms + W)
        cands.push({ kind: "hello", line: "act.hello", vars: { name: name, app: ag.hello_app } })
    }
    var fa = ag.finished_at_ms
    if (num(ag.finished_seq) && ag.finished_seq > seenFinished && num(fa) && now < fa + W) {
        if (now < fa + G) wakes.push(fa + G)
        else {
            wakes.push(fa + W)
            if (working.indexOf(ag.finished_app) < 0) {
                if (ag.finished_jar_full === true) cands.push({ kind: "finished", line: "act.done_full", vars: {} })
                else if (ag.finished_treat === true) cands.push({ kind: "finished", line: "act.done_treat", vars: { name: name } })
                else cands.push({ kind: "finished", line: "act.done", vars: {} })
            }
        }
    }
    var ea = ag.error_at_ms
    var oops = num(ea) && now < ea + C.error_recent_ms
    if (num(ea)) {
        var ends = [ea + C.error_frame_ms, ea + C.error_recent_ms]
        for (var e = 0; e < ends.length; e++) if (ends[e] > now) wakes.push(ends[e])
    }
    res.wake_at_ms = wakes.length ? Math.min.apply(null, wakes) : null
    var groups = { now: [], soon: [] }
    var needs = Array.isArray(doc.needs) ? doc.needs : []
    for (var i = 0; i < needs.length; i++) {
        var n = needs[i]
        if (!n || typeof n !== "object") continue
        var lv = NEED_LEVELS[n.state]
        if (lv === "now" || lv === "critical") groups.now.push({ need: n.action, level: lv })
        else if (lv === "soon") groups.soon.push({ need: n.action, level: lv })
    }
    var byPriority = function (a, b) { return NEED_PRIORITY.indexOf(a.need) - NEED_PRIORITY.indexOf(b.need) }
    groups.now.sort(byPriority)
    groups.soon.sort(byPriority)
    var star = tv ? "star" : "none"
    function set(state, bar, line, vars, overlay, extra) {
        res.state = state; res.bar = bar; res.line = line; res.vars = vars; res.overlay = overlay
        if (extra) for (var k in extra) res[k] = extra[k]
    }
    if (life.fainted === true) {
        set("fainted", "sleep_bandage", "mood.fainted", { name: name }, "sleep", { bubble: { bandage: true } })
    } else if (life.paused === true) {
        set("paused", "sleep", "mood.asleep", { name: name }, "sleep")
    } else if (sleeping) {
        set("asleep", "sleep", "mood.asleep", { name: name }, "sleep")
    } else if (waiting.length) {
        set("waiting", "hand", "act.waiting", { app: waiting[0].id }, "waiting", { pulse: true, bubble: { apps: [waiting[0].id] } })
    } else if (groups.now.length) {
        var gn = groups.now[0]
        set("need_now", "need_" + gn.need, "mood." + gn.need + "." + gn.level, { name: name }, "needs_care", { bubble: { need: gn.need } })
    } else if (working.length) {
        if (working.length === 1) set("working", "working", "act.working.one", { name: name, app: working[0] }, "working")
        else if (working.length === 2) set("working", "working", "act.working.two", { name: name, app: working[0], app2: working[1] }, "working")
        else set("working", "working", "act.working.many", { name: name }, "working")
        res.bubble = { apps: working.slice(0, 3) }
    } else if (testing) {
        set("testing", "working", "act.working.one", { name: name, app: t.client }, "working", { bubble: { apps: [t.client] } })
    } else if (cands.length) {
        var c = cands[0]
        set("celebrate", "star", c.line, c.vars, "celebrate", { bubble: c.kind === "grew" ? { star: true, confetti: true } : { star: true } })
    } else if (oops) {
        set("oops", star, "act.oops", { app: ag.error_app }, now < ea + C.error_frame_ms ? "error" : "idle")
    } else if (groups.soon.length) {
        var gs = groups.soon[0]
        set("need_soon", star, "mood." + gs.need + ".soon", { name: name }, "needs_care_frame", { bubble: { need: gs.need } })
    } else {
        set("idle", star, "mood.happy", { name: name }, "wander", { bubble: ag.helpers_busy === true ? { helpers: true } : null })
    }
    return res
}
