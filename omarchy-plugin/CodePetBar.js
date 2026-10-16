.pragma library
// CodePet bar plugin model (v0.3): pure functions, no QML or Quickshell imports,
// node-tested (qml/tests/test_bar_model.mjs). Everything the bar cell and the popup show
// is derived here from the bar-safe `bar-state.json` that Rust writes (readers accept
// codepet.bar.v1 and codepet.bar.v2 for one release, SPEC-v0.3 §8), the asset registry,
// and the copy table in strings.json (passed in as `strings`; see Strings.js).
//
// The face (which state wins, its line and its bubble) comes from petstate.js
// sharedPriority(); this file turns that face into what each surface draws.
//
// Kid copy rules (SPEC-v0.3 §1.2 K1/K2/K8): no numbers, no jargon, short lines. Every word a
// kid sees comes from strings.json; nothing here invents copy.

var SCHEMAS = ["codepet.bar.v1", "codepet.bar.v2"]
var MAX_STATE_BYTES = 16384
var MAX_SETTINGS_BYTES = 1048576
var MAX_REGISTRY_BYTES = 262144

// The only methods the bar may send (SPEC-v0.3 §6.2). Task methods, pet.adopt and
// pet.configure_life are never sent from the shell process.
var ALLOWED_METHODS = [
    "state.get", "pet.care", "pet.sleep", "pet.wake", "pet.pause", "pet.events_seen",
    "pet.start", "pet.rename", "pet.select", "pet.rules", "pet.retire", "pet.memorial",
    "treats.give", "app.set_enabled", "app.set_helpers", "agent.forget_history",
    "clients.status", "clients.connect", "clients.disconnect", "clients.test"
]
var CARE_NEEDS = ["food", "water", "play", "wash"]
var METER_OF = { food: "hunger", water: "thirst", play: "mental_health", wash: "hygiene" }
var NEED_OF_METER = { hunger: "food", thirst: "water", mental_health: "play", hygiene: "wash" }
var NEED_LEVEL = { ok: "happy", low: "soon", empty: "now", critical: "critical" }
var METER_FULL = 10000

var CLIENTS = ["claude-code", "codex", "opencode", "hermes", "grok-cli"]
var APP_NAMES = { "claude-code": "Claude Code", "codex": "Codex", "opencode": "OpenCode",
                  "hermes": "Hermes", "grok-cli": "Grok" }
var APP_ICONS = { "claude-code": "apps/app_claude_code", "codex": "apps/app_codex",
                  "opencode": "apps/app_opencode", "hermes": "apps/app_hermes", "grok-cli": "apps/app_grok" }
var SPECIES = ["cat", "dog", "capybara", "penguin", "robot", "iguana", "spider", "snake", "goldfish",
               "rabbit", "turtle", "frog", "owl", "red_panda", "axolotl"]
var REASON_COPY = { too_old: "reason.too_old", not_installed: "reason.not_installed", blocked: "reason.blocked",
                    settings_changed: "reason.blocked", ready_check: "reason.blocked" }
var SEEN_JUST_NOW_MS = 300000
// XP floor of each stage (codepet-core Rules: level_xp [0, 80, 200, 400, 700], stage
// minimum levels [1, 2, 4]); five stars span the current stage's floor to the next.
var STAGE_FLOOR_XP = [0, 80, 400]
var NAME_MAX_CHARS = 24
var NAME_MAX_BYTES = 96

// Bar cell (icons.json bar_cell): badge icon per shared-priority `bar` value.
var BAR_BADGE = {
    off: "bar/badge_moon", egg: "bar/badge_sparkle", hi: "bar/badge_sparkle", sleep: "",
    sleep_bandage: "bar/badge_bandage", hand: "bar/badge_hand", need_food: "bar/badge_bowl",
    need_water: "bar/badge_drop", need_play: "bar/badge_ball", need_wash: "bar/badge_tub",
    working: "", star: "bar/badge_star", none: ""
}

// ---------------------------------------------------------------- basics

function utf8Bytes(s) {
    var n = 0
    for (var i = 0; i < s.length; i++) {
        var c = s.charCodeAt(i)
        if (c < 0x80) n += 1
        else if (c < 0x800) n += 2
        else if (c >= 0xd800 && c <= 0xdbff) { n += 4; i++ }
        else n += 3
    }
    return n
}

function isObject(v) { return v !== null && typeof v === "object" && !Array.isArray(v) }
function isNum(v) { return typeof v === "number" && isFinite(v) }
function str(v, fallback) { return typeof v === "string" && v.length > 0 ? v : fallback }
function clampText(s, max) { s = String(s); return s.length > max ? s.slice(0, max - 1) + "…" : s }

function boundedJson(text, maxBytes) {
    if (typeof text !== "string" || text.length === 0) return null
    if (text.length > maxBytes || utf8Bytes(text) > maxBytes) return null
    try { return JSON.parse(text) } catch (e) { return null }
}

// ---------------------------------------------------------------- bar-state.json

function defaultAgent() {
    return { state: "idle", apps: [], stale_at_ms: null, finished_seq: 0, finished_at_ms: null,
             finished_app: null, finished_treat: false, finished_jar_full: false, hello_seq: 0,
             hello_at_ms: null, hello_app: null, test: null, error_at_ms: null, error_app: null,
             helpers_busy: false }
}

// bar-state.json: at most 16 KiB, schema codepet.bar.v1 or v2, otherwise null. A v1 file
// (a v0.2 export, or v0.3 Rust before the label switch) gains the v0.3 blocks' defaults,
// so every reader below sees one shape.
function parse(text) {
    return normalize(boundedJson(text, MAX_STATE_BYTES))
}

function normalize(doc) {
    if (!isObject(doc) || SCHEMAS.indexOf(doc.schema) < 0) return null
    if (!isNum(doc.state_version) || doc.state_version < 0 || Math.floor(doc.state_version) !== doc.state_version) return null
    if (doc.life !== null && doc.life !== undefined && !isObject(doc.life)) return null
    var out = {}
    for (var k in doc) out[k] = doc[k]
    if (out.life === undefined) out.life = null
    out.enabled = doc.enabled !== false
    var agent = defaultAgent()
    if (isObject(doc.agent)) for (var a in doc.agent) agent[a] = doc.agent[a]
    if (!Array.isArray(agent.apps)) agent.apps = []
    out.agent = agent
    out.treats = isObject(doc.treats) && isNum(doc.treats.jar)
        ? { jar: Math.max(0, Math.floor(doc.treats.jar)), full: doc.treats.full === true } : { jar: 0, full: false }
    out.clients = isObject(doc.clients) ? doc.clients : {}
    out.care = isObject(doc.care) ? doc.care : { sleeping: false }
    out.needs = Array.isArray(doc.needs) ? doc.needs : []
    out.unseen_events = Array.isArray(doc.unseen_events) ? doc.unseen_events : []
    out.memorial = isObject(doc.memorial) ? doc.memorial : { count: 0, latest_name: null }
    return out
}

// The asset registry (qml/assets/pets/registry.json); null when missing or unusable.
function parseRegistry(text) {
    var doc = boundedJson(text, MAX_REGISTRY_BYTES)
    if (!isObject(doc) || !isObject(doc.species)) return null
    return doc
}

// This widget's own entry in ~/.config/omarchy/shell.json. Injected settings go stale
// after a hot reload, so the file wins; the injected object is the fallback.
function ownSettings(text, moduleName, injected) {
    var fallback = {}
    if (isObject(injected)) for (var k in injected) if (k !== "id") fallback[k] = injected[k]
    var doc = boundedJson(text, MAX_SETTINGS_BYTES)
    if (!isObject(doc) || !isObject(doc.bar)) return fallback
    var holders = [doc.bar.layout, doc.bar]
    var sections = ["left", "center", "right"]
    for (var h = 0; h < holders.length; h++) {
        if (!isObject(holders[h])) continue
        for (var s = 0; s < sections.length; s++) {
            var list = holders[h][sections[s]]
            if (!Array.isArray(list)) continue
            for (var i = 0; i < list.length; i++) {
                var entry = list[i]
                if (isObject(entry) && entry.id === moduleName) {
                    var out = {}
                    for (var key in entry) if (key !== "id") out[key] = entry[key]
                    return out
                }
            }
        }
    }
    return fallback
}

function absolutePath(v) {
    return typeof v === "string" && v.length > 1 && v.charAt(0) === "/" && v.indexOf("\n") < 0 && v.indexOf("\0") < 0
        && v.split("/").indexOf("..") < 0 ? v.replace(/\/+$/, "") : ""
}

function boolSetting(v, fallback) {
    if (v === undefined || v === null || v === "") return fallback
    return String(v) === "true"
}

// Normalized plugin settings; junk falls back to the manifest defaults. The v0.2 label
// mode "status" (a glyph next to the name) is gone: it reads as "name".
function config(settings) {
    var s = isObject(settings) ? settings : {}
    return {
        label: s.label === "none" ? "none" : "name",
        presence: boolSetting(s.presence, true),
        overlayOnLoad: boolSetting(s.overlayOnLoad, false),
        calm: boolSetting(s.calm, false),
        binary: absolutePath(s.binary),
        stateHome: absolutePath(s.stateHome)
    }
}

// Paths (nothing here points inside the plugin folder). env: {HOME, XDG_STATE_HOME, XDG_DATA_HOME, XDG_CONFIG_HOME}.
function paths(cfg, env) {
    env = env || {}
    var home = absolutePath(env.HOME) || "/nonexistent"
    var testState = cfg.stateHome !== ""
    var stateHome = cfg.stateHome || absolutePath(env.XDG_STATE_HOME) || home + "/.local/state"
    var dataHome = absolutePath(env.XDG_DATA_HOME) || home + "/.local/share"
    var configHome = absolutePath(env.XDG_CONFIG_HOME) || home + "/.config"
    var stateDir = stateHome + "/codepet"
    return {
        stateHome: stateHome,
        stateDir: stateDir,
        barState: stateDir + "/bar-state.json",
        presence: stateDir + "/presence.json",
        runtime: dataHome + "/codepet/current/",
        registry: dataHome + "/codepet/current/qml/assets/pets/registry.json",
        binary: cfg.binary || home + "/.local/bin/codepet",
        launcher: home + "/.local/bin/codepet-overlay",
        shellConfig: configHome + "/omarchy/shell.json",
        testState: testState,
        // Extra environment for every BarRpc process; empty = inherit. With a test state home
        // the CLI is told so (CODEPET_TEST_STATE=1): clients.connect / disconnect / test then
        // answer TEST_STATE instead of repointing the real shim (notes.md §7 WP3 item 1).
        childEnv: testState ? { XDG_STATE_HOME: cfg.stateHome, CODEPET_TEST_STATE: "1" } : {},
        // env(1) words for the desktop-pet launcher: the configured binary, a test state
        // home and Calm mode reach the overlay too. Empty = inherit.
        launcherEnv: (cfg.binary ? ["CODEPET_BIN=" + cfg.binary] : [])
            .concat(testState ? ["XDG_STATE_HOME=" + cfg.stateHome] : [])
            .concat(cfg.calm ? ["CODEPET_REDUCED_MOTION=1"] : [])
    }
}

function allowedMethod(m) { return ALLOWED_METHODS.indexOf(m) >= 0 }
function allowedCareAction(a) { return CARE_NEEDS.indexOf(a) >= 0 }
function newRequestId(now, seq) { return "bar-rq-" + Math.max(0, Math.floor(now)).toString(36) + "-" + Math.max(0, Math.floor(seq)) }

// Presses whose repeat must not count twice when an answer was lost (care and treats).
var RETRY_METHODS = ["pet.care", "treats.give"]
// How long an unconfirmed press may be retried under its own key. A press of the same
// button after this window is a new press: reusing the old key would make the backend
// replay the old receipt (no new care applied). The overlay uses the same window.
var RETRY_WINDOW_MS = 120000

// Retry keys, one per press (SPEC-v0.3 K5: a care tap is never dropped, and never counted
// twice). `book` maps a request id to { signature, expiresAt, pending }. A press reuses a key
// only from a record whose answer came back unconfirmed (failure, timeout, lost response;
// the backend stores no receipt for a failure), for the same method and params, inside
// RETRY_WINDOW_MS. A press made while an identical one is still queued or in flight gets a
// fresh key, so both are applied.
var RETRY_BOOK_MAX = 16
function retryClaim(book, signature, now, seq) {
    var next = {}
    var reuse = null
    var keys = isObject(book) ? Object.keys(book) : []
    for (var i = 0; i < keys.length; i++) {
        var e = book[keys[i]]
        if (!isObject(e)) continue
        if (!e.pending && !(isNum(e.expiresAt) && isNum(now) && now <= e.expiresAt)) continue
        next[keys[i]] = { signature: e.signature, expiresAt: e.expiresAt, pending: e.pending === true }
        if (reuse === null && !e.pending && e.signature === signature) reuse = keys[i]
    }
    var key = reuse
    if (key === null) {
        key = "bar-" + Math.max(0, Math.floor(isNum(now) ? now : 0)).toString(36) + "-" + Math.max(0, Math.floor(seq))
        next[key] = { signature: signature, expiresAt: (isNum(now) ? now : 0) + RETRY_WINDOW_MS, pending: true }
    } else {
        next[key].pending = true
    }
    // Bounded: the oldest settled records go first.
    var settled = Object.keys(next).filter(function (k) { return !next[k].pending })
    var size = Object.keys(next).length
    for (var j = 0; size > RETRY_BOOK_MAX && j < settled.length; j++) { delete next[settled[j]]; size-- }
    return { book: next, key: key }
}

// After the answer to the request sent under `key`: a confirmed press forgets its key; an
// unconfirmed one keeps it for RETRY_WINDOW_MS from now, for that press's own signature.
function retryRelease(book, key, resp, now) {
    var next = {}
    var keys = isObject(book) ? Object.keys(book) : []
    for (var i = 0; i < keys.length; i++) if (isObject(book[keys[i]])) next[keys[i]] = book[keys[i]]
    var e = next[key]
    if (!isObject(e)) return next
    if (isObject(resp) && resp.ok === true) delete next[key]
    else next[key] = { signature: e.signature, expiresAt: (isNum(now) ? now : 0) + RETRY_WINDOW_MS, pending: false }
    return next
}

// What the bar widget itself does after an answer, whether or not the popup is still open
// (it may have closed while the request was in flight): Let's go! (pet.start) turns the
// desktop pet on and shows it; turning CodePet on shows it when its switch is on (notes.md
// §8 item 7). Each effect is { setting, value } or { launch }.
function responseEffects(method, params, resp, cfg) {
    if (!isObject(resp) || resp.ok !== true) return []
    if (method === "pet.start") return [{ setting: "overlayOnLoad", value: true }, { launch: "show" }]
    if (method === "app.set_enabled" && isObject(params) && params.enabled === true && isObject(cfg) && cfg.overlayOnLoad === true)
        return [{ launch: "show" }]
    return []
}

// ---------------------------------------------------------------- registry + art

function speciesMeta(reg, id) { return reg && isObject(reg.species) && isObject(reg.species[id]) ? reg.species[id] : null }
function speciesIds(reg) {
    var order = reg && Array.isArray(reg.order) ? reg.order : SPECIES
    var out = []
    for (var i = 0; i < order.length; i++)
        if (typeof order[i] === "string" && /^[a-z_]{2,24}$/.test(order[i]) && (!reg || speciesMeta(reg, order[i]))) out.push(order[i])
    // The cat is always first in the picker (SPEC-v0.3 §3.3).
    var cat = out.indexOf("cat")
    if (cat > 0) { out.splice(cat, 1); out.unshift("cat") }
    return out
}
function defaultName(reg, id) {
    var m = speciesMeta(reg, id)
    return m ? clampText(str(m.defaultName, ""), NAME_MAX_CHARS) : ""
}
function speciesWord(reg, id) {
    var m = speciesMeta(reg, id)
    return m ? str(m.displayName, "") : ""
}

// careIconSet: the manifest value, else aquatic for an aquatic habitat, else floor.
function careSet(reg, id) {
    var m = speciesMeta(reg, id)
    if (m && ["floor", "aquatic", "robot"].indexOf(m.careIconSet) >= 0) return m.careIconSet
    if (m && m.habitat === "aquatic") return "aquatic"
    if (id === "goldfish" || id === "axolotl") return "aquatic"
    if (id === "robot") return "robot"
    return "floor"
}
function careWord(reg, id, need) {
    var m = speciesMeta(reg, id)
    var w = m && isObject(m.careWords) ? m.careWords[need] : null
    return typeof w === "string" && w.length > 0 && w.length <= 40 ? w : ""
}
// Plugin-relative icon path (assets/icons/<sheet>/<id>.webp), never a pet frame.
function iconPath(icon) {
    return typeof icon === "string" && /^[a-z_-]+\/[a-z0-9_]+$/.test(icon) ? "assets/icons/" + icon + ".webp" : ""
}
function careIcon(reg, id, need) {
    return "care-" + careSet(reg, id) + "/" + need
}

function thumbPath(petId, stage) {
    if (typeof petId !== "string" || !/^[a-z_]{2,24}$/.test(petId)) return ""
    if (stage !== 1 && stage !== 2 && stage !== 3) return ""
    return "qml/assets/pets/" + petId + "/thumb-s" + stage + ".webp"
}

// Frame tables (the defaults of qml/shared/PetCatalog.js, which the plugin cannot import);
// a manifest state overrides any field. Only the states the popup and bar draw.
var DEFAULT_STATES = {
    idle:        { frames: ["b00", "b00", "b01", "b00"], frameMs: 600, loop: true, reducedFrame: 0 },
    food:        { frames: ["b03", "c00", "c01", "c00", "c01", "b04"], frameMs: 400, loop: false, reducedFrame: 1 },
    water:       { frames: ["b03", "c02", "c03", "c02", "c03", "b04"], frameMs: 400, loop: false, reducedFrame: 1 },
    play:        { frames: ["b03", "c04", "c05", "c04", "c05", "b04"], frameMs: 400, loop: false, reducedFrame: 1 },
    wash:        { frames: ["b00", "c06", "c07", "c06", "c07", "b04"], frameMs: 400, loop: false, reducedFrame: 1 },
    sleep:       { frames: ["b07"], frameMs: 1000, loop: true, reducedFrame: 0 },
    celebrate:   { frames: ["c11", "b04", "c11", "b04"], frameMs: 400, loop: false, reducedFrame: 0 },
    working:     { frames: ["b04", "b00"], frameMs: 800, loop: true, reducedFrame: 0 },
    waiting:     { frames: ["b02", "b00"], frameMs: 800, loop: true, reducedFrame: 0 },
    needs_care:  { frames: ["b05"], frameMs: 800, loop: true, reducedFrame: 0 },
    error:       { frames: ["b06"], frameMs: 1000, loop: true, reducedFrame: 0 }
}

function stateMeta(reg, id, state) {
    var name = DEFAULT_STATES[state] ? state : "idle"
    var base = DEFAULT_STATES[name]
    var out = { name: name, frames: base.frames.slice(), frameMs: base.frameMs, loop: base.loop, reducedFrame: base.reducedFrame }
    var m = speciesMeta(reg, id)
    var own = m && isObject(m.states) && isObject(m.states[name]) ? m.states[name] : null
    if (!own && m && isObject(m.states) && isObject(m.states.idle) && Array.isArray(m.states.idle.frames))
        out.frames = m.states.idle.frames.slice()
    if (own) {
        if (Array.isArray(own.frames) && own.frames.length > 0) out.frames = own.frames.slice()
        if (isNum(own.frameMs) && own.frameMs >= 50) out.frameMs = own.frameMs
        if (typeof own.loop === "boolean") out.loop = own.loop
        if (isNum(own.reducedFrame)) out.reducedFrame = own.reducedFrame
    }
    out.totalMs = out.frames.length * out.frameMs
    return out
}

function stageOf(life) {
    var s = life && isNum(life.stage) ? Math.floor(life.stage) : 1
    return s >= 1 && s <= 3 ? s : 1
}

function framePath(entry, stage) {
    var e = String(entry)
    if (!/^[A-Za-z0-9_{}./-]{1,64}$/.test(e) || e.indexOf("..") >= 0) return ""
    if (e.indexOf("/") >= 0) return e.replace("{stage}", String(stage))
    return "s" + stage + "/" + (/\.[a-z0-9]+$/i.test(e) ? e : e + ".webp")
}

// Runtime-relative path of one frame: the frame index wraps for loops and holds the last
// frame of a one-shot state; Calm mode shows the state's reduced frame only.
function frameRel(reg, id, stage, state, elapsedMs, calm) {
    if (typeof id !== "string" || !/^[a-z_]{2,24}$/.test(id) || !speciesMeta(reg, id)) return ""
    var meta = stateMeta(reg, id, state)
    var n = meta.frames.length
    var f
    if (calm) f = Math.max(0, Math.min(n - 1, Math.floor(meta.reducedFrame)))
    else {
        f = Math.floor(Math.max(0, Number(elapsedMs) || 0) / meta.frameMs)
        f = meta.loop ? f % n : Math.min(f, n - 1)
    }
    var rel = framePath(meta.frames[f], stage === 1 || stage === 2 || stage === 3 ? stage : 1)
    return rel ? "qml/assets/pets/" + id + "/" + rel : ""
}

// The popup pet's semantic state for a face (SPEC-v0.3 §2.4, §3.2); `clockMs` makes a
// "soon" need show its needs_care frame now and then.
function petStateFor(face, clockMs) {
    var s = face ? face.state : "idle"
    switch (s) {
    case "off": case "fainted": case "paused": case "asleep": return "sleep"
    case "waiting": return "waiting"
    case "need_now": return "needs_care"
    case "working": case "testing": return "working"
    case "celebrate": return "celebrate"
    case "oops": return face.overlay === "error" ? "error" : "idle"
    case "need_soon": return (Math.floor((Number(clockMs) || 0) / 1600) % 4) === 3 ? "needs_care" : "idle"
    default: return "idle"
    }
}

// ---------------------------------------------------------------- copy

// One string: the habitat override (`aquatic.` / `robot.`) when it exists, else the base key.
function template(strings, key, set) {
    if (!isObject(strings)) return ""
    if (set === "aquatic" || set === "robot") {
        var over = strings[set + "." + key]
        if (typeof over === "string") return over
    }
    return typeof strings[key] === "string" ? strings[key] : ""
}

function appName(id) { return APP_NAMES[id] || "" }

// Fills {name}, {app}, {app2} and {word}. App vars are client ids (agent_vectors.json
// priority.vars); they become display names here.
function fill(text, vars) {
    vars = vars || {}
    return String(text).replace(/\{([a-z0-9]+)\}/g, function (m, key) {
        var v = vars[key]
        if (v === undefined || v === null) return ""
        if (key === "app" || key === "app2") return appName(v) || String(v)
        return String(v)
    })
}

function t(strings, key, set, vars) { return fill(template(strings, key, set), vars) }

// The bubble after a care tap (SPEC-v0.3 §3.2): "{name} is full!" when nothing will change;
// otherwise the per-set react line, with the species' careWords {word} when the line takes
// one, else the plain verb.
function reactText(strings, reg, appearance, need, full, name) {
    var set = careSet(reg, appearance)
    if (full) return t(strings, "react.full." + need, set, { name: name })
    var tpl = template(strings, "react." + need, set)
    if (tpl.indexOf("{word}") >= 0) {
        var word = careWord(reg, appearance, need)
        if (word) return fill(tpl, { word: word, name: name })
        var plain = template(strings, "react." + need + ".plain", set)
        return fill(plain || tpl.replace(/\s*\{word\}!?/, ""), { name: name })
    }
    return fill(tpl, { name: name })
}

// The face's line (shared priority) as text.
function faceLine(strings, reg, doc, face) {
    if (!face || !face.line) return ""
    var life = doc && isObject(doc.life) ? doc.life : null
    return t(strings, face.line, careSet(reg, life ? life.appearance : ""), face.vars)
}

// ---------------------------------------------------------------- Home model

function needLevels(doc) {
    var out = { food: "happy", water: "happy", play: "happy", wash: "happy" }
    var list = doc && Array.isArray(doc.needs) ? doc.needs : []
    for (var i = 0; i < list.length; i++) {
        var n = list[i]
        if (!isObject(n)) continue
        var need = NEED_OF_METER[n.need] || (CARE_NEEDS.indexOf(n.action) >= 0 ? n.action : "")
        if (need && NEED_LEVEL[n.state]) out[need] = NEED_LEVEL[n.state]
    }
    return out
}

// True when a care tap will change nothing (Rust: the meter is already full).
function careIsFull(doc, need) {
    var meter = doc && isObject(doc.meters) ? doc.meters[METER_OF[need]] : null
    return isNum(meter) && meter >= METER_FULL
}

// Which controls Home shows for a face (agent_vectors.json priority.home_controls, K4, K6).
// tiles: [{need, glow: "now"|"soon"|"", label key}], wake: "" | "pet.wake" | "pet.pause",
// turnOn, round (the three round buttons), treats.
function homeControls(doc, face) {
    var st = face ? face.state : ""
    var out = { tiles: [], wake: "", turnOn: false, round: false, treats: false }
    if (st === "off") { out.turnOn = true; return out }
    if (st === "asleep") { out.wake = "pet.wake"; return out }
    if (st === "paused") { out.wake = "pet.pause"; return out }
    if (st === "no_pet" || st === "welcome" || !doc || !isObject(doc.life)) return out
    var levels = needLevels(doc)
    var needs = st === "fainted" ? ["food", "water"] : CARE_NEEDS
    for (var i = 0; i < needs.length; i++) {
        var lv = levels[needs[i]]
        out.tiles.push({ need: needs[i], glow: lv === "now" || lv === "critical" ? "now" : lv === "soon" ? "soon" : "" })
    }
    out.round = st !== "fainted"
    out.treats = !!(face && face.treats_visible === true)
    return out
}

// The thought bubble's pictures for a face bubble: [{icon}] (at most three), and its size.
function bubbleIcons(reg, doc, face) {
    var b = face ? face.bubble : null
    if (!isObject(b)) return []
    var life = doc && isObject(doc.life) ? doc.life : null
    var appearance = life ? life.appearance : ""
    if (Array.isArray(b.apps)) {
        var out = []
        for (var i = 0; i < b.apps.length && out.length < 3; i++) if (APP_ICONS[b.apps[i]]) out.push(APP_ICONS[b.apps[i]])
        return out
    }
    if (typeof b.need === "string" && CARE_NEEDS.indexOf(b.need) >= 0) return [careIcon(reg, appearance, b.need)]
    if (b.bandage) return ["status/bandage"]
    if (b.star) return b.confetti ? ["care-floor/star", "status/confetti"] : ["care-floor/star"]
    if (b.helpers) return ["status/robot_helpers"]
    return []
}

// Five stars from the current stage's XP floor to the next stage's floor, rounded down;
// all five at the last stage (SPEC-v0.3 §3.3).
function stars(life) {
    if (!isObject(life)) return 0
    var stage = stageOf(life)
    if (stage >= 3) return 5
    var xp = isNum(life.xp) ? life.xp : 0
    var floor = STAGE_FLOOR_XP[stage - 1]
    var next = isNum(life.next_stage_xp) && life.next_stage_xp > floor ? life.next_stage_xp : STAGE_FLOOR_XP[stage]
    var s = Math.floor(5 * (xp - floor) / (next - floor))
    return Math.max(0, Math.min(5, s))
}

// The client-side name check before pet.start / pet.rename (core normalize_pet_name:
// trimmed, 1..=24 characters, <= 96 bytes, no control characters). "" when fine, else
// the strings.json key to show: an empty name asks for one ("Type a name").
function nameProblem(name) {
    var s = String(name === undefined || name === null ? "" : name).replace(/^\s+|\s+$/g, "")
    if (s.length === 0) return "name.type"
    if (/[\u0000-\u001f\u007f]/.test(s)) return "err.name_too_long"
    var chars = 0
    for (var i = 0; i < s.length; i++) {
        var c = s.charCodeAt(i)
        if (c >= 0xd800 && c <= 0xdbff) i++
        chars++
    }
    if (chars > NAME_MAX_CHARS || utf8Bytes(s) > NAME_MAX_BYTES) return "err.name_too_long"
    return ""
}
function cleanName(name) { return String(name === undefined || name === null ? "" : name).replace(/^\s+|\s+$/g, "") }

// ---------------------------------------------------------------- routing (agent_vectors.json popup_routing)

function route(doc) {
    if (!doc) return "loading"
    var life = isObject(doc.life) ? doc.life : null
    if (doc.enabled === false) return life ? "off_look" : "first_run_off"
    if (!life) return doc.memorial && isNum(doc.memorial.count) && doc.memorial.count > 0 ? "memorial" : "first_run"
    return life.armed === true ? "home" : "welcome_back"
}

// ---------------------------------------------------------------- Helpers rows (agent_vectors.json helpers_rows)

function localDay(ms, tzOffsetMin) { return Math.floor((ms + tzOffsetMin * 60000) / 86400000) }

function seenBucket(seen, now, tzOffsetMin) {
    if (now - seen < SEEN_JUST_NOW_MS) return "row.seen.now"
    if (localDay(seen, tzOffsetMin) === localDay(now, tzOffsetMin)) return "row.seen.today"
    if (localDay(seen, tzOffsetMin) === localDay(now, tzOffsetMin) - 1) return "row.seen.yesterday"
    return "row.seen.while"
}

// How one Helpers row and its sheet look. Copy keys only; the UI fills {app}.
function helperRow(cid, rec, now, tzOffsetMin) {
    if (!isObject(rec) || (rec.installed !== true && rec.connected !== true)) return { visible: false }
    var r = { visible: true, row: null, hint: null, sheet_hints: [], reason: null, step: null,
              stop_watching: false, spinner: false, guide: null }
    var job = isObject(rec.job) ? rec.job : null
    var connected = rec.connected === true
    if (job) {
        r.row = { connect: "row.connecting", disconnect: "row.disconnecting", test: "row.testing" }[job.kind] || "row.connecting"
        r.spinner = true
    } else if (rec.failed_reason) {
        r.row = "row.failed"; r.reason = REASON_COPY[rec.failed_reason] || "reason.blocked"; r.step = "btn.retry"
        r.stop_watching = connected
    } else if (rec.needs_step === "codex_hooks_off") {
        r.row = "row.codex.off"; r.step = "btn.showme"; r.guide = "codex.off"; r.stop_watching = connected
    } else if (rec.needs_step === "codex_trust") {
        r.row = "row.step"; r.step = "btn.showme"; r.guide = "codex.trust"; r.stop_watching = connected
    } else if (connected && (rec.first_seen_ms === null || rec.first_seen_ms === undefined)) {
        r.row = "row.waiting"; r.hint = "hint.open"
        r.sheet_hints = cid === "claude-code" ? ["hint.open", "hint.claude2"] : ["hint.open"]
        r.stop_watching = true
    } else if (connected) {
        var seen = isNum(rec.last_seen_ms) ? rec.last_seen_ms : rec.first_seen_ms
        r.row = seenBucket(seen, now, tzOffsetMin); r.stop_watching = true
    } else if (cid === "codex") {
        r.row = "row.codex.new"; r.step = "btn.connect"
    } else {
        r.row = "row.connect"; r.step = "btn.connect"
    }
    return r
}

function helperRows(clients, now, tzOffsetMin) {
    var out = []
    for (var i = 0; i < CLIENTS.length; i++) {
        var cid = CLIENTS[i]
        var row = helperRow(cid, clients ? clients[cid] : null, now, tzOffsetMin)
        if (row.visible) { row.id = cid; out.push(row) }
    }
    return out
}

// Connect all (and the first-run Connect card's offer): installed, not connected, no
// running job, never Codex; a failed row counts.
function connectAll(clients) {
    var out = []
    for (var i = 0; i < CLIENTS.length; i++) {
        var c = clients ? clients[CLIENTS[i]] : null
        if (CLIENTS[i] !== "codex" && isObject(c) && c.installed === true && c.connected !== true && !c.job) out.push(CLIENTS[i])
    }
    return out
}

// clients.status result -> the ids the Connect card offers (all pre-ticked).
function connectOffered(status) {
    var map = {}
    var list = status && Array.isArray(status.clients) ? status.clients : []
    for (var i = 0; i < list.length; i++) if (isObject(list[i]) && typeof list[i].id === "string") map[list[i].id] = list[i]
    return connectAll(map)
}

// The Connect card's row state for one ticked app after Connect.
function connectRowState(rec) {
    if (!isObject(rec)) return "pending"
    if (isObject(rec.job)) return "busy"
    if (rec.failed_reason) return "failed"
    if (rec.connected === true) return "done"
    return "pending"
}

// How long after Connect a failure the row already had is taken for the old one, while the
// export has not yet shown the new job.
var CONNECT_GRACE_MS = 10000

// The row state as the card shows it. `mark` is what the card noted when it sent Connect:
// { refused: the request itself failed (BUSY, INVALID_STATE, TEST_STATE, a transport
// error: no job will come, the row fails at once), reason: the failed_reason the row had
// then, sawJob: the export has shown a job for it since, at: the send time }. A failure
// still equal to the one from before is the old one: pending, not failed, until a job was
// seen, the reason changed or CONNECT_GRACE_MS passed.
function connectRowView(rec, mark, now) {
    if (isObject(mark) && mark.refused === true) return "failed"
    var st = connectRowState(rec)
    if (st !== "failed" || !isObject(mark) || mark.sawJob === true) return st
    if (mark.reason !== undefined && mark.reason === rec.failed_reason && isNum(mark.at) && isNum(now)
            && now - mark.at < CONNECT_GRACE_MS) return "pending"
    return st
}

// ---------------------------------------------------------------- bar cell

// What the bar cell draws for a face (icons.json bar_cell, SPEC-v0.3 §3.1).
// thumb: runtime-relative pet art or a plugin icon (thumbIsIcon), opacity, badge icon,
// pulse, and the label.
function barCell(doc, face, reg, cfg, calm) {
    var life = doc && isObject(doc.life) ? doc.life : null
    var bar = face ? face.bar : "egg"
    var out = { thumb: "", thumbIsIcon: false, opacity: 1, badge: BAR_BADGE[bar] || "", pulse: false, label: "" }
    if (!doc) {
        out.thumb = "ui/egg"; out.thumbIsIcon = true; out.badge = ""
        out.label = cfg && cfg.label === "none" ? "" : "CodePet"
        return out
    }
    var name = life ? clampText(str(life.name, ""), NAME_MAX_CHARS) : ""
    var stage = stageOf(life)
    if (!life || bar === "egg") {
        out.thumb = "ui/egg"; out.thumbIsIcon = true
    } else if (bar === "sleep" || bar === "sleep_bandage") {
        out.thumb = frameRel(reg, life.appearance, stage, "sleep", 0, true) || thumbPath(life.appearance, stage)
    } else if (bar === "working") {
        out.thumb = frameRel(reg, life.appearance, stage, "working", 0, true) || thumbPath(life.appearance, stage)
    } else {
        out.thumb = thumbPath(life.appearance, stage)
    }
    if (bar === "off") out.opacity = 0.4
    out.pulse = bar === "hand" && face && face.pulse === true && !calm
    if (cfg && cfg.label === "none") out.label = ""
    else if (bar === "egg" || bar === "hi") out.label = "" // the "Hi!" text comes from strings (bar.hi)
    else out.label = name
    out.hi = bar === "egg" || bar === "hi"
    return out
}

// ---------------------------------------------------------------- kid copy checks (K1, K2, K8)

var JARGON = ["session", "hook", "turn", "provider", "xp", "level", "percent", "simulation", "preflight",
              "token", "armed", "attended", "presence", "daemon", "config", "rpc", "plugin", "marketplace",
              "client", "agent", "trust"]
var VERB_PHRASES = [/\bturn(ed)? (on|off)\b/g, /\bturn \S+ into/g]
var EXEMPT_LITERALS = ["/reload-plugins"]

function wordCount(text) {
    var parts = String(text).replace(/^\s+|\s+$/g, "").split(/\s+/)
    var n = 0
    for (var i = 0; i < parts.length; i++) if (/[A-Za-z{]/.test(parts[i])) n++
    return n
}

// Problems of one rendered kid-facing line: [] when it passes K1, K2 and K8.
function kidTextProblems(text) {
    var out = []
    var s = String(text)
    for (var e = 0; e < EXEMPT_LITERALS.length; e++) s = s.split(EXEMPT_LITERALS[e]).join(" ")
    if (/[0-9%]/.test(s)) out.push("K1 digit or %")
    var low = s.toLowerCase()
    for (var v = 0; v < VERB_PHRASES.length; v++) low = low.replace(VERB_PHRASES[v], " ")
    for (var j = 0; j < JARGON.length; j++)
        if (new RegExp("\\b" + JARGON[j]).test(low)) out.push("K2 " + JARGON[j])
    if (wordCount(text) > 8) out.push("K8 more than 8 words")
    return out
}

// When-words for Memories (strings when.*), from a past time.
function whenKey(ms, now, tzOffsetMin) {
    if (!isNum(ms)) return "when.while"
    var b = seenBucket(ms, now, tzOffsetMin)
    return { "row.seen.now": "when.now", "row.seen.today": "when.today", "row.seen.yesterday": "when.yesterday" }[b] || "when.while"
}

// Unseen evolution events: the highest seq to acknowledge with pet.events_seen.
function maxUnseenSeq(events) {
    var best = -1
    if (!Array.isArray(events)) return best
    for (var i = 0; i < events.length; i++) {
        var e = events[i]
        if (isObject(e) && isNum(e.seq) && e.seq > best && (e.kind === undefined || e.kind === "evolved")) best = e.seq
    }
    return best
}

// The `seen` record sharedPriority() takes, advanced past the celebration a face shows.
function seenAfter(seen, doc, face) {
    var out = { finished_seq: 0, hello_seq: 0, test_at_ms: 0 }
    if (isObject(seen)) for (var k in out) if (isNum(seen[k])) out[k] = seen[k]
    if (!doc || !face || face.state !== "celebrate") return out
    var ag = doc.agent || {}
    if (face.line === "act.ready" && ag.test && isNum(ag.test.at_ms)) out.test_at_ms = Math.max(out.test_at_ms, ag.test.at_ms)
    else if (face.line === "act.hello" && isNum(ag.hello_seq)) out.hello_seq = Math.max(out.hello_seq, ag.hello_seq)
    else if (/^act\.done/.test(String(face.line)) && isNum(ag.finished_seq)) out.finished_seq = Math.max(out.finished_seq, ag.finished_seq)
    return out
}
