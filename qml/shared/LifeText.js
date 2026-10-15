.pragma library

// LifeText.js: pure copy and attended-time math for the CodePet life UI (SPEC-v0.2 §2, §6, §12).
//
// No imports, no Quickshell, no I/O and no requests. The Omarchy bar plugin stages this exact
// file byte-for-byte, so it must stay a dependency-free `.pragma library`.
//
// - The engine (`advance`, `applyCare`, `status`, `windows`) is an integer-exact transcription
//   of `codepet_core::lifecycle` (spec §4.3). It is checked against the shared vectors in
//   crates/codepet-core/tests/fixtures/lifecycle_vectors.json by qml/tests/test_life_text.mjs.
//   UIs use it only for labelled predictions ("≈"); Rust stays the authority.
// - Copy follows the calm rules of D7(e): no streaks, guilt, sounds or flashing; every duration
//   says "active time"; a prediction never shows a passing.
// - Arrays are in need order [hunger, thirst, mental_health, hygiene]. Meters are hundredths
//   (10000 = full). Durations are attended (active) milliseconds.

var NEEDS = ["hunger", "thirst", "mental_health", "hygiene"]
var HOUR_MS = 3600000
var MINUTE_MS = 60000
var TICK_MS = 60000
var SETTLE_CAP_MS = 28800000
var MAX_ADVANCE_ITERATIONS = 32
var METER_MAX = 10000
var MAX_NAME_CHARS = 24
var MAX_NAME_BYTES = 96

var NEED_ACTION = { hunger: "food", thirst: "water", mental_health: "play", hygiene: "wash" }
var NEED_LABEL = { hunger: "Nourishment", thirst: "Hydration", mental_health: "Mood", hygiene: "Hygiene" }
// "Ginger is getting <feeling>."
var NEED_FEELING = { hunger: "hungry", thirst: "thirsty", mental_health: "a little low", hygiene: "scruffy" }
// "Ginger fainted from <noun>."
var NEED_NOUN = { hunger: "hunger", thirst: "thirst", mental_health: "low mood", hygiene: "grime" }
// "Ginger urgently needs <care>." / "Without <care>, ..."
var NEED_CARE = { hunger: "food", thirst: "water", mental_health: "play", hygiene: "a wash" }
// "<Help> will help."
var ACTION_HELP = { food: "Food", water: "Water", play: "Play", wash: "A wash", bathroom: "A bathroom break", routine: "A care routine" }
var DEFAULT_ACTION_LABEL = { food: "Food", water: "Water", play: "Play", wash: "Wash", bathroom: "Bathroom", routine: "Routine" }
var STAGE_KEYS = ["young", "grown", "mature"]

// Ruleset lifecycle-v1, in the shared-vector JSON shape. `lifecycle.rules` (Rust) is the
// authority; these defaults only fill copy before it has been read, and a test pins them to
// the vectors file.
var RULES_V1 = {
    steady: {
        version: "lifecycle-v1", decay_per_hour: [1000, 1250, 600, 500], full_hold_ms: 3600000,
        warning_at: 3000, lethal: ["hunger", "thirst"], critical_after_ms: 7200000, grace_ms: 21600000,
        level_xp: [0, 80, 200, 400, 700], stage_min_level: [1, 2, 4]
    },
    playtest: {
        version: "lifecycle-v1", decay_per_hour: [20000, 25000, 12000, 10000], full_hold_ms: 180000,
        warning_at: 3000, lethal: ["hunger", "thirst"], critical_after_ms: 360000, grace_ms: 1080000,
        level_xp: [0, 80, 200, 400, 700], stage_min_level: [1, 2, 4]
    }
}
// pet.configure_life bounds (crates/codepet-protocol/src/params.rs).
var MAX_GRACE_HOURS = 48
var MAX_CRITICAL_AFTER_HOURS = 24
var DECAY_PERCENT_CHOICES = [90, 75, 50, 25]

// The roster in canonical order (codepet_core::species::SPECIES_IDS). Display text only; the
// generated pet registry, when a host provides it, wins over these fallbacks. Stage names follow
// spec Amendment A4 (the art provenance names when recorded, else §11.5).
var ROSTER = [
    { id: "cat", displayName: "Cat", defaultName: "Ginger", personality: "the curious desk inspector", habitat: "floor",
      stages: ["Pounce Kit", "Desk Prowler", "Maned Inspector"] },
    { id: "dog", displayName: "Dog", defaultName: "Biscuit", personality: "the enthusiastic teammate", habitat: "floor",
      stages: ["Sunpup", "Rambler", "Hearthguard"] },
    { id: "capybara", displayName: "Capybara", defaultName: "Yuzu", personality: "the unhurried team lead", habitat: "floor",
      stages: ["Puddle Pup", "Reed Wader", "Yuzu Sage"] },
    { id: "penguin", displayName: "Penguin", defaultName: "Pip", personality: "the earnest little courier", habitat: "floor",
      stages: ["Snowpuff", "Floe Courier", "Aurora Emperor"] },
    { id: "robot", displayName: "Robot", defaultName: "Bolt", personality: "the meticulous helper", habitat: "floor",
      stages: ["Sparkbit", "Gearwright", "Prime Steward"] },
    { id: "iguana", displayName: "Iguana", defaultName: "Sol", personality: "the patient sun-seeker", habitat: "floor",
      stages: ["Sunsprout", "Crestrunner", "Sunstone Regent"] },
    { id: "spider", displayName: "Spider", defaultName: "Lace", personality: "the careful silk architect", habitat: "floor",
      stages: ["Silkling", "Threadstrider", "Starweaver"] },
    { id: "snake", displayName: "Snake", defaultName: "Ribbon", personality: "the serene ribbon", habitat: "floor",
      stages: ["Ribbonlet", "Sunweave", "Ambercrown Sage"] },
    { id: "goldfish", displayName: "Goldfish", defaultName: "Bubbles", personality: "the dreamy bubble navigator", habitat: "aquatic",
      stages: ["Bubblet", "Ripple Rover", "Veiltail Navigator"] },
    { id: "rabbit", displayName: "Rabbit", defaultName: "Clover", personality: "the alert garden scout", habitat: "floor",
      stages: ["Garden Kit", "Meadow Scout", "Warren Sentinel"] },
    { id: "turtle", displayName: "Turtle", defaultName: "Atlas", personality: "the steadfast planner", habitat: "floor",
      stages: ["Pebble", "Trailshell", "Mossback Sage"] },
    { id: "frog", displayName: "Frog", defaultName: "Hopper", personality: "the spring-loaded optimist", habitat: "floor",
      stages: ["Puddle Sprout", "Lily Leaper", "Marsh Monarch"] },
    { id: "owl", displayName: "Owl", defaultName: "Quill", personality: "the thoughtful night librarian", habitat: "floor",
      stages: ["Owlet", "Librarian", "Great Scholar"] },
    { id: "red_panda", displayName: "Red panda", defaultName: "Maple", personality: "the bashful explorer", habitat: "floor",
      stages: ["Cub", "Explorer", "Mountain Elder"] },
    { id: "axolotl", displayName: "Axolotl", defaultName: "Lotus", personality: "the cheerful tank gardener", habitat: "aquatic",
      stages: ["Larva", "Axolotl", "Grand Gardener"] }
]

// Words and phrases the life copy never uses (spec §17.3; qml/tests/copy_lint.py enforces the
// same list over every user-facing string). Matched case-insensitively at a word start.
// copy-lint: off
var BANNED = ["kill", "killed", "starv", "your fault", "abandon", "neglected you", "guilt", "punish", "streak", "hurry", "!!!"]
// copy-lint: on

// ------------------------------------------------------------------ small helpers

function isObject(v) { return v !== null && typeof v === "object" && !Array.isArray(v) }
function isNum(v) { return typeof v === "number" && isFinite(v) }
function isInt(v) { return isNum(v) && Math.floor(v) === v }
function needIndex(need) { return NEEDS.indexOf(need) }
function plural(n, one, many) { return n + " " + (n === 1 ? one : many) }
function capitalize(s) { s = String(s); return s.length > 0 ? s.charAt(0).toUpperCase() + s.slice(1) : s }
function cleanName(name) {
    var s = typeof name === "string" ? name.replace(/[\u0000-\u001f\u007f-\u009f‎‏‪-‮⁦-⁩]/g, "").trim() : ""
    return s.length > 0 ? s : "Your companion"
}

// "about 2 h 10 min of active time"; `long` spells out "hours"/"minutes" for accessible names.
// Whole minutes, rounded down, so an estimate never claims more time than the rules give.
function formatActive(ms, long) {
    if (!isNum(ms) || ms < 0)
        return ""
    var mins = Math.floor(ms / MINUTE_MS)
    if (mins < 1)
        return "less than a minute of active time"
    var h = Math.floor(mins / 60), m = mins % 60
    var parts = []
    if (h > 0) parts.push(long ? plural(h, "hour", "hours") : h + " h")
    if (m > 0) parts.push(long ? plural(m, "minute", "minutes") : m + " min")
    return "about " + parts.join(" ") + " of active time"
}

// A rules duration such as "6 h" or "18 min" (exact, for the rules card).
function formatSpan(ms) {
    if (!isNum(ms) || ms < 0)
        return ""
    var mins = Math.round(ms / MINUTE_MS)
    var h = Math.floor(mins / 60), m = mins % 60
    if (h === 0)
        return m + " min"
    return m === 0 ? h + " h" : h + " h " + m + " min"
}

function utf8Bytes(s) {
    var n = 0
    for (var i = 0; i < s.length; i++) {
        var c = s.charCodeAt(i)
        if (c < 0x80) n += 1
        else if (c < 0x800) n += 2
        else if (c >= 0xd800 && c <= 0xdbff && i + 1 < s.length) { n += 4; i++ }
        else n += 3
    }
    return n
}

// Client-side mirror of codepet_core::species::normalize_pet_name: trims, drops control and
// bidi characters, then needs 1..=24 characters and at most 96 bytes. Returns "" when invalid.
// Rust re-checks; this only keeps the Adopt button honest.
function normalizeName(raw) {
    if (typeof raw !== "string")
        return ""
    var s = raw.replace(/[\u0000-\u001f\u007f-\u009f‎‏‪-‮⁦-⁩]/g, "").trim()
    var chars = Array.from ? Array.from(s).length : s.length
    if (chars < 1 || chars > MAX_NAME_CHARS || utf8Bytes(s) > MAX_NAME_BYTES)
        return ""
    return s
}

// ------------------------------------------------------------------ roster and registry

function speciesIds() { return ROSTER.map(function (s) { return s.id }) }
function rosterEntry(id) {
    for (var i = 0; i < ROSTER.length; i++)
        if (ROSTER[i].id === id)
            return ROSTER[i]
    return null
}
function registryEntry(registry, id) {
    return isObject(registry) && isObject(registry.species) && isObject(registry.species[id]) ? registry.species[id] : null
}
function isSpecies(id) { return rosterEntry(id) !== null }
function displayName(id, registry) {
    var r = registryEntry(registry, id)
    if (r && typeof r.displayName === "string" && r.displayName.length > 0)
        return r.displayName
    var e = rosterEntry(id)
    return e ? e.displayName : "Companion"
}
function personality(id, registry) {
    var r = registryEntry(registry, id)
    if (r && typeof r.personality === "string" && r.personality.length > 0)
        return r.personality
    var e = rosterEntry(id)
    return e ? e.personality : "a CodePet companion"
}
function defaultName(id, registry) {
    var r = registryEntry(registry, id)
    if (r && typeof r.defaultName === "string" && r.defaultName.length > 0)
        return r.defaultName
    var e = rosterEntry(id)
    return e ? e.defaultName : "Companion"
}
function stageName(id, stage, registry) {
    var r = registryEntry(registry, id)
    if (r && Array.isArray(r.stages)) {
        for (var i = 0; i < r.stages.length; i++)
            if (isObject(r.stages[i]) && r.stages[i].stage === stage && typeof r.stages[i].name === "string")
                return r.stages[i].name
    }
    var e = rosterEntry(id)
    if (e && isInt(stage) && stage >= 1 && stage <= 3)
        return e.stages[stage - 1]
    return "stage " + stage
}
// Accessible name of a chooser cell, for example "Cat: the curious desk inspector".
function lookLabel(id, registry) {
    return displayName(id, registry) + ": " + personality(id, registry)
}
function careLabel(id, action, registry) {
    var r = registryEntry(registry, id)
    if (r && isObject(r.careLabels) && typeof r.careLabels[action] === "string")
        return r.careLabels[action]
    return DEFAULT_ACTION_LABEL[action] || "Care"
}

// ------------------------------------------------------------------ engine (spec §4.3)

function lethalFlags(rules) {
    var l = rules && rules.lethal
    var out = [false, false, false, false]
    if (!Array.isArray(l))
        return out
    for (var i = 0; i < l.length; i++) {
        if (typeof l[i] === "boolean" && i < 4) out[i] = l[i]
        else if (typeof l[i] === "string" && needIndex(l[i]) >= 0) out[needIndex(l[i])] = true
    }
    return out
}
function ceilDiv(a, b) { return a <= 0 ? 0 : Math.floor((a + b - 1) / b) }
function cloneVitals(v) {
    return {
        meters: v.meters.slice(), hold_ms: v.hold_ms.slice(), carry: v.carry.slice(),
        neglect_ms: v.neglect_ms.slice(),
        critical: v.critical ? { need: v.critical.need, grace_ms: v.critical.grace_ms, elapsed_ms: v.critical.elapsed_ms } : null,
        fainted: !!v.fainted
    }
}
// Fresh vitals for the given meters: every full meter starts a satiety hold.
function vitalsFromMeters(meters, rules) {
    var v = { meters: meters.slice(), hold_ms: [0, 0, 0, 0], carry: [0, 0, 0, 0], neglect_ms: [0, 0, 0, 0], critical: null, fainted: false }
    for (var i = 0; i < 4; i++)
        if (v.meters[i] >= METER_MAX) v.hold_ms[i] = rules.full_hold_ms
    return v
}
function mostNeglected(s, rules, lethal) {
    var best = -1
    for (var j = 0; j < 4; j++)
        if (lethal[j] && s.neglect_ms[j] >= rules.critical_after_ms && (best < 0 || s.neglect_ms[j] > s.neglect_ms[best]))
            best = j
    return best
}

// advance(vitals, rules, mode, deltaMs) → {vitals, events, charged, discarded}.
// Integer-exact; the step that enters critical and the step that ends a permadeath life
// discard the rest of the interval (D7a).
function advance(v, rules, mode, deltaMs) {
    var s = cloneVitals(v)
    var lethal = lethalFlags(rules)
    var rate = rules.decay_per_hour
    var events = []
    var remaining = Math.min(Math.max(isNum(deltaMs) ? deltaMs : 0, 0), SETTLE_CAP_MS)
    var charged = 0, discarded = 0, iterations = 0
    while (remaining > 0) {
        if (iterations === MAX_ADVANCE_ITERATIONS) {
            discarded += remaining
            break
        }
        iterations += 1
        var step = remaining
        var i, j
        for (i = 0; i < 4; i++) {
            if (s.hold_ms[i] > 0)
                step = Math.min(step, s.hold_ms[i])
            else if (rate[i] > 0 && s.meters[i] > 0)
                step = Math.min(step, ceilDiv(s.meters[i] * HOUR_MS - s.carry[i], rate[i]))
        }
        if (s.critical === null) {
            for (j = 0; j < 4; j++)
                if (lethal[j] && s.meters[j] === 0 && s.hold_ms[j] === 0)
                    step = Math.min(step, rules.critical_after_ms - s.neglect_ms[j])
        } else if (!s.fainted) {
            step = Math.min(step, s.critical.grace_ms - s.critical.elapsed_ms)
        }
        step = Math.max(step, 1)
        var neglecting = []
        for (j = 0; j < 4; j++)
            neglecting.push(lethal[j] && s.meters[j] === 0 && s.hold_ms[j] === 0)
        for (i = 0; i < 4; i++) {
            if (s.hold_ms[i] > 0) {
                s.hold_ms[i] -= step
            } else {
                var t = rate[i] * step + s.carry[i]
                var meter = Math.max(0, s.meters[i] - Math.floor(t / HOUR_MS))
                s.meters[i] = meter
                s.carry[i] = meter === 0 ? 0 : t % HOUR_MS
            }
        }
        for (j = 0; j < 4; j++)
            if (neglecting[j]) s.neglect_ms[j] += step
        if (s.critical !== null && !s.fainted)
            s.critical.elapsed_ms += step
        remaining -= step
        charged += step
        if (s.critical !== null && !s.fainted && s.critical.elapsed_ms >= s.critical.grace_ms) {
            if (mode === "gentle") {
                s.fainted = true
                events.push({ kind: "fainted" })
            } else {
                events.push({ kind: "died", need: s.critical.need })
                discarded += remaining
                break
            }
        }
        if (s.critical === null) {
            var k = mostNeglected(s, rules, lethal)
            if (k >= 0) {
                s.critical = { need: NEEDS[k], grace_ms: rules.grace_ms, elapsed_ms: 0 }
                events.push({ kind: "entered_critical", need: NEEDS[k] })
                discarded += remaining
                break
            }
        }
    }
    return { vitals: s, events: events, charged: charged, discarded: discarded }
}

// applyCare(vitals, rules, filled) → {vitals, events}. Matching care only; never a fresh grace.
function applyCare(v, rules, filled) {
    var s = cloneVitals(v)
    var lethal = lethalFlags(rules)
    var events = []
    var list = Array.isArray(filled) ? filled : []
    for (var f = 0; f < list.length; f++) {
        var i = needIndex(list[f])
        if (i < 0) continue
        s.meters[i] = METER_MAX
        s.hold_ms[i] = rules.full_hold_ms
        s.carry[i] = 0
        if (lethal[i]) s.neglect_ms[i] = 0
    }
    if (s.critical !== null) {
        var still = []
        for (var j = 0; j < 4; j++)
            if (lethal[j] && s.neglect_ms[j] >= rules.critical_after_ms) still.push(NEEDS[j])
        if (still.length === 0) {
            s.critical = null
            s.fainted = false
            events.push({ kind: "recovered" })
        } else if (list.indexOf(s.critical.need) >= 0) {
            s.critical.need = still[0]
        }
    }
    return { vitals: s, events: events }
}

// "healthy" | "warning" | "critical".
function status(v, rules) {
    if (v.critical !== null && v.critical !== undefined)
        return "critical"
    var lethal = lethalFlags(rules)
    for (var i = 0; i < 4; i++)
        if (v.meters[i] <= rules.warning_at || (lethal[i] && v.neglect_ms[i] > 0))
            return "warning"
    return "healthy"
}

// Per-need attended-time estimates (same meaning as codepet_core::lifecycle::windows).
function windows(v, rules, mode) {
    var lethal = lethalFlags(rules)
    var out = []
    for (var i = 0; i < 4; i++) {
        var need = NEEDS[i]
        var meter = v.meters[i]
        var hold = Math.max(0, v.hold_ms[i])
        var rate = rules.decay_per_hour[i]
        var isCriticalNeed = v.critical !== null && v.critical !== undefined && v.critical.need === need
        var state = isCriticalNeed ? "critical" : meter === 0 ? "empty" : meter <= rules.warning_at ? "low" : "ok"
        var untilWarning = meter <= rules.warning_at ? 0
            : rate === 0 ? null : hold + ceilDiv((meter - rules.warning_at) * HOUR_MS - v.carry[i], rate)
        var untilZero = meter === 0 ? hold : rate === 0 ? null : hold + ceilDiv(meter * HOUR_MS - v.carry[i], rate)
        var untilCritical = !lethal[i] ? null
            : v.neglect_ms[i] >= rules.critical_after_ms ? 0
            : untilZero === null ? null : untilZero + (rules.critical_after_ms - Math.max(0, v.neglect_ms[i]))
        var untilEnd = null
        if (mode !== "gentle" && lethal[i]) {
            if (v.critical && (isCriticalNeed || v.neglect_ms[i] >= rules.critical_after_ms))
                untilEnd = Math.max(0, v.critical.grace_ms - v.critical.elapsed_ms)
            else if (untilCritical !== null)
                untilEnd = untilCritical + rules.grace_ms
        }
        out.push({ need: need, lethal: lethal[i], state: state, until_warning_ms: untilWarning,
                   until_critical_ms: untilCritical, until_end_ms: untilEnd })
    }
    return out
}

// ------------------------------------------------------------------ snapshot helpers

function lifeOf(snap) { return isObject(snap) && isObject(snap.life) ? snap.life : null }
function lifecycleOf(snap) { return isObject(snap) && isObject(snap.lifecycle) ? snap.lifecycle : null }
// True for a v0.2 snapshot that explicitly reports "no living companion".
function hasNoLife(snap) { return isObject(snap) && ("life" in snap) && snap.life === null }
function needRow(snap, need) {
    var lc = lifecycleOf(snap)
    var list = lc && Array.isArray(lc.needs) ? lc.needs : []
    for (var i = 0; i < list.length; i++)
        if (isObject(list[i]) && list[i].need === need)
            return list[i]
    return null
}
function rulesOf(snap, pace) {
    var lc = lifecycleOf(snap)
    if (lc && isObject(lc.rules) && Array.isArray(lc.rules.decay_per_hour))
        return lc.rules
    return pace === "playtest" ? RULES_V1.playtest : RULES_V1.steady
}
// Reads the steady/playtest presets out of a `lifecycle.rules` result. Accepts the canonical
// `{steady, playtest}` object at the top level or under `rules`; falls back to RULES_V1.
function presetsFrom(result) {
    var src = isObject(result) && isObject(result.rules) && isObject(result.rules.steady) ? result.rules : result
    var ok = function (r) { return isObject(r) && Array.isArray(r.decay_per_hour) && r.decay_per_hour.length === 4 && isInt(r.grace_ms) && isInt(r.critical_after_ms) }
    return {
        steady: isObject(src) && ok(src.steady) ? src.steady : RULES_V1.steady,
        playtest: isObject(src) && ok(src.playtest) ? src.playtest : RULES_V1.playtest
    }
}

// Vitals from a committed snapshot's lifecycle block (carry is not exported, so 0: a prediction
// may run up to one hundredth slow, never fast enough to invent a passing).
function vitalsFromSnapshot(snap) {
    var lc = lifecycleOf(snap)
    if (!lc || !Array.isArray(lc.needs))
        return null
    var v = { meters: [0, 0, 0, 0], hold_ms: [0, 0, 0, 0], carry: [0, 0, 0, 0], neglect_ms: [0, 0, 0, 0], critical: null, fainted: false }
    for (var i = 0; i < 4; i++) {
        var row = needRow(snap, NEEDS[i])
        if (!row || !isInt(row.meter))
            return null
        v.meters[i] = Math.min(METER_MAX, Math.max(0, row.meter))
        v.hold_ms[i] = isInt(row.hold_ms) ? Math.max(0, row.hold_ms) : 0
        v.neglect_ms[i] = isInt(row.neglect_ms) ? Math.max(0, row.neglect_ms) : 0
    }
    if (isObject(lc.critical) && needIndex(lc.critical.need) >= 0 && isInt(lc.critical.grace_ms) && isInt(lc.critical.elapsed_ms))
        v.critical = { need: lc.critical.need, grace_ms: lc.critical.grace_ms, elapsed_ms: lc.critical.elapsed_ms }
    var life = lifeOf(snap)
    v.fainted = !!(life && life.fainted)
    return v
}

// ------------------------------------------------------------------ copy templates (spec §6)

function modeLabel(mode) { return mode === "gentle" ? "Gentle" : "Permadeath" }
function paceLabel(pace) { return pace === "playtest" ? "Playtest" : "Steady" }

function warningText(name, need, untilCriticalMs) {
    var n = cleanName(name)
    var action = NEED_ACTION[need] || "food"
    var feeling = NEED_FEELING[need] || "a little low"
    var verb = need === "mental_health" ? "is feeling " : "is getting "
    var text = n + " " + verb + feeling + ". " + (ACTION_HELP[action] || "Care") + " will help."
    if (isNum(untilCriticalMs) && untilCriticalMs > 0)
        text += " " + capitalize(formatActive(untilCriticalMs)) + " before it becomes serious."
    else if (isNum(untilCriticalMs) && untilCriticalMs === 0)
        text += " Please care for them soon."
    return text
}

function criticalText(name, need, remainingMs, mode) {
    var n = cleanName(name)
    var care = NEED_CARE[need] || "care"
    var text = n + " urgently needs " + care + ". Care now keeps them safe."
    if (!isNum(remainingMs) || remainingMs < 0)
        return text
    if (mode === "gentle")
        return text + " Without " + care + ", " + n + " may faint after " + formatActive(remainingMs)
            + "; in gentle mode that is never permanent."
    return text + " Without " + care + ", " + n + " may pass away after " + formatActive(remainingMs) + "."
}

function faintedText(name, need) {
    var n = cleanName(name)
    var action = NEED_ACTION[need] || "food"
    return n + " fainted from " + (NEED_NOUN[need] || "tiredness") + ". " + (ACTION_HELP[action] || "Care")
        + " will revive them; work waits until then."
}

function pendingText(name) {
    var n = cleanName(name)
    return n + "'s grace time has run out. They will pass away when the current task is finished. "
        + "The task and its result are kept safe: review, accept, discard or cancel it as usual. New work waits until then."
}

var FROZEN_REASON = {
    paused: "on a break",
    task: "task running",
    unarmed: "not set up",
    sleeping: "napping",
    no_life: "no companion yet",
    pending: "task finishing"
}
function frozenText(reason) {
    if (typeof reason !== "string" || reason === "" || reason === "none")
        return ""
    return "Paused: time isn't counting (" + (FROZEN_REASON[reason] || "not counting right now") + ")."
}
var COUNTING_TEXT = "Needs only change during active time: while CodePet is on your Omarchy bar and you are at the computer."

function memorialStory(summary, registry) {
    if (!isObject(summary))
        return ""
    var n = cleanName(summary.name)
    var level = isInt(summary.final_level) ? summary.final_level : 1
    var species = typeof summary.appearance === "string" ? summary.appearance : summary.species
    var stage = isInt(summary.final_stage) ? summary.final_stage : 1
    var tasks = isInt(summary.tasks_accepted) ? summary.tasks_accepted : 0
    return "They reached level " + level + " as a " + stageName(species, stage, registry)
        + " and helped with " + plural(tasks, "accepted task", "accepted tasks") + "."
}
var AFTER_TEXT = "Your work, tasks and history are unchanged. You can adopt a new companion whenever you're ready."

// The memorial line for a dead or retired life (spec §6 death template).
function endText(summary, registry) {
    if (!isObject(summary))
        return ""
    var n = cleanName(summary.name)
    var retired = summary.end_cause === "retired"
    return n + (retired ? " has retired. " : " has passed away. ") + memorialStory(summary, registry) + " " + AFTER_TEXT
}

// "Ginger · Level 2 · Desk Prowler"
function lifeHeadline(life, registry) {
    if (!isObject(life))
        return ""
    return cleanName(life.name) + " · Level " + (isInt(life.level) ? life.level : 1) + " · "
        + stageName(life.appearance || life.species, isInt(life.stage) ? life.stage : 1, registry)
}
// "Grows into a Maned Inspector at level 4 (280 XP to go)", or "" at the final stage.
function nextStageText(life, registry) {
    if (!isObject(life) || !isObject(life.next_stage))
        return isObject(life) ? "Fully grown. Every look stays available." : ""
    var ns = life.next_stage
    var togo = isInt(ns.xp) && isInt(life.xp) ? Math.max(0, ns.xp - life.xp) : null
    return "Grows into a " + stageName(life.appearance || life.species, ns.stage, registry) + " at level " + ns.min_level
        + (togo !== null ? " (" + togo + " XP to go)" : "")
}

// The single most important need row: critical, then empty, then low; ties by shortest window.
function worstNeed(snap) {
    var lc = lifecycleOf(snap)
    var list = lc && Array.isArray(lc.needs) ? lc.needs : []
    var rank = { critical: 3, empty: 2, low: 1 }
    var best = null
    for (var i = 0; i < list.length; i++) {
        var n = list[i]
        if (!isObject(n) || !rank[n.state])
            continue
        var w = isNum(n.until_critical_ms) ? n.until_critical_ms : Infinity
        if (best === null || rank[n.state] > rank[best.state]
                || (rank[n.state] === rank[best.state] && w < (isNum(best.until_critical_ms) ? best.until_critical_ms : Infinity)))
            best = n
    }
    return best
}

// One calm status sentence for the committed snapshot (never a prediction).
function statusLine(snap, registry) {
    if (!isObject(snap))
        return ""
    if (hasNoLife(snap)) {
        var m = isObject(snap.memorial) ? snap.memorial.latest : null
        return isObject(m) ? endText(m, registry) : "No companion yet. Adopt one whenever you like; it starts at level 1."
    }
    var life = lifeOf(snap)
    if (!life)
        return ""
    var name = life.name
    var lc = lifecycleOf(snap)
    if (life.death_pending)
        return pendingText(name)
    if (!life.armed)
        return cleanName(name) + " is here with you. Set up life rules to start the active-time clock; until then nothing changes on its own."
    var crit = lc && isObject(lc.critical) ? lc.critical : null
    if (life.fainted && crit)
        return faintedText(name, crit.need)
    if (crit || life.status === "critical") {
        var need = crit ? crit.need : (worstNeed(snap) || {}).need
        return criticalText(name, need, crit && isNum(crit.remaining_ms) ? crit.remaining_ms : null, life.mode)
    }
    var w = worstNeed(snap)
    if (w)
        return warningText(name, w.need, w.lethal === false ? null : w.until_critical_ms)
    return cleanName(name) + " is doing well."
}

// Per-need row text for the life panel: "Hydration 28% · low · water · about 4 h 24 min of active time before it becomes serious".
function needLine(row) {
    if (!isObject(row) || needIndex(row.need) < 0)
        return ""
    var label = NEED_LABEL[row.need]
    var pct = isInt(row.meter) ? Math.round(row.meter / 100) + "%" : "?"
    var stateWord = { ok: "fine", low: "low", empty: "empty", critical: "urgent" }[row.state] || "unknown"
    var text = label + " " + pct + " · " + stateWord + " · " + (row.action || NEED_ACTION[row.need])
    if (row.state === "ok" && isNum(row.until_warning_ms))
        text += " · low in " + formatActive(row.until_warning_ms)
    else if (row.lethal && row.state !== "critical" && isNum(row.until_critical_ms) && row.until_critical_ms > 0)
        text += " · " + formatActive(row.until_critical_ms) + " before it becomes serious"
    return text
}

// ------------------------------------------------------------------ predictions (never a passing)

var PREDICT_URGENT_TEXT = " may need you urgently. Open CodePet to check."

// Extrapolates committed vitals by attended time not yet settled. Returns
// {severity: "none"|"warning"|"critical", need, text, approx: true}. A predicted passing is shown
// only as urgent ("may need you urgently"); a prediction never states a death.
function predict(snap, attendedSinceMs, registry) {
    var life = lifeOf(snap)
    var none = { severity: "none", need: "", text: "", approx: true }
    if (!life || !life.armed || life.death_pending || !isNum(attendedSinceMs) || attendedSinceMs <= 0)
        return none
    var lc = lifecycleOf(snap)
    if (lc && typeof lc.frozen_reason === "string" && lc.frozen_reason !== "none" && lc.frozen_reason !== "")
        return none
    var v = vitalsFromSnapshot(snap)
    if (!v)
        return none
    var rules = rulesOf(snap, life.pace)
    var a = advance(v, rules, life.mode, attendedSinceMs)
    var name = cleanName(life.name)
    for (var e = 0; e < a.events.length; e++) {
        if (a.events[e].kind === "died")
            return { severity: "critical", need: a.events[e].need, text: "≈ " + name + PREDICT_URGENT_TEXT, approx: true }
    }
    var st = status(a.vitals, rules)
    if (st === "critical") {
        if (a.vitals.fainted)
            return { severity: "critical", need: a.vitals.critical.need, text: "≈ " + name + PREDICT_URGENT_TEXT, approx: true }
        return { severity: "critical", need: a.vitals.critical.need,
                 text: "≈ " + name + " urgently needs " + NEED_CARE[a.vitals.critical.need] + ". Care now keeps them safe.", approx: true }
    }
    if (st === "warning") {
        var w = windows(a.vitals, rules, life.mode)
        var pick = null
        for (var i = 0; i < 4; i++)
            if (w[i].state !== "ok" && (pick === null || (w[i].lethal && !pick.lethal)))
                pick = w[i]
        var need = pick ? pick.need : "hunger"
        return { severity: "warning", need: need, text: "≈ " + warningText(name, need, null), approx: true }
    }
    return none
}

// ------------------------------------------------------------------ rules card / adoption copy

// The full rules, as plain lines, with exact numbers from `rules` (one preset).
function rulesLines(rules, mode, name) {
    var r = rules || RULES_V1.steady
    var n = cleanName(name)
    var full = vitalsFromMeters([METER_MAX, METER_MAX, METER_MAX, METER_MAX], r)
    var w = windows(full, r, "permadeath")
    var firstLow = null, firstEnd = null
    for (var i = 0; i < 4; i++) {
        if (w[i].until_warning_ms !== null && (firstLow === null || w[i].until_warning_ms < firstLow)) firstLow = w[i].until_warning_ms
        if (w[i].until_end_ms !== null && (firstEnd === null || w[i].until_end_ms < firstEnd)) firstEnd = w[i].until_end_ms
    }
    var lethal = lethalFlags(r)
    var lethalNames = []
    for (var j = 0; j < 4; j++) if (lethal[j]) lethalNames.push(NEED_LABEL[NEEDS[j]])
    var lines = [
        "Only active time counts: while CodePet is on your Omarchy bar and you are at the computer. Breaks, sleep, travel, nights away and running tasks never count.",
        "Needs slowly drop during active time. Food, water, play and a wash fill them back to 100.",
        "A need at " + Math.round(r.warning_at / 100) + "% or less shows a gentle warning on the bar and here.",
        lethalNames.join(" and ") + " are the only needs that matter for safety. Mood and hygiene only show warnings.",
        "If " + lethalNames.join(" or ") + " stays empty for " + formatSpan(r.critical_after_ms) + " of active time, " + n + " becomes critical and you see it right away.",
        mode === "gentle"
            ? "Gentle: after " + formatSpan(r.grace_ms) + " more active time while critical, " + n + " faints and work waits until you care for them. They never pass away."
            : "Permadeath: after " + formatSpan(r.grace_ms) + " more active time while critical, " + n + " passes away. Their memorial keeps their story; your work, tasks and history are never touched.",
        "From full, the first gentle warning comes after " + formatActive(firstLow)
            + (mode === "gentle" ? "." : ", and the earliest possible passing after " + formatActive(firstEnd) + " with every warning ignored."),
        "Pause/Travel stops the clock whenever you like, even when a need is urgent. Care always helps right away.",
        "There are no sounds, notifications or daily targets. Level and looks never change what CodePet is allowed to do."
    ]
    return lines
}

function modeExplain(mode, rules) {
    var r = rules || RULES_V1.steady
    if (mode === "gentle")
        return "Gentle: when a need stays urgent for " + formatSpan(r.grace_ms) + " of active time, your companion faints until you care for them. They never pass away."
    return "Permadeath (preselected): when a need stays urgent for " + formatSpan(r.grace_ms) + " of active time, your companion passes away and is remembered in the memorial. Your work is never touched."
}
function paceExplain(pace, rules) {
    var r = rules || (pace === "playtest" ? RULES_V1.playtest : RULES_V1.steady)
    var full = vitalsFromMeters([METER_MAX, METER_MAX, METER_MAX, METER_MAX], r)
    var w = windows(full, r, "permadeath")
    var end = null
    for (var i = 0; i < 4; i++) if (w[i].until_end_ms !== null && (end === null || w[i].until_end_ms < end)) end = w[i].until_end_ms
    if (pace === "playtest")
        return "Playtest: about 20 times faster, for trying the whole cycle in under an hour (" + formatActive(end) + " from full with no care). Shown with a Playtest badge."
    return "Steady (default): " + formatActive(end) + " from full with every warning ignored."
}

// Values offered by "More lenient settings": strictly longer windows or slower decay only.
function graceHourChoices(currentGraceMs) {
    var out = []
    for (var h = 1; h <= MAX_GRACE_HOURS; h++)
        if (h * HOUR_MS > currentGraceMs) out.push(h)
    return out
}
function criticalAfterHourChoices(currentCriticalAfterMs) {
    var out = []
    for (var h = 1; h <= MAX_CRITICAL_AFTER_HOURS; h++)
        if (h * HOUR_MS > currentCriticalAfterMs) out.push(h)
    return out
}
function decayPercentChoices() { return DECAY_PERCENT_CHOICES.slice() }

// ------------------------------------------------------------------ banned copy

// Returns the first banned term found in `text`, or "" when the text is calm.
function bannedIn(text) {
    if (typeof text !== "string")
        return ""
    var lower = text.toLowerCase()
    for (var i = 0; i < BANNED.length; i++) {
        var term = BANNED[i]
        if (/^!+$/.test(term)) {
            if (lower.indexOf(term) >= 0) return term
            continue
        }
        var re = new RegExp("(^|[^a-z])" + term.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))
        if (re.test(lower))
            return term
    }
    return ""
}
