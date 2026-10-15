.pragma library
.import "PetRegistry.js" as PetRegistry
// Pure lookups over the generated pet registry (SPEC §12 with Amendment A).
// No I/O, no timers, no Quickshell. URLs are relative to qml/shared (where
// PetSprite.qml resolves them) unless install() points the catalog elsewhere.
//
// Asset contract (Amendment A2–A4): every frame is a 192×192 WebP with alpha,
// `<id>/s<stage>/<file>`, and each state lists its frames explicitly, so frames may
// be shared between states. A frame entry may be written as "b00", "b00.webp", or a
// species-relative path containing "/" with an optional "{stage}" token.

var CANVAS = 192
var GROUND_Y = 184
var DEFAULT_BASE = "../assets/pets/"
var CARE_ACTIONS = ["food", "water", "play", "wash", "bathroom", "sleep"]
var GENERIC_CARE_LABELS = {
    food: "Offer food", water: "Fill water", play: "Pet / play", wash: "Wash",
    bathroom: "Bathroom + wash", sleep: "Bed · 2 min"
}

// Amendment A4 frame table with the §11.2 priority/interrupt rules. A manifest state
// overrides any field; a missing manifest state uses this default.
var DEFAULT_STATES = {
    idle:        { frames: ["b00", "b00", "b01", "b00"], frameMs: 600, loop: true, reducedFrame: 0, priority: 0, interrupt: "any" },
    locomotion:  { frames: ["b08", "b09", "b10", "b11"], frameMs: 200, loop: true, reducedFrame: 0, priority: 20, interrupt: "any" },
    food:        { frames: ["b03", "c00", "c01", "c00", "c01", "b04"], frameMs: 400, loop: false, reducedFrame: 1, priority: 70, interrupt: "higher" },
    water:       { frames: ["b03", "c02", "c03", "c02", "c03", "b04"], frameMs: 400, loop: false, reducedFrame: 1, priority: 70, interrupt: "higher" },
    play:        { frames: ["b03", "c04", "c05", "c04", "c05", "b04"], frameMs: 400, loop: false, reducedFrame: 1, priority: 70, interrupt: "higher" },
    wash:        { frames: ["b00", "c06", "c07", "c06", "c07", "b04"], frameMs: 400, loop: false, reducedFrame: 1, priority: 70, interrupt: "higher" },
    bathroom:    { frames: ["b03", "c08", "c08", "b00"], frameMs: 600, loop: false, reducedFrame: 1, priority: 70, interrupt: "higher" },
    sleep_enter: { frames: ["c09", "b07"], frameMs: 600, loop: false, reducedFrame: 1, priority: 90, interrupt: "none" },
    sleep:       { frames: ["b07"], frameMs: 1000, loop: true, reducedFrame: 0, priority: 85, interrupt: "higher" },
    sleep_exit:  { frames: ["c10", "b00"], frameMs: 600, loop: false, reducedFrame: 1, priority: 90, interrupt: "none" },
    celebrate:   { frames: ["c11", "b04", "c11", "b04"], frameMs: 400, loop: false, reducedFrame: 0, priority: 60, interrupt: "higher" },
    working:     { frames: ["b04", "b00"], frameMs: 800, loop: true, reducedFrame: 0, priority: 40, interrupt: "any" },
    waiting:     { frames: ["b02", "b00"], frameMs: 800, loop: true, reducedFrame: 0, priority: 35, interrupt: "any" },
    needs_care:  { frames: ["b05"], frameMs: 800, loop: true, reducedFrame: 0, priority: 50, interrupt: "any" },
    error:       { frames: ["b06"], frameMs: 1000, loop: true, reducedFrame: 0, priority: 100, interrupt: "higher" }
}
var STATE_NAMES = ["idle", "locomotion", "food", "water", "play", "wash", "bathroom", "sleep_enter",
                   "sleep", "sleep_exit", "celebrate", "working", "waiting", "needs_care", "error"]
var LEGACY_STATES = { walk: "locomotion" }
var EMPTY = { schemaVersion: 1, order: [], species: {} }

var _registry = null
var _base = DEFAULT_BASE

function defaultRegistry() {
    try {
        var r = PetRegistry.REGISTRY
        return r && typeof r === "object" && r.species ? r : null
    } catch (e) {
        return null
    }
}

// Points the catalog at another registry and asset base (tests and previews). With
// no arguments it restores the generated registry and the shipped asset directory.
function install(registry, baseUrl) {
    _registry = registry && typeof registry === "object" && registry.species ? registry : null
    var base = baseUrl === undefined || baseUrl === null || baseUrl === "" ? DEFAULT_BASE : String(baseUrl)
    _base = base.charAt(base.length - 1) === "/" ? base : base + "/"
}

function registry() {
    if (_registry === null)
        _registry = defaultRegistry() || EMPTY
    return _registry
}

function baseUrl() {
    return _base
}

function isSpecies(id) {
    var key = String(id || "")
    if (!/^[a-z][a-z0-9_]{0,31}$/.test(key)) return false
    var s = registry().species
    return Object.prototype.hasOwnProperty.call(s, key) && !!s[key]
}

function species(id) {
    return isSpecies(id) ? registry().species[String(id)] : null
}

function ids() {
    var r = registry()
    var order = Array.isArray(r.order) ? r.order : Object.keys(r.species)
    return order.filter(function (id) { return isSpecies(id) })
}

function isAquatic(id) {
    var s = species(id)
    return !!(s && s.habitat === "aquatic")
}

// "walk" → "locomotion"; unknown → "idle"; "memorial" passes through.
function normalizeState(state) {
    var s = String(state || "")
    if (LEGACY_STATES[s]) return LEGACY_STATES[s]
    if (s === "memorial") return s
    return STATE_NAMES.indexOf(s) >= 0 ? s : "idle"
}

function copyMeta(src) {
    var out = {}
    for (var k in src) out[k] = Array.isArray(src[k]) ? src[k].slice() : src[k]
    return out
}

// Effective metadata for a state: defaults, overlaid by the species manifest, with a
// missing state following its fallback chain (ending at idle, cycle-safe).
function stateMeta(id, state) {
    var name = normalizeState(state)
    if (name === "memorial")
        return { name: "memorial", frames: ["_memorial"], frameMs: 1000, loop: true, reducedFrame: 0,
                 priority: 0, interrupt: "any", mirror: false, repeats: 1 }
    var s = species(id)
    var states = s && s.states ? s.states : {}
    var meta = copyMeta(DEFAULT_STATES[name])
    var own = states[name] || null
    // A state missing from the manifest falls back to idle's pictures (fallback chains
    // end at idle) while keeping its own timing and priority rules.
    if (!own && s && states.idle && Array.isArray(states.idle.frames))
        meta.frames = states.idle.frames.slice()
    if (own)
        for (var k in own)
            if (own[k] !== undefined && own[k] !== null || k === "fallback")
                meta[k] = Array.isArray(own[k]) ? own[k].slice() : own[k]
    if (!Array.isArray(meta.frames) || meta.frames.length === 0)
        meta.frames = DEFAULT_STATES[name].frames.slice()
    meta.name = name
    if (meta.repeats === undefined) meta.repeats = 1
    // Aquatic habitats never mirror (Amendment A4); floor species mirror unless told not to.
    meta.mirror = isAquatic(id) ? false : meta.mirror !== false
    return meta
}

function clampStage(id, stage) {
    var s = species(id)
    var count = s && Array.isArray(s.stages) && s.stages.length > 0 ? s.stages.length : 3
    var n = Math.floor(Number(stage))
    if (!isFinite(n) || n < 1) return 1
    return Math.min(n, count)
}

function framePath(entry, stage) {
    var e = String(entry)
    if (e.indexOf("/") >= 0) return e.replace("{stage}", String(stage))
    return "s" + stage + "/" + (/\.[a-z0-9]+$/i.test(e) ? e : e + ".webp")
}

function placeholderUrl() {
    return _base + "_placeholder.webp"
}

function memorialUrl() {
    return _base + "_memorial.webp"
}

function packPrefix(id) {
    return _base + String(id) + "/"
}

// URL of one frame. The frame index wraps into the state's frame list.
function frameRelUrl(id, stage, state, frame) {
    var meta = stateMeta(id, state)
    if (meta.name === "memorial") return memorialUrl()
    if (!isSpecies(id)) return placeholderUrl()
    var n = meta.frames.length
    var f = ((Math.floor(Number(frame) || 0) % n) + n) % n
    return packPrefix(id) + framePath(meta.frames[f], clampStage(id, stage))
}

// What PetSprite shows: {url, mirror, placeholder, state, frame}. Reduced motion uses
// the state's reducedFrame. An unknown species gives the placeholder.
function resolve(id, stage, state, frame, reducedMotion) {
    var meta = stateMeta(id, state)
    if (meta.name === "memorial")
        return { url: memorialUrl(), mirror: false, placeholder: false, state: "memorial", frame: 0 }
    if (!isSpecies(id))
        return { url: placeholderUrl(), mirror: false, placeholder: true, state: meta.name, frame: 0 }
    var n = meta.frames.length
    var r = Number(meta.reducedFrame)
    var f = reducedMotion ? (isFinite(r) && r >= 0 && r < n ? Math.floor(r) : 0)
                          : ((Math.floor(Number(frame) || 0) % n) + n) % n
    return { url: frameRelUrl(id, stage, meta.name, f), mirror: meta.mirror, placeholder: false,
             state: meta.name, frame: f }
}

function thumbRelUrl(id, stage) {
    if (!isSpecies(id)) return placeholderUrl()
    var s = species(id)
    var pattern = typeof s.thumbnailPattern === "string" && s.thumbnailPattern !== "" ? s.thumbnailPattern : "thumb-s{stage}.webp"
    return packPrefix(id) + pattern.replace("{stage}", String(clampStage(id, stage)))
}

function displayName(id) {
    var s = species(id)
    return s && s.displayName ? String(s.displayName) : "Companion"
}

function careLabel(id, action) {
    var a = String(action || "")
    var s = species(id)
    if (s && s.careLabels && typeof s.careLabels[a] === "string" && s.careLabels[a] !== "")
        return s.careLabels[a]
    return GENERIC_CARE_LABELS[a] || "Care"
}

function careHint(id, action) {
    var s = species(id)
    var a = String(action || "")
    return s && s.careHints && typeof s.careHints[a] === "string" ? s.careHints[a] : ""
}

function stageName(id, stage) {
    var s = species(id)
    var n = clampStage(id, stage)
    var stages = s && Array.isArray(s.stages) ? s.stages : []
    for (var i = 0; i < stages.length; i++)
        if (stages[i] && Number(stages[i].stage) === n && stages[i].name) return String(stages[i].name)
    return ["Young", "Grown", "Mature"][n - 1] || ""
}

function validBox(b) {
    return Array.isArray(b) && b.length === 4 && b.every(function (v) { return typeof v === "number" && isFinite(v) })
        && b[2] > 0 && b[3] > 0
}

// Geometry in canvas units with safe defaults; `hitBox`/`water.*` boxes are [x, y, w, h].
function geometry(id) {
    var s = species(id)
    var g = s && s.geometry ? s.geometry : {}
    var vb = Array.isArray(g.viewBox) && g.viewBox.length === 4 && g.viewBox[2] > 0 && g.viewBox[3] > 0
        ? g.viewBox.slice() : [0, 0, CANVAS, CANVAS]
    var aquatic = isAquatic(id)
    var water = aquatic && g.water && validBox(g.water.outer) ? g.water : null
    var stageBoxes = {}
    if (g.stageHitBoxes && typeof g.stageHitBoxes === "object")
        for (var k in g.stageHitBoxes)
            if (validBox(g.stageHitBoxes[k])) stageBoxes[String(k)] = g.stageHitBoxes[k].slice()
    return {
        viewBox: vb,
        groundY: typeof g.groundY === "number" ? g.groundY : GROUND_Y,
        hitBox: validBox(g.hitBox) ? g.hitBox.slice() : [vb[0], vb[1], vb[2], vb[3]],
        // Per-stage boxes: young stages are drawn smaller (0.82/0.92), so the union
        // `hitBox` would capture clicks over transparent pixels around them.
        stageHitBoxes: stageBoxes,
        roams: aquatic ? false : g.roams !== false,
        water: water
    }
}

function roams(id) {
    return isSpecies(id) && geometry(id).roams
}

// Gait parameters: pixels per 200 ms tick (canvas px at a 192 display), the pause
// range in ticks between walks, and the frames that move (hoppers). The airborne frames
// live in `states.locomotion.airborneFrames` (§11.3, measured from the extracted art by
// tools/petart); an older top-level `locomotion.airborneFrames`, then the roster's
// `locomotion.pxFrames`, are fallbacks.
function frameIndexList(v) {
    return Array.isArray(v) ? v.filter(function (n) { return typeof n === "number" && n >= 0 && Math.floor(n) === n }) : []
}
function locomotion(id) {
    var s = species(id)
    var l = s && s.locomotion ? s.locomotion : {}
    var pause = Array.isArray(l.pauseTicks) && l.pauseTicks.length === 2
        && l.pauseTicks[0] >= 1 && l.pauseTicks[1] >= l.pauseTicks[0] ? l.pauseTicks.slice() : [40, 110]
    var stateLoco = s && s.states && s.states.locomotion ? s.states.locomotion : {}
    var airborne = frameIndexList(stateLoco.airborneFrames)
    if (airborne.length === 0) airborne = frameIndexList(l.airborneFrames)
    if (airborne.length === 0) airborne = frameIndexList(l.pxFrames)
    var px = Number(l.pxPerTick)
    return {
        style: typeof l.style === "string" ? l.style : (isAquatic(id) ? "swim_in_habitat" : "walk"),
        pxPerTick: isAquatic(id) ? 0 : (isFinite(px) && px >= 0 ? Math.min(px, 16) : 5),
        pauseTicks: pause,
        airborneFrames: airborne
    }
}

// Clickable rectangle in display pixels for a pet drawn at `size`×`size` at `stage`.
// The stage's own box (`geometry.stageHitBoxes[stage]`, which for aquatic species is
// the habitat as drawn at that stage) is used when present; otherwise aquatic species
// use the habitat (`water.outer`) and floor species the all-stage `hitBox`.
// Image.mirror flips the whole frame about its centre, so a mirrored frame mirrors the
// box's x about the viewBox centre.
function hitRect(id, size, facingLeft, stage) {
    var px = Math.max(0, Number(size) || 0)
    if (!isSpecies(id)) return { x: 0, y: 0, width: px, height: px }
    var g = geometry(id)
    var own = stage !== undefined ? g.stageHitBoxes[String(clampStage(id, stage))] : undefined
    var box = own ? own : g.water ? g.water.outer : g.hitBox
    var vb = g.viewBox
    var sx = px / vb[2], sy = px / vb[3]
    var x = box[0] - vb[0]
    if (facingLeft && !isAquatic(id)) x = vb[2] - (x + box[2])
    var left = Math.max(0, Math.min(px, x * sx))
    var top = Math.max(0, Math.min(px, (box[1] - vb[1]) * sy))
    return {
        x: left,
        y: top,
        width: Math.max(0, Math.min(px - left, box[2] * sx)),
        height: Math.max(0, Math.min(px - top, box[3] * sy))
    }
}
