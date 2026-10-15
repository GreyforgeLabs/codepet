.pragma library
.import "../shared/LifeText.js" as LifeText

// Fixed preview snapshots for PreviewHost.qml (SPEC-v0.2 §12). They are presentation data
// only: nothing here is saved, rewarded or sent anywhere. The lifecycle blocks are computed
// with LifeText.js (the vectors-tested transcription of the Rust engine) so the copy and the
// numbers agree with what a real `state.get` would show.

var LIFE_ID = "life-0123456789abcdef01234567"
var BASE_MS = 1760400000000

var NAMES = ["idle", "ready", "bathroom_due", "sleep_due", "sleeping", "demo_level_2", "running",
             "review_not_verified", "review_check_failed", "recovery_required",
             "unarmed", "warning_thirst", "critical_hunger", "gentle_fainted", "death_pending_review",
             "no_life_memorial", "evolved_unseen", "paused", "aquatic_goldfish", "robot_working"]

var PROVIDERS = [
    { "id": "mock", "label": "Mock", "available": true, "note": "Simulation only; runs no command" },
    { "id": "codex", "label": "Codex", "available": false, "note": "Not yet supported on this version" },
    { "id": "opencode", "label": "OpenCode", "available": false, "note": "Not yet supported on this version" },
    { "id": "hermes", "label": "Hermes", "available": false, "note": "Not yet supported on this version" },
    { "id": "claude_code", "label": "Claude Code", "available": false, "note": "Not yet supported on this version" }
]

// Ended lives shown by the memorial fixtures, newest ordinal first.
var MEMORIAL = [
    { "life_id": "life-00000000000000000000000c", "ordinal": 3, "name": "Biscuit", "species": "dog", "appearance": "dog",
      "mode": "permadeath", "pace": "steady", "final_level": 2, "final_stage": 2, "stage_name_key": "grown",
      "life_xp": 120, "tasks_accepted": 6, "adopted_at_ms": BASE_MS - 9 * 86400000, "ended_at_ms": BASE_MS - 86400000,
      "end_cause": "neglect:thirst", "final_meters": { "hunger": 1200, "thirst": 0, "mental_health": 4000, "hygiene": 5000 },
      "attended_ms": 61200000 },
    { "life_id": "life-00000000000000000000000b", "ordinal": 2, "name": "Pip", "species": "penguin", "appearance": "owl",
      "mode": "permadeath", "pace": "playtest", "final_level": 1, "final_stage": 1, "stage_name_key": "young",
      "life_xp": 0, "tasks_accepted": 0, "adopted_at_ms": BASE_MS - 12 * 86400000, "ended_at_ms": BASE_MS - 11 * 86400000,
      "end_cause": "retired", "final_meters": { "hunger": 9000, "thirst": 9000, "mental_health": 9000, "hygiene": 9000 },
      "attended_ms": 1200000 },
    { "life_id": "life-00000000000000000000000a", "ordinal": 1, "name": "Ginger", "species": "cat", "appearance": "cat",
      "mode": "permadeath", "pace": "steady", "final_level": 4, "final_stage": 3, "stage_name_key": "mature",
      "life_xp": 420, "tasks_accepted": 21, "adopted_at_ms": BASE_MS - 40 * 86400000, "ended_at_ms": BASE_MS - 13 * 86400000,
      "end_cause": "retired", "final_meters": { "hunger": 10000, "thirst": 10000, "mental_health": 10000, "hygiene": 10000 },
      "attended_ms": 180000000 }
]

function summary(id, state, outcome, verification) {
    var s = {
        "task_id": id, "revision": 2, "state": state,
        "category": "mental_health", "provider": "mock",
        "folder": "/preview/example-project", "scope": ["docs"],
        "instructions": "Preview fixture: write a short plan for the parser refactor.",
        "mock": { "outcome": outcome, "step_ms": 0 },
        "created_at_ms": BASE_MS, "updated_at_ms": BASE_MS + 60000,
        "review": null, "receipt": null
    }
    if (verification !== "")
        s.review = {
            "digest": "preview0000000000000000000000000000000000000000000000000000000000",
            "verification": verification,
            "metadata_repaired": false,
            "artifact": "PREVIEW FIXTURE ARTIFACT\n<b>Markup stays literal</b>\nNo command ran; this text is static.",
            "simulated": true
        }
    return s
}

// state.get lists briefs (docs/milestone-1.md): no folder, scope or artifact text.
function brief(s) {
    if (!s)
        return null
    return {
        "task_id": s.task_id, "revision": s.revision, "state": s.state,
        "category": s.category, "provider": s.provider, "mock": s.mock,
        "instructions_excerpt": s.instructions, "instructions_bytes": s.instructions.length,
        "instructions_sha256": "(preview: not computed)",
        "created_at_ms": s.created_at_ms, "updated_at_ms": s.updated_at_ms,
        "review": s.review ? { "digest": s.review.digest, "verification": s.review.verification,
                               "metadata_repaired": s.review.metadata_repaired,
                               "simulated": s.review.simulated, "artifact_bytes": s.review.artifact.length } : null,
        "receipt": s.receipt
    }
}

function activeFor(name) {
    if (name === "running" || name === "robot_working")
        return summary("preview-running", "running", "artifact", "")
    if (name === "review_not_verified" || name === "death_pending_review")
        return summary("preview-review", "review_ready", "artifact", "not_verified")
    if (name === "review_check_failed")
        return summary("preview-review", "review_ready", "check_failed", "failed")
    if (name === "recovery_required")
        return summary("preview-recovery", "recovery_required", "artifact", "not_verified")
    return null
}

function fullTasks(name) {
    var list = [summary("preview-failed", "failed", "provider_failure", ""),
                summary("preview-draft", "draft", "artifact", "")]
    var active = activeFor(name)
    if (active)
        list.push(active)
    return list
}

// The life shown by each fixture: {life fields, meters, vitals overrides, lifecycle extras}.
function lifeSpec(name) {
    var spec = {
        life: { "name": "Ginger", "species": "cat", "appearance": "cat", "origin": "migrated", "mode": "permadeath",
                "pace": "steady", "armed": true, "ordinal": 4, "revision": 3 },
        meters: null, neglect: [0, 0, 0, 0], critical: null, fainted: false, pending: false, unseen: [], xp: 0
    }
    switch (name) {
    case "unarmed":
        spec.life.armed = false
        spec.life.ordinal = 1
        spec.life.origin = "migrated"
        break
    case "warning_thirst":
        spec.meters = [6100, 2800, 8000, 9000]
        break
    case "critical_hunger":
        spec.meters = [0, 10000, 7000, 6000]
        spec.neglect = [7200000, 0, 0, 0]
        spec.critical = { "need": "hunger", "grace_ms": 21600000, "elapsed_ms": 3600000 }
        break
    case "gentle_fainted":
        spec.life.mode = "gentle"
        spec.meters = [5000, 0, 6000, 6000]
        spec.neglect = [0, 28800000, 0, 0]
        spec.critical = { "need": "thirst", "grace_ms": 21600000, "elapsed_ms": 21600000 }
        spec.fainted = true
        break
    case "death_pending_review":
        spec.meters = [3000, 0, 6000, 6000]
        spec.neglect = [0, 28800000, 0, 0]
        spec.critical = { "need": "thirst", "grace_ms": 21600000, "elapsed_ms": 21600000 }
        spec.pending = true
        break
    case "no_life_memorial":
        return null
    case "evolved_unseen":
        spec.xp = 80
        spec.unseen = [{ "seq": 88, "kind": "evolved", "stage": 2, "at_ms": BASE_MS }]
        break
    case "demo_level_2":
        spec.xp = 80
        break
    case "aquatic_goldfish":
        spec.life.name = "Bubbles"
        spec.life.species = "goldfish"
        spec.life.appearance = "goldfish"
        spec.life.origin = "adopted"
        spec.xp = 420
        break
    case "robot_working":
        spec.life.name = "Bolt"
        spec.life.species = "cat"
        spec.life.appearance = "robot"
        spec.xp = 200
        break
    }
    return spec
}

function levelFor(xp, rules) {
    var level = 1
    for (var i = 0; i < rules.level_xp.length; i++)
        if (xp >= rules.level_xp[i]) level = i + 1
    return level
}
function stageFor(level, rules) {
    var stage = 1
    for (var i = 0; i < rules.stage_min_level.length; i++)
        if (level >= rules.stage_min_level[i]) stage = i + 1
    return stage
}

function lifeBlocks(name, meterList, decayPaused, activeTask) {
    var spec = lifeSpec(name)
    var rules = LifeText.RULES_V1.steady
    var mem = name === "no_life_memorial" ? { "count": MEMORIAL.length, "latest": MEMORIAL[0] }
                                          : { "count": 0, "latest": null }
    if (spec === null)
        return { "life": null, "memorial": mem, "unseen_events": [],
                 "lifecycle": { "ruleset": "lifecycle-v1", "frozen_reason": "no_life", "attended_total_ms": 0,
                                "needs": [], "critical": null, "rules": lifecycleRules(rules) } }
    var meters = spec.meters || meterList
    var v = LifeText.vitalsFromMeters(meters, rules)
    v.neglect_ms = spec.neglect.slice()
    v.critical = spec.critical
    v.fainted = spec.fainted
    var w = LifeText.windows(v, rules, spec.life.mode)
    var needs = []
    for (var i = 0; i < 4; i++) {
        var n = w[i]
        needs.push({ "need": n.need, "action": LifeText.NEED_ACTION[n.need], "lethal": n.lethal, "meter": v.meters[i],
                     "hold_ms": v.hold_ms[i], "neglect_ms": v.neglect_ms[i], "state": n.state,
                     "until_warning_ms": n.until_warning_ms, "until_critical_ms": n.until_critical_ms,
                     "until_end_ms": spec.life.armed ? n.until_end_ms : null })
    }
    var level = levelFor(spec.xp, rules)
    var stage = stageFor(level, rules)
    var nextLevelXp = level < rules.level_xp.length ? rules.level_xp[level] : null
    var nextStage = null
    if (stage < 3) {
        var minLevel = rules.stage_min_level[stage]
        nextStage = { "stage": stage + 1, "min_level": minLevel, "xp": rules.level_xp[minLevel - 1] }
    }
    var paused = !!decayPaused || name === "paused"
    var slotTask = activeTask !== null
    var frozen = !spec.life.armed ? "unarmed" : spec.pending ? "pending" : paused ? "paused" : slotTask ? "task"
        : name === "sleeping" ? "sleeping" : "none"
    var status = LifeText.status(v, rules)
    var life = {
        "life_id": LIFE_ID, "ordinal": spec.life.ordinal, "name": spec.life.name, "species": spec.life.species,
        "appearance": spec.life.appearance, "origin": spec.life.origin, "status": status, "mode": spec.life.mode,
        "armed": spec.life.armed, "pace": spec.life.pace, "paused": paused, "fainted": spec.fainted,
        "death_pending": spec.pending, "revision": spec.life.revision, "adopted_at_ms": BASE_MS - 86400000,
        "xp": spec.xp, "level": level, "next_level_xp": nextLevelXp, "stage": stage,
        "stage_key": LifeText.STAGE_KEYS[stage - 1], "next_stage": nextStage, "tasks_accepted": Math.floor(spec.xp / 20)
    }
    var critical = null
    if (v.critical)
        critical = { "need": v.critical.need, "action": LifeText.NEED_ACTION[v.critical.need], "grace_ms": v.critical.grace_ms,
                     "elapsed_ms": v.critical.elapsed_ms,
                     "remaining_ms": Math.max(0, v.critical.grace_ms - v.critical.elapsed_ms), "since_ms": BASE_MS - 3600000 }
    return {
        "life": life,
        "memorial": mem,
        "unseen_events": spec.unseen,
        "lifecycle": { "ruleset": "lifecycle-v1", "frozen_reason": frozen, "attended_total_ms": 5400000,
                       "needs": needs, "critical": critical, "rules": lifecycleRules(rules) }
    }
}

function lifecycleRules(r) {
    return { "decay_per_hour": r.decay_per_hour.slice(), "full_hold_ms": r.full_hold_ms, "warning_at": r.warning_at,
             "lethal": r.lethal.slice(), "critical_after_ms": r.critical_after_ms, "grace_ms": r.grace_ms,
             "tick_ms": 60000, "idle_timeout_s": 180, "settle_cap_ms": 28800000 }
}

// `lifecycle.rules` result: the canonical presets (preview copy of RULES_V1).
function rulesResult() {
    return { "ruleset": "lifecycle-v1", "steady": LifeText.RULES_V1.steady, "playtest": LifeText.RULES_V1.playtest }
}

// One `pet.memorial` page for the named fixture.
function memorialPage(name, params) {
    var list = name === "no_life_memorial" ? MEMORIAL : []
    var limit = params && typeof params.limit === "number" ? params.limit : 20
    var before = params && typeof params.before_ordinal === "number" ? params.before_ordinal : null
    var entries = list.filter(function (e) { return before === null || e.ordinal < before }).slice(0, limit)
    var last = entries.length > 0 ? entries[entries.length - 1].ordinal : null
    var more = last !== null && list.some(function (e) { return e.ordinal < last })
    return { "entries": entries, "next_before_ordinal": more ? last : null }
}

function snapshot(name, now) {
    var active = brief(activeFor(name))
    var recent = fullTasks(name).map(brief).reverse()
    var ready = name === "ready" || name === "demo_level_2" || name === "evolved_unseen" || name === "aquatic_goldfish"
    var sleeping = name === "sleeping"
    var due = name === "sleep_due" || sleeping
    var bathroom = name === "bathroom_due"
    var meter = ready || due || bathroom ? 10000 : 7000
    var blocks = lifeBlocks(name, [meter, meter, meter, meter], false, active)
    var meters = blocks.life ? blocks.lifecycle.needs.map(function (n) { return n.meter }) : [meter, meter, meter, meter]
    var reasons = []
    if (blocks.life === null)
        reasons.push("Adopt a companion to start work")
    else if (blocks.life.death_pending)
        reasons.push("Finish or resolve the active task first")
    else if (blocks.life.status === "critical") {
        var crit = blocks.lifecycle.critical
        reasons.push(blocks.life.name + " urgently needs " + LifeText.NEED_CARE[crit.need] + "; care before starting work")
    }
    var full = meters.every(function (m) { return m === 10000 })
    if (!full && blocks.life !== null && reasons.length === 0) reasons.push("Needs food, water, play and a wash")
    if (bathroom) reasons.push("Needs a bathroom break at the tree")
    if (due) reasons.push(sleeping ? "Resting. Wake or refresh after the nap" : "Needs a two-minute nap")
    if (active && reasons.indexOf("Finish or resolve the active task first") < 0) reasons.push("Finish or resolve the active task first")
    var demoXp = blocks.life ? blocks.life.xp : 0
    var demoLevel = blocks.life ? blocks.life.level : 1
    return {
        "schema_version": 4, "state_version": 812, "protocol_version": 1, "last_seq": 3,
        "meters": { "hunger": meters[0], "thirst": meters[1], "mental_health": meters[2], "hygiene": meters[3] },
        "care": { "revision": 1, "bathroom_due": bathroom, "starts_since_bathroom": bathroom ? 2 : 0,
            "sleeping": sleeping, "sleep_due": due, "sleep_remaining_ms": sleeping ? 92000 : 120000,
            "sleep_started_at_ms": sleeping ? now - 28000 : null,
            "work_sessions": due ? 3 : 0, "execution_ms": 0,
            "ready_to_work": ready && !active && reasons.length === 0, "readiness_reasons": reasons,
            "welcome_care_available": name === "idle" },
        "progression": { "real": { "lifetime_xp": 0, "level": 1, "next_level_xp": 80,
            "mastery": { "bugfix": 0, "quality_performance": 0, "learning_specification": 0, "cleanup": 0 } },
            "demo": { "lifetime_xp": demoXp, "level": demoLevel,
                "next_level_xp": demoLevel < 5 ? LifeText.RULES_V1.steady.level_xp[demoLevel] : null,
                "mastery": { "bugfix": 0, "quality_performance": 0, "learning_specification": demoXp, "cleanup": 0 } } },
        "decay": { "enabled": false, "paused": name === "paused" },
        "settings": { "revision": 1, "decay_enabled": false, "decay_per_hour": [100, 100, 50, 50], "timeout_seconds": 1800 },
        "active_task": active,
        "recent": recent,
        "providers": PROVIDERS,
        "life": blocks.life,
        "lifecycle": blocks.lifecycle,
        "unseen_events": blocks.unseen_events,
        "memorial": blocks.memorial
    }
}
