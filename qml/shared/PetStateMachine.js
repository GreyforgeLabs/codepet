.pragma library
// Pure presentation state machine for the CodePet renderer (SPEC §12, Amendment A4).
// No imports, no timers, no I/O: callers pass the facts and a state's metadata (from
// PetCatalog.stateMeta) and get back which semantic state and frame to show. Node
// tests load this file directly (qml/tests/test_state_machine.mjs).
//
// Nothing here ever decides care, readiness or lifecycle outcomes; those come from the
// Rust snapshot. This only chooses a picture.

// Base-state priorities and interrupt rules (SPEC §11.2). A manifest may override
// them per state; these are the fallbacks for missing fields.
var PRIORITY = {
    idle: 0, locomotion: 20, food: 70, water: 70, play: 70, wash: 70, bathroom: 70,
    sleep_enter: 90, sleep: 85, sleep_exit: 90, celebrate: 60, needs_care: 50,
    working: 40, waiting: 35, error: 100, memorial: 0
}
var INTERRUPT = {
    idle: "any", locomotion: "any", food: "higher", water: "higher", play: "higher", wash: "higher",
    bathroom: "higher", sleep_enter: "none", sleep: "higher", sleep_exit: "none", celebrate: "higher",
    needs_care: "any", working: "any", waiting: "any", error: "higher", memorial: "any"
}
var CARE_ACTIONS = ["food", "water", "play", "wash", "bathroom"]
var WORKING_TASK_STATES = ["preparing", "running", "verifying", "cancelling"]
var WAITING_TASK_STATES = ["waiting_for_user", "review_ready"]
var TICK_MS = 200

function isCareAction(action) {
    return CARE_ACTIONS.indexOf(String(action)) >= 0
}

// "working" | "waiting" | "error" | "" for a slot task state.
function taskVisual(taskState) {
    var s = String(taskState || "")
    if (WORKING_TASK_STATES.indexOf(s) >= 0) return "working"
    if (WAITING_TASK_STATES.indexOf(s) >= 0) return "waiting"
    if (s === "recovery_required") return "error"
    return ""
}

// Facts from a snapshot (pure; tolerant of v0.1 snapshots that have no `life` key).
// `life === undefined` (legacy schema) means "the one v0.1 pet"; `life === null`
// means there is no living companion.
function facts(snap, walking) {
    var s = snap || null
    var hasLifeKey = !!s && Object.prototype.hasOwnProperty.call(s, "life")
    var life = hasLifeKey ? s.life : undefined
    var care = s && s.care ? s.care : null
    var task = s && s.active_task ? s.active_task : null
    return {
        loaded: !!s,
        hasLife: !hasLifeKey || !!life,
        sleeping: !!(care && care.sleeping),
        taskState: task ? String(task.state || "") : "",
        critical: !!(life && (life.status === "critical" || life.fainted || life.death_pending)),
        warning: !!(life && life.status === "warning"),
        bathroomDue: !!(care && care.bathroom_due),
        sleepDue: !!(care && care.sleep_due),
        walking: !!walking,
        error: false
    }
}

// SPEC §12 priority: error > sleep (when sleeping) > needs_care (critical/fainted) >
// working > waiting (recovery_required → error) > needs_care (warning, bathroom due
// or nap due, when not walking) > locomotion > idle. No life → memorial.
function baseState(i) {
    var f = i || {}
    if (f.hasLife === false) return "memorial"
    if (f.error) return "error"
    if (f.sleeping) return "sleep"
    if (f.critical) return "needs_care"
    var task = taskVisual(f.taskState)
    if (task === "working") return "working"
    if (task === "waiting") return "waiting"
    if (task === "error") return "error"
    if ((f.warning || f.bathroomDue || f.sleepDue) && !f.walking) return "needs_care"
    if (f.walking) return "locomotion"
    return "idle"
}

// Roaming is only for a calm, awake pet: never while sleeping, critical, busy with a
// task, or showing an error.
function canRoam(i) {
    var f = i || {}
    if (f.hasLife === false || f.sleeping || f.critical || f.error) return false
    return taskVisual(f.taskState) === ""
}

function priorityOf(meta) {
    if (!meta) return 0
    if (typeof meta.priority === "number") return meta.priority
    var p = PRIORITY[meta.name]
    return typeof p === "number" ? p : 0
}

function interruptOf(meta) {
    if (!meta) return "any"
    if (meta.interrupt === "any" || meta.interrupt === "higher" || meta.interrupt === "none") return meta.interrupt
    return INTERRUPT[meta.name] || "any"
}

// May `next` take over from `current`? any: always; higher: only a strictly higher
// priority; none: never (the current state completes first).
function canInterrupt(currentMeta, nextMeta) {
    if (!currentMeta) return true
    var rule = interruptOf(currentMeta)
    if (rule === "any") return true
    if (rule === "none") return false
    return priorityOf(nextMeta) > priorityOf(currentMeta)
}

// Which of a running transient and the base state is shown: "transient" or "base".
function display(baseMeta, transientMeta) {
    if (!transientMeta) return "base"
    return canInterrupt(transientMeta, baseMeta) ? "base" : "transient"
}

// A new transient arriving while another runs: "replace" it or "queue" behind it.
function admit(currentMeta, nextMeta) {
    if (!currentMeta) return "replace"
    return canInterrupt(currentMeta, nextMeta) ? "replace" : "queue"
}

function frameCount(meta) {
    if (!meta) return 1
    var n = Array.isArray(meta.frames) ? meta.frames.length : Number(meta.frames)
    return n >= 1 && isFinite(n) ? Math.floor(n) : 1
}

function frameMsOf(meta) {
    var ms = meta ? Number(meta.frameMs) : NaN
    if (!isFinite(ms) || ms < TICK_MS) return TICK_MS
    // Keep every frame boundary on the 200 ms scheduler grid (≤ 5 updates/s).
    return Math.round(ms / TICK_MS) * TICK_MS
}

function isLoop(meta) {
    return !!(meta && meta.loop)
}

// One full pass through the state's frames, in ms.
function cycleMs(meta) {
    return frameCount(meta) * frameMsOf(meta)
}

// Finite duration of a one-shot state (frames × frameMs × repeats); Infinity for a
// loop or hold.
function durationMs(meta) {
    if (!meta || isLoop(meta)) return Infinity
    var repeats = Number(meta.repeats)
    if (!isFinite(repeats) || repeats < 1) repeats = 1
    return cycleMs(meta) * Math.floor(repeats)
}

function reducedFrameOf(meta) {
    var n = frameCount(meta)
    var r = meta ? Number(meta.reducedFrame) : 0
    if (!isFinite(r) || r < 0 || r >= n) return 0
    return Math.floor(r)
}

// Frame index for `elapsedTicks` 200 ms ticks into a state. Reduced motion always
// shows the state's reducedFrame. Loops wrap; one-shot states hold their last frame.
function frameAt(meta, elapsedTicks, reducedMotion) {
    if (reducedMotion) return reducedFrameOf(meta)
    var n = frameCount(meta)
    var ticks = Math.max(0, Math.floor(Number(elapsedTicks) || 0))
    var index = Math.floor(ticks * TICK_MS / frameMsOf(meta))
    if (isLoop(meta)) return index % n
    if (meta && Number(meta.repeats) > 1) {
        var total = n * Math.floor(Number(meta.repeats))
        return index >= total ? n - 1 : index % n
    }
    return Math.min(index, n - 1)
}

// Next timer interval in ms for the single cosmetic scheduler: always a positive
// multiple of 200 so the scene never exceeds 5 updates per second.
function snapInterval(ms) {
    var v = Math.ceil(Math.max(TICK_MS, Number(ms) || TICK_MS) / TICK_MS) * TICK_MS
    return isFinite(v) ? v : TICK_MS
}

// ---- Status copy for the pet menu (plain text; calm wording, SPEC §6 copy rules). ----

var NEED_WORDS = {
    hunger: { adjective: "hungry", action: "Food", noun: "food", from: "hunger" },
    thirst: { adjective: "thirsty", action: "Water", noun: "water", from: "thirst" },
    mental_health: { adjective: "a little low", action: "Play", noun: "some play", from: "low mood" },
    hygiene: { adjective: "a bit grubby", action: "A wash", noun: "a wash", from: "grubbiness" }
}

// "about 2 h 10 min of active time"
function formatActive(ms) {
    var minutes = Math.max(1, Math.round(Number(ms) / 60000))
    if (!isFinite(minutes)) return "a while of active time"
    var h = Math.floor(minutes / 60), m = minutes % 60
    var parts = []
    if (h > 0) parts.push(h + " h")
    if (m > 0 || h === 0) parts.push(m + " min")
    return "about " + parts.join(" ") + " of active time"
}

function needOf(lifecycle, predicate) {
    var needs = lifecycle && Array.isArray(lifecycle.needs) ? lifecycle.needs : []
    for (var i = 0; i < needs.length; i++)
        if (needs[i] && predicate(needs[i])) return needs[i]
    return null
}

// One short line for the pet menu. Returns "" when the caller should show readiness.
function statusLine(snap) {
    var f = facts(snap, false)
    if (!f.loaded) return ""
    if (!f.hasLife) return "No companion right now. Adopt one whenever you're ready."
    var life = snap.life
    if (!life) return ""
    var name = String(life.name || "Your companion")
    var lc = snap.lifecycle || null
    var critical = lc && lc.critical ? lc.critical : null
    var words = critical ? NEED_WORDS[critical.need] : null
    if (life.death_pending)
        return name + " urgently needs care. Care opens again once the current task is finished; your work is kept."
    if (life.fainted)
        return name + " fainted from " + (words ? words.from : "low needs") + ". "
            + (words ? words.action : "Care") + " will revive them; work waits until then."
    if (life.status === "critical") {
        var line = name + " urgently needs " + (words ? words.noun : "care") + ". Care now keeps them safe."
        if (life.mode === "permadeath" && critical && typeof critical.remaining_ms === "number")
            line += " Without " + (words ? words.noun : "care") + ", " + name + " may pass away after "
                + formatActive(critical.remaining_ms) + "."
        return line
    }
    if (life.paused) return "Paused: time isn't counting (on a break)."
    if (life.armed === false) return "Paused: time isn't counting (not set up)."
    if (life.status === "warning") {
        var low = needOf(lc, function (n) { return n.state !== "ok" && n.lethal })
            || needOf(lc, function (n) { return n.state !== "ok" })
        var w = low ? NEED_WORDS[low.need] : null
        if (w) {
            var text = name + " is getting " + w.adjective + ". " + w.action + " will help."
            if (typeof low.until_critical_ms === "number" && low.until_critical_ms > 0)
                text += " " + formatActive(low.until_critical_ms).replace(/^about/, "About") + " before it becomes serious."
            return text
        }
        return name + " could use some care."
    }
    return ""
}
