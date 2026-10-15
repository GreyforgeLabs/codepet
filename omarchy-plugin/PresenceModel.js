.pragma library
// Attended-presence recorder model: pure, no QML imports, node-tested
// (qml/tests/test_presence_model.mjs). PresenceRecorder.qml drives it from one 60 s
// timer on the bar leader; Rust validates and settles what it writes.
//
// Rules (spec §5.1):
// - starting (or restarting) the timer only records the wall time; it never credits;
// - a tick whose wall gap is 50..90 s becomes a pending tick; any other gap (suspend,
//   stall, clock jump, negative) clears every pending tick;
// - a pending tick commits only once it is at least 180 s old (the idle look-back),
//   so the idle timeout's own 180 s are never credited;
// - idle, lock (seen as idle), fullscreen or any eligibility loss clears pending ticks.
//
// Each committed tick credits min(gap, 60 s). Rust requires
// attended_ms <= ticks * 60000 and attended_ms <= (updated - started) + 60000; crediting
// at most the real wall gap keeps both true however long a session runs, and can only
// under-charge.

var TICK_MS = 60000
var MIN_GAP_MS = 50000
var MAX_GAP_MS = 90000
var LOOKBACK_MS = 180000
var MAX_HISTORY = 480 // commits remembered for the badge prediction (8 h, the settle cap)
var MAX_TICKS = 525600
var SESSION_RE = /^bar-[a-z0-9-]{6,58}$/
var LIFE_RE = /^life-[0-9a-f]{24}$/

function isNum(v) { return typeof v === "number" && isFinite(v) }

// "bar-<now base36>-<8 random base36 chars>". rand: () => [0, 1).
function newSession(now, rand) {
    var r = typeof rand === "function" ? rand : Math.random
    var tail = ""
    for (var i = 0; i < 8; i++) tail += Math.floor(Math.max(0, Math.min(0.999999, Number(r()) || 0)) * 36).toString(36)
    var id = "bar-" + Math.max(0, Math.floor(isNum(now) ? now : 0)).toString(36) + "-" + tail
    return SESSION_RE.test(id) ? id : ""
}

function validLifeId(id) { return typeof id === "string" && LIFE_RE.test(id) }

function create(session, lifeId, now) {
    if (!SESSION_RE.test(String(session)) || !validLifeId(lifeId) || !isNum(now)) return null
    return {
        v: 1, session: session, life_id: lifeId,
        started_wall_ms: Math.floor(now), updated_wall_ms: Math.floor(now),
        attended_ms: 0, ticks: 0,
        pending: [],        // [{at, credit}] waiting for the look-back
        lastWall: null,     // wall time of the timer start or the previous tick
        history: [],        // commit wall times (bounded) for attendedSince()
        historyCredit: []
    }
}

// The timer started (or restarted after eligibility returned): note the time only.
function start(model, now) {
    if (!model) return
    model.lastWall = isNum(now) ? now : null
}

function clearPending(model) {
    if (!model) return 0
    var n = model.pending.length
    model.pending = []
    return n
}

// Eligibility dropped (idle, lock, fullscreen, setting off, life gone): drop pending
// ticks and forget the wall mark so the next start never credits the gap.
function stop(model) {
    if (!model) return 0
    model.lastWall = null
    return clearPending(model)
}

// Commit pending ticks stamped at least 180 s before `now`. Returns the count.
function commit(model, now) {
    if (!model || !isNum(now)) return 0
    var n = 0
    while (model.pending.length > 0 && model.pending[0].at <= now - LOOKBACK_MS && model.ticks < MAX_TICKS) {
        var p = model.pending.shift()
        model.attended_ms += p.credit
        model.ticks += 1
        n += 1
        model.history.push(now)
        model.historyCredit.push(p.credit)
    }
    if (model.history.length > MAX_HISTORY) {
        model.history = model.history.slice(-MAX_HISTORY)
        model.historyCredit = model.historyCredit.slice(-MAX_HISTORY)
    }
    if (n > 0) model.updated_wall_ms = Math.max(model.updated_wall_ms, Math.floor(now))
    return n
}

// One timer tick. Returns the number of ticks committed (a write is due when > 0).
function onTick(model, now, eligible, idle) {
    if (!model || !isNum(now)) return 0
    if (!eligible || idle) {
        stop(model)
        return 0
    }
    if (model.lastWall === null || !isNum(model.lastWall)) {
        model.lastWall = now
        return 0
    }
    var gap = now - model.lastWall
    model.lastWall = now
    if (gap >= MIN_GAP_MS && gap <= MAX_GAP_MS) model.pending.push({ at: now, credit: Math.min(gap, TICK_MS) })
    else clearPending(model)
    return commit(model, now)
}

// presence.json contents: exactly the seven fields Rust accepts (deny_unknown_fields).
function payload(model) {
    if (!model) return ""
    return JSON.stringify({
        v: 1, session: model.session, life_id: model.life_id,
        started_wall_ms: model.started_wall_ms, updated_wall_ms: model.updated_wall_ms,
        attended_ms: model.attended_ms, ticks: model.ticks
    }) + "\n"
}

// Attended time committed strictly after `wallMs` (for the leader's predicted badge).
function attendedSince(model, wallMs) {
    if (!model || !isNum(wallMs)) return 0
    var total = 0
    for (var i = model.history.length - 1; i >= 0; i--) {
        if (model.history[i] <= wallMs) break
        total += model.historyCredit[i]
    }
    return total
}

// Mirror of the Rust validator (codepet-core presence::validate) for self-checks.
function valid(model, nowWall) {
    if (!model) return false
    return model.v === 1 && SESSION_RE.test(model.session) && validLifeId(model.life_id)
        && model.attended_ms >= 0 && model.attended_ms <= model.ticks * TICK_MS
        && model.attended_ms <= (model.updated_wall_ms - model.started_wall_ms) + TICK_MS
        && model.started_wall_ms <= model.updated_wall_ms
        && (!isNum(nowWall) || model.updated_wall_ms <= nowWall + 120000)
        && model.ticks <= MAX_TICKS
}
