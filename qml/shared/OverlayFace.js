.pragma library

// Pure presentation rules for the desktop pet (SPEC-v0.3 §3.8). petstate.js decides the
// face (one order for the bar, Home and the overlay); this file turns its `overlay` and
// `bubble` fields into an animation state and a picture-only thought bubble. No imports,
// no I/O, no timers. Node-tested by qml/tests/test_overlay_flow.mjs.

var CARE_SETS = ["floor", "aquatic", "robot"]
var NEEDS = ["food", "water", "play", "wash"]
// Brand-neutral app tiles (qml/assets/icons/apps/), one per supported client id.
var APP_TILES = {
    "claude-code": "apps/app_claude_code.webp",
    "codex": "apps/app_codex.webp",
    "opencode": "apps/app_opencode.webp",
    "hermes": "apps/app_hermes.webp",
    "grok-cli": "apps/app_grok.webp"
}
var ICONS = {
    star: "care-floor/star.webp",
    heart: "care-floor/heart.webp",
    bandage: "status/bandage.webp",
    confetti: "status/confetti.webp",
    helpers: "status/robot_helpers.webp",
    wait: "status/bubble_wait.webp"
}
// Overlay values of petstate.js -> the pet's animation and whether it may roam.
//   none: no pet drawn (Off, no pet); the process exits on Off.
//   needs_care_frame: a need at "soon" shows the needs_care frame between walks.
//   idle: the oops line after its error frame.
var ANIMATIONS = {
    none: { state: "", roam: false, bob: false },
    sleep: { state: "sleep", roam: false, bob: false },
    waiting: { state: "waiting", roam: false, bob: true },
    needs_care: { state: "needs_care", roam: false, bob: false },
    needs_care_frame: { state: "needs_care", roam: true, bob: false },
    working: { state: "working", roam: true, bob: false },
    celebrate: { state: "idle", roam: false, bob: false },
    error: { state: "error", roam: false, bob: false },
    idle: { state: "idle", roam: true, bob: false },
    wander: { state: "idle", roam: true, bob: false }
}

function own(o, k) {
    return o !== null && typeof o === "object" && Object.prototype.hasOwnProperty.call(o, String(k))
}

// {state, roam, bob} for a face's `overlay` value; unknown values fall back to idle.
function animation(overlay) {
    return own(ANIMATIONS, overlay) ? ANIMATIONS[overlay] : ANIMATIONS.idle
}

// Owned jobs have a care icon; observed coding clients keep their app icon.
function withWork(face, work) {
    if (!face || ["off", "asleep", "fainted", "paused", "no_pet", "welcome", "need_now", "need_soon", "celebrate"].indexOf(face.state) >= 0
            || !work || !work.task) return face
    var status = work.task.status
    if (["preparing", "running", "review", "applying", "scouted"].indexOf(status) < 0) return face
    var next = Object.assign({}, face)
    if (status === "scouted" && !work.task.has_finding) return face
    next.state = status === "scouted" ? "idle" : status === "review" || status === "applying" ? "waiting" : "working"
    next.overlay = next.state
    next.bubble = work.task.source_kind === "scout" || work.task.source_kind === "scout_fix" ? {} : { need: work.task.kind === "bug" ? "food" : work.task.kind === "research" ? "water" : "play" }
    next.celebrate = null
    return next
}

// The species' care icon set: its `careIconSet`, else from its habitat (SPEC-v0.3 §4.1).
function careIconSet(species) {
    if (own(species, "careIconSet") && CARE_SETS.indexOf(species.careIconSet) >= 0)
        return species.careIconSet
    return own(species, "habitat") && species.habitat === "aquatic" ? "aquatic" : "floor"
}

// A care/owned-work bubble has no client ids, so it cannot borrow an observer label.
function observerLabel(face, names) {
    if (!face || ["working", "waiting", "testing"].indexOf(face.state) < 0
            || !face.bubble || !Array.isArray(face.bubble.apps)) return ""
    var apps = face.bubble.apps.filter(function (id) { return own(APP_TILES, id) && own(names, id) })
    if (apps.length === 0) return ""
    return names[apps[0]] + (apps.length > 1 ? " +" + (apps.length - 1) : "")
        + (face.state === "waiting" ? " needs you" : face.state === "testing" ? " checking" : " working")
}

function appTile(id) {
    return own(APP_TILES, id) ? APP_TILES[id] : ""
}

// Icon paths (relative to qml/assets/icons/) for a face bubble, in drawing order, at most
// four. `waiting` is true when the face is an app that needs you: a "!" bubble precedes
// its app tile. Unknown ids are skipped; an empty list draws no bubble.
function bubbleIcons(bubble, careSet, waiting) {
    var out = []
    if (bubble === null || typeof bubble !== "object")
        return out
    if (Array.isArray(bubble.apps)) {
        if (waiting === true)
            out.push(ICONS.wait)
        for (var i = 0; i < bubble.apps.length && i < 3; i++) {
            var tile = appTile(bubble.apps[i])
            if (tile !== "")
                out.push(tile)
        }
    }
    if (typeof bubble.need === "string" && NEEDS.indexOf(bubble.need) >= 0)
        out.push("care-" + (CARE_SETS.indexOf(careSet) >= 0 ? careSet : "floor") + "/" + bubble.need + ".webp")
    if (bubble.bandage === true)
        out.push(ICONS.bandage)
    if (bubble.star === true)
        out.push(ICONS.star)
    if (bubble.confetti === true)
        out.push(ICONS.confetti)
    if (bubble.helpers === true)
        out.push(ICONS.helpers)
    return out.slice(0, 4)
}

// A key that changes exactly when a new celebration should play once.
function celebrationKey(celebrate) {
    if (celebrate === null || typeof celebrate !== "object")
        return ""
    return String(celebrate.kind) + ":" + String(celebrate.seq) + ":" + String(celebrate.at_ms)
}

// `seen` after a celebration was shown (petstate.js `seen` input; grew is tracked apart).
function markSeen(seen, celebrate) {
    var out = { finished_seq: 0, hello_seq: 0, test_at_ms: 0 }
    if (seen !== null && typeof seen === "object") {
        for (var k in out)
            if (typeof seen[k] === "number" && isFinite(seen[k])) out[k] = seen[k]
    }
    if (celebrate === null || typeof celebrate !== "object")
        return out
    if (celebrate.kind === "finished" && typeof celebrate.seq === "number")
        out.finished_seq = Math.max(out.finished_seq, celebrate.seq)
    else if (celebrate.kind === "hello" && typeof celebrate.seq === "number")
        out.hello_seq = Math.max(out.hello_seq, celebrate.seq)
    else if (celebrate.kind === "test" && typeof celebrate.at_ms === "number")
        out.test_at_ms = Math.max(out.test_at_ms, celebrate.at_ms)
    return out
}

// What a new desktop-pet process counts as already celebrated: everything in the first
// export it draws. A celebration therefore plays once on the desktop, never again after a
// restart of the overlay (Off to On, the switch off and on, a bar or session restart);
// one that happened before the process started is left to the bar and Home.
// Returns {seen, grewSeq} (grewSeq -1 = no growth waiting).
function seenAtStart(doc) {
    var seen = { finished_seq: 0, hello_seq: 0, test_at_ms: 0 }
    var grewSeq = -1
    if (doc === null || typeof doc !== "object")
        return { seen: seen, grewSeq: grewSeq }
    var a = doc.agent !== null && typeof doc.agent === "object" ? doc.agent : null
    if (a !== null) {
        if (typeof a.finished_seq === "number" && isFinite(a.finished_seq))
            seen.finished_seq = Math.max(0, a.finished_seq)
        if (typeof a.hello_seq === "number" && isFinite(a.hello_seq))
            seen.hello_seq = Math.max(0, a.hello_seq)
        var t = a.test
        if (t !== null && typeof t === "object" && t.state === "done" && typeof t.at_ms === "number"
                && isFinite(t.at_ms))
            seen.test_at_ms = Math.max(0, t.at_ms)
    }
    if (Array.isArray(doc.unseen_events)) {
        for (var i = 0; i < doc.unseen_events.length; i++) {
            var e = doc.unseen_events[i]
            if (e && typeof e.seq === "number" && isFinite(e.seq) && e.seq > grewSeq) grewSeq = e.seq
        }
    }
    return { seen: seen, grewSeq: grewSeq }
}

// The document the face is computed from once growth was celebrated here: the same
// export without its unseen growth (only Home acknowledges growth; the overlay plays it
// once and then shows the next row). Returns `doc` itself when nothing changes.
function withoutGrowth(doc, grewSeq) {
    if (doc === null || typeof doc !== "object" || !Array.isArray(doc.unseen_events)
            || doc.unseen_events.length === 0)
        return doc
    var top = 0
    for (var i = 0; i < doc.unseen_events.length; i++) {
        var e = doc.unseen_events[i]
        if (e && typeof e.seq === "number" && e.seq > top) top = e.seq
    }
    if (!(typeof grewSeq === "number" && grewSeq >= top))
        return doc
    var copy = {}
    for (var k in doc) copy[k] = doc[k]
    copy.unseen_events = []
    return copy
}
