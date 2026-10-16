.pragma library

// Pure helpers for the overlay's passive `bar-state.json` input (SPEC-v0.3 §8,
// docs/omarchy-plugin.md). No Quickshell, no I/O, no RPC: BarStateWatcher.qml reads the
// file and the overlay draws straight from the parsed document (petstate.js decides the
// face). Node-tested by qml/tests/test_quickshell_host.py.
//
// The file is written by Rust under the database lock (temp + rename), so
// `state_version` only grows **within one database** (`db_id`). A restored, recreated or
// swapped database starts a new order (BarStateWatcher.qml). Anything malformed is
// ignored as a whole (fail closed): the overlay keeps the last valid document.
//
// Readers accept codepet.bar.v1 and codepet.bar.v2 for one release (SPEC-v0.3 §8). A v2
// document must carry the v0.3 blocks (`enabled`, `agent`, `treats`, `clients`); a v1
// document may carry them too (the v0.3.0 writer keeps the v1 label until both readers
// accept v2) and is validated the same way when it does.

var SCHEMAS = ["codepet.bar.v1", "codepet.bar.v2"]
var CLIENT_IDS = ["claude-code", "codex", "opencode", "hermes", "grok-cli"]
var APP_STATES = ["working", "waiting", "error"]
var AGENT_STATES = ["idle", "error", "working", "waiting"]
var TEST_STATES = ["running", "done", "failed"]
var MAX_APPS = 5
var JAR_MAX = 10
var MAX_BYTES = 16384
var METERS = ["hunger", "thirst", "mental_health", "hygiene"]
var CARE_BOOLS = ["sleeping", "sleep_due", "bathroom_due"]
// Enumerated values (life status, need state, task state, mode, pace) are owned by
// the Rust side; the overlay only checks their token shape so a new value never
// blinds the pet, and renders unknown values through the shared fallbacks.
var TOKEN = /^[a-z_]{1,32}$/
var PET_ID = /^[a-z_]{2,24}$/
var DB_ID = /^db-[0-9a-f]{24}$/
var MAX_REASONS = 8
var MAX_REASON_BYTES = 160
var MAX_EVENTS = 64

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

function isObject(v) {
    return v !== null && typeof v === "object" && !Array.isArray(v)
}

function isCount(v) {
    return typeof v === "number" && isFinite(v) && Math.floor(v) === v && v >= 0
           && v <= 9007199254740991
}

function isMs(v) {
    return typeof v === "number" && isFinite(v) && Math.floor(v) === v
           && Math.abs(v) <= 9007199254740991
}

function isToken(v) {
    return typeof v === "string" && TOKEN.test(v)
}

function boundedText(v, maxBytes) {
    return typeof v === "string" && utf8Bytes(v) <= maxBytes
}

// `$XDG_STATE_HOME/codepet/bar-state.json`, default `~/.local/state/codepet/`.
// A relative XDG_STATE_HOME is ignored (XDG base-directory rule). Returns "" when
// no absolute base exists, which leaves the watcher idle.
function statePath(xdgStateHome, home) {
    var base = ""
    if (typeof xdgStateHome === "string" && xdgStateHome.charAt(0) === "/")
        base = xdgStateHome
    else if (typeof home === "string" && home.charAt(0) === "/")
        base = home.replace(/\/+$/, "") + "/.local/state"
    if (base === "")
        return ""
    return base.replace(/\/+$/, "") + "/codepet/bar-state.json"
}

function validMeters(m) {
    if (!isObject(m))
        return false
    for (var i = 0; i < METERS.length; i++) {
        var v = m[METERS[i]]
        if (!isCount(v) || v > 10000)
            return false
    }
    return true
}

function validLife(l) {
    if (l === null)
        return true
    if (!isObject(l))
        return false
    if (!boundedText(l.life_id, 128) || !boundedText(l.name, 96))
        return false
    if (typeof l.appearance !== "string" || !PET_ID.test(l.appearance))
        return false
    if (!isCount(l.stage) || l.stage < 1 || l.stage > 3)
        return false
    if (!isCount(l.level) || !isCount(l.xp))
        return false
    if (!isToken(l.status))
        return false
    if ((l.mode !== undefined && !isToken(l.mode)) || (l.pace !== undefined && !isToken(l.pace))
            || (l.ordinal !== undefined && !isCount(l.ordinal)))
        return false
    var flags = ["armed", "paused", "fainted", "death_pending"]
    for (var i = 0; i < flags.length; i++) {
        if (typeof l[flags[i]] !== "boolean")
            return false
    }
    return true
}

function validCare(c) {
    if (!isObject(c))
        return false
    for (var i = 0; i < CARE_BOOLS.length; i++) {
        if (typeof c[CARE_BOOLS[i]] !== "boolean")
            return false
    }
    if (!isMs(c.sleep_remaining_ms))
        return false
    if (c.sleep_started_at_ms !== null && c.sleep_started_at_ms !== undefined
            && !isMs(c.sleep_started_at_ms))
        return false
    // v0.3.0 compatibility values (always false / []), dropped in v0.3.1: optional here.
    if (c.ready_to_work !== undefined && typeof c.ready_to_work !== "boolean")
        return false
    var r = c.readiness_reasons === undefined ? [] : c.readiness_reasons
    if (!Array.isArray(r) || r.length > MAX_REASONS)
        return false
    for (var j = 0; j < r.length; j++) {
        if (!boundedText(r[j], MAX_REASON_BYTES))
            return false
    }
    return true
}

function validTask(t) {
    if (t === null)
        return true
    return isObject(t) && isToken(t.state)
           && (t.category === undefined || t.category === null || isToken(t.category))
}

function validNeeds(n) {
    if (n === undefined)
        return true
    if (!Array.isArray(n) || n.length > METERS.length)
        return false
    for (var i = 0; i < n.length; i++) {
        var e = n[i]
        if (!isObject(e) || METERS.indexOf(e.need) < 0 || !isToken(e.action) || !isToken(e.state))
            return false
        if ((e.until_warning_ms !== undefined && e.until_warning_ms !== null && !isMs(e.until_warning_ms))
                || (e.until_critical_ms !== undefined && e.until_critical_ms !== null && !isMs(e.until_critical_ms))
                || (e.until_end_ms !== undefined && e.until_end_ms !== null && !isMs(e.until_end_ms)))
            return false
    }
    return true
}

function validEvents(ev) {
    if (ev === undefined)
        return true
    if (!Array.isArray(ev) || ev.length > MAX_EVENTS)
        return false
    for (var i = 0; i < ev.length; i++) {
        if (!isObject(ev[i]) || !isCount(ev[i].seq) || !isCount(ev[i].stage)
                || ev[i].stage < 1 || ev[i].stage > 3)
            return false
    }
    return true
}

function isMsOrNull(v) {
    return v === null || isCount(v)
}

function isClientOrNull(v) {
    return v === null || CLIENT_IDS.indexOf(v) >= 0
}

function validApp(a) {
    return isObject(a) && CLIENT_IDS.indexOf(a.id) >= 0 && APP_STATES.indexOf(a.state) >= 0
           && isCount(a.since_ms) && isCount(a.stale_at_ms)
}

function validTest(t) {
    return t === null || (isObject(t) && TEST_STATES.indexOf(t.state) >= 0 && isCount(t.at_ms)
                          && isClientOrNull(t.client))
}

// The v0.3 `agent` block (bar.v2.schema.json $defs/agent), bounded.
function validAgent(a) {
    if (!isObject(a) || AGENT_STATES.indexOf(a.state) < 0 || !Array.isArray(a.apps) || a.apps.length > MAX_APPS)
        return false
    for (var i = 0; i < a.apps.length; i++) {
        if (!validApp(a.apps[i]))
            return false
    }
    return isMsOrNull(a.stale_at_ms) && isCount(a.finished_seq) && isMsOrNull(a.finished_at_ms)
           && isClientOrNull(a.finished_app) && typeof a.finished_treat === "boolean"
           && typeof a.finished_jar_full === "boolean" && isCount(a.hello_seq)
           && isMsOrNull(a.hello_at_ms) && isClientOrNull(a.hello_app) && validTest(a.test)
           && isMsOrNull(a.error_at_ms) && isClientOrNull(a.error_app) && typeof a.helpers_busy === "boolean"
}

function validTreats(t) {
    return isObject(t) && isCount(t.jar) && t.jar <= JAR_MAX && typeof t.full === "boolean"
}

// Only the shape the overlay relies on: an object keyed by client ids (WP5 reads the rest).
function validClients(c) {
    if (!isObject(c))
        return false
    for (var k in c) {
        if (CLIENT_IDS.indexOf(k) < 0 || !isObject(c[k]))
            return false
    }
    return true
}

// True when the v0.3 blocks are present and valid (required for v2), or all absent (v1).
function validV03(s) {
    var keys = ["enabled", "agent", "treats", "clients"]
    var present = 0
    for (var i = 0; i < keys.length; i++) {
        if (s[keys[i]] !== undefined)
            present += 1
    }
    if (present === 0)
        return s.schema === "codepet.bar.v1"
    if (present !== keys.length)
        return false
    return typeof s.enabled === "boolean" && validAgent(s.agent) && validTreats(s.treats)
           && validClients(s.clients)
}

function validMemorial(m) {
    if (m === undefined)
        return true
    return isObject(m) && isCount(m.count)
           && (m.latest_name === null || m.latest_name === undefined || boundedText(m.latest_name, 96))
}

// Bounded parse. Returns the validated object, or null for anything else.
function parse(text) {
    if (typeof text !== "string" || text.length === 0 || text.length > MAX_BYTES
            || utf8Bytes(text) > MAX_BYTES)
        return null
    var s
    try {
        s = JSON.parse(text)
    } catch (e) {
        return null
    }
    if (!isObject(s) || SCHEMAS.indexOf(s.schema) < 0 || !isCount(s.state_version))
        return null
    if (s.db_id !== undefined && !(typeof s.db_id === "string" && DB_ID.test(s.db_id)))
        return null
    if (!validMeters(s.meters) || !validLife(s.life === undefined ? null : s.life)
            || !validCare(s.care) || !validTask(s.task === undefined ? null : s.task)
            || !validNeeds(s.needs) || !validEvents(s.unseen_events) || !validMemorial(s.memorial))
        return null
    if (s.critical !== undefined && s.critical !== null && !isObject(s.critical))
        return null
    if (!validV03(s))
        return null
    return s
}

// True when two parsed files name the same database (an unnamed file, from before
// db_id existed, only matches another unnamed one).
function sameFileDatabase(a, b) {
    var x = isObject(a) && typeof a.db_id === "string" ? a.db_id : ""
    var y = isObject(b) && typeof b.db_id === "string" ? b.db_id : ""
    return x === y
}
