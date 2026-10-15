.pragma library

// Pure helpers for the overlay's passive `bar-state.json` input (SPEC-v0.2 §10, §13).
// No Quickshell, no I/O, no RPC: BarStateWatcher.qml reads the file, HostAdapter.qml
// merges the parsed subset into its presentation snapshot. Node-tested by
// qml/tests/test_quickshell_host.py.
//
// The file is a bar-safe subset written by `Store::export_bar_state()` under the
// database lock, so `state_version` only grows. Anything malformed is ignored as a
// whole (fail closed): the overlay keeps its last authoritative snapshot.

var SCHEMA = "codepet.bar.v1"
var MAX_BYTES = 16384
var METERS = ["hunger", "thirst", "mental_health", "hygiene"]
var CARE_BOOLS = ["sleeping", "sleep_due", "bathroom_due", "ready_to_work"]
// Enumerated values (life status, need state, task state, mode, pace) are owned by
// the Rust side; the overlay only checks their token shape so a new value never
// blinds the pet, and renders unknown values through the shared fallbacks.
var TOKEN = /^[a-z_]{1,32}$/
var PET_ID = /^[a-z_]{2,24}$/
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
    var r = c.readiness_reasons
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
        if ((e.until_critical_ms !== undefined && e.until_critical_ms !== null && !isMs(e.until_critical_ms))
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
    if (!isObject(s) || s.schema !== SCHEMA || !isCount(s.state_version))
        return null
    if (!validMeters(s.meters) || !validLife(s.life === undefined ? null : s.life)
            || !validCare(s.care) || !validTask(s.task === undefined ? null : s.task)
            || !validNeeds(s.needs) || !validEvents(s.unseen_events) || !validMemorial(s.memorial))
        return null
    if (s.critical !== undefined && s.critical !== null && !isObject(s.critical))
        return null
    return s
}

// True when `bar` carries a strictly newer committed state than `snap`. Without a
// numeric `state_version` on both sides the order is unknown, and the authoritative
// snapshot wins (fail closed: the passive file never overrides it).
function isNewer(snap, bar) {
    if (!bar || !isCount(bar.state_version) || !snap || !isCount(snap.state_version))
        return false
    return bar.state_version > snap.state_version
}

function copy(o) {
    var out = {}
    if (isObject(o)) {
        for (var k in o)
            out[k] = o[k]
    }
    return out
}

// Merges the passive subset of a newer `bar-state.json` into a copy of the full
// `state.get` snapshot. Returns `snap` unchanged when `bar` is not newer. Never
// invents task details: the bar subset has no task id, so a task the snapshot does
// not know stays unknown until an explicit refresh.
function merge(snap, bar) {
    if (!snap || !bar || !isNewer(snap, bar))
        return snap
    var out = copy(snap)
    out.state_version = bar.state_version

    var meters = {}
    for (var i = 0; i < METERS.length; i++)
        meters[METERS[i]] = bar.meters[METERS[i]]
    out.meters = meters

    var care = copy(snap.care)
    for (var j = 0; j < CARE_BOOLS.length; j++)
        care[CARE_BOOLS[j]] = bar.care[CARE_BOOLS[j]]
    care.sleep_remaining_ms = bar.care.sleep_remaining_ms
    care.sleep_started_at_ms = bar.care.sleep_started_at_ms === undefined
        ? null : bar.care.sleep_started_at_ms
    care.readiness_reasons = bar.care.readiness_reasons.slice(0)
    out.care = care

    var life = bar.life === undefined ? null : bar.life
    if (life === null) {
        out.life = null
    } else {
        var same = isObject(snap.life) && snap.life.life_id === life.life_id
        var merged = same ? copy(snap.life) : {}
        var keys = ["life_id", "ordinal", "name", "appearance", "stage", "level", "xp",
                    "status", "mode", "armed", "pace", "paused", "fainted", "death_pending"]
        for (var k = 0; k < keys.length; k++) {
            if (life[keys[k]] !== undefined)
                merged[keys[k]] = life[keys[k]]
        }
        out.life = merged
        if (isObject(snap.decay)) {
            var decay = copy(snap.decay)
            decay.paused = life.paused
            out.decay = decay
        }
    }

    var task = bar.task === undefined ? null : bar.task
    if (task === null) {
        out.active_task = null
    } else if (isObject(snap.active_task)) {
        var brief = copy(snap.active_task)
        brief.state = task.state
        if (typeof task.category === "string")
            brief.category = task.category
        out.active_task = brief
    }

    if (bar.unseen_events !== undefined) {
        var events = []
        for (var e = 0; e < bar.unseen_events.length; e++)
            events.push({ "seq": bar.unseen_events[e].seq, "kind": "evolved",
                          "stage": bar.unseen_events[e].stage })
        out.unseen_events = events
    }

    if (isObject(snap.lifecycle) && (bar.needs !== undefined || bar.critical !== undefined)) {
        var lc = copy(snap.lifecycle)
        if (bar.needs !== undefined && Array.isArray(snap.lifecycle.needs)) {
            var needs = []
            for (var n = 0; n < snap.lifecycle.needs.length; n++) {
                var row = copy(snap.lifecycle.needs[n])
                for (var b = 0; b < bar.needs.length; b++) {
                    if (bar.needs[b].need !== row.need)
                        continue
                    row.state = bar.needs[b].state
                    if (bar.needs[b].until_critical_ms !== undefined)
                        row.until_critical_ms = bar.needs[b].until_critical_ms
                    if (bar.needs[b].until_end_ms !== undefined)
                        row.until_end_ms = bar.needs[b].until_end_ms
                }
                if (meters[row.need] !== undefined)
                    row.meter = meters[row.need]
                needs.push(row)
            }
            lc.needs = needs
        }
        if (bar.critical !== undefined)
            lc.critical = bar.critical === null ? null : copy(bar.critical)
        out.lifecycle = lc
    }

    if (bar.memorial !== undefined) {
        var mem = copy(snap.memorial)
        mem.count = bar.memorial.count
        var latestName = bar.memorial.latest_name
        if (latestName === null || latestName === undefined)
            mem.latest = null
        else if (!(isObject(mem.latest) && mem.latest.name === latestName))
            mem.latest = { "name": latestName }
        out.memorial = mem
    }
    return out
}
