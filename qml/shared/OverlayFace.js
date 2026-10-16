.pragma library

// Pure presentation rules for the desktop pet (SPEC-v0.3 §3.8). petstate.js decides the
// face (one order for the bar, Home and the overlay); this file turns its `overlay` and
// `bubble` fields into an animation state and a picture-only thought bubble. No imports,
// no I/O, no timers. Node-tested by qml/tests/test_overlay_face.mjs.

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
    working: { state: "working", roam: false, bob: false },
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

// The species' care icon set: its `careIconSet`, else from its habitat (SPEC-v0.3 §4.1).
function careIconSet(species) {
    if (own(species, "careIconSet") && CARE_SETS.indexOf(species.careIconSet) >= 0)
        return species.careIconSet
    return own(species, "habitat") && species.habitat === "aquatic" ? "aquatic" : "floor"
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
