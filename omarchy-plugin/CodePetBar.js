.pragma library
// CodePet bar plugin model: pure functions, no QML or Quickshell imports, node-tested
// (qml/tests/test_bar_model.mjs). Everything the bar shows is derived here from the
// bar-safe `bar-state.json` subset that Rust writes (schema codepet.bar.v1).
//
// Copy rules: calm text, every duration labelled "active time", no countdown to a death
// in any prediction. Colour is never the only cue: every non-healthy status has a glyph.

var SCHEMA = "codepet.bar.v1"
var MAX_STATE_BYTES = 16384
var MAX_SETTINGS_BYTES = 1048576
var MAX_REGISTRY_BYTES = 262144

// The only methods the bar may send. Task methods are never sent from the shell
// process: a mock supervisor must never become a child of the Omarchy shell.
var ALLOWED_METHODS = ["state.get", "pet.care", "pet.sleep", "pet.wake", "pet.pause", "pet.events_seen"]
var CARE_ACTIONS = ["food", "water", "play", "wash", "bathroom", "routine"]

var NEEDS = ["hunger", "thirst", "mental_health", "hygiene"]
var NEED_ACTION = { hunger: "food", thirst: "water", mental_health: "play", hygiene: "wash" }
var NEED_LABEL = { hunger: "Nourishment", thirst: "Hydration", mental_health: "Mood", hygiene: "Hygiene" }
var NEED_FEELING = { hunger: "hungry", thirst: "thirsty", mental_health: "restless", hygiene: "scruffy" }
var NEED_NOUN = { hunger: "hunger", thirst: "thirst", mental_health: "low mood", hygiene: "grime" }
var ACTION_WORD = { food: "Food", water: "Water", play: "Play", wash: "A wash", bathroom: "A bathroom break", routine: "A care routine" }
var DEFAULT_CARE_LABEL = { food: "Food", water: "Water", play: "Play", wash: "Wash", bathroom: "Bathroom", routine: "Routine", sleep: "Bed" }
var DEFAULT_STAGE_MIN_LEVEL = [1, 2, 4]

var STATUS_GLYPH = {
    missing: "", healthy: "",
    no_life: "+", death_pending: "✿", critical: "!!", fainted: "!!",
    unarmed: "◦", warning: "!", review: "?", sleeping: "z",
    paused: "‖", working: "•", evolved: "★"
}
// Display precedence, most important first.
var STATUS_ORDER = ["missing", "no_life", "death_pending", "fainted", "critical", "unarmed", "warning",
                    "review", "sleeping", "paused", "working", "evolved", "healthy"]
var SEVERITY_RANK = { none: 0, warning: 1, critical: 2 }
var WORKING_STATES = ["preparing", "running", "verifying", "cancelling"]
var REVIEW_STATES = ["waiting_for_user", "review_ready", "recovery_required"]

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

// bar-state.json: at most 16 KiB and schema codepet.bar.v1, otherwise null.
function parse(text) {
    var doc = boundedJson(text, MAX_STATE_BYTES)
    if (!isObject(doc) || doc.schema !== SCHEMA) return null
    if (!isNum(doc.state_version) || doc.state_version < 0 || Math.floor(doc.state_version) !== doc.state_version) return null
    if (doc.life !== null && doc.life !== undefined && !isObject(doc.life)) return null
    return doc
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

// Normalized plugin settings; junk falls back to the manifest defaults.
function config(settings) {
    var s = isObject(settings) ? settings : {}
    var label = ["name", "status", "none"].indexOf(s.label) >= 0 ? s.label : "name"
    return {
        label: label,
        presence: s.presence === undefined || s.presence === null ? true : String(s.presence) === "true",
        overlayOnLoad: String(s.overlayOnLoad) === "true",
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
        // Extra environment for every BarRpc process; empty = inherit. A test state home
        // also sets CODEPET_TEST_STATE=1: the CLI then refuses Connect, Stop watching and
        // the ready check (TEST_STATE), so a test never touches the owner's coding apps.
        childEnv: testState ? { XDG_STATE_HOME: cfg.stateHome, CODEPET_TEST_STATE: "1" } : {},
        // env(1) words for the desktop-pet launcher: the configured binary and a test
        // state home (with its CODEPET_TEST_STATE) reach the overlay too. Empty = inherit.
        launcherEnv: (cfg.binary ? ["CODEPET_BIN=" + cfg.binary] : [])
            .concat(testState ? ["XDG_STATE_HOME=" + cfg.stateHome, "CODEPET_TEST_STATE=1"] : [])
    }
}

function thumbPath(petId, stage) {
    if (typeof petId !== "string" || !/^[a-z_]{2,24}$/.test(petId)) return ""
    if (stage !== 1 && stage !== 2 && stage !== 3) return ""
    return "qml/assets/pets/" + petId + "/thumb-s" + stage + ".webp"
}

function allowedMethod(m) { return ALLOWED_METHODS.indexOf(m) >= 0 }
function allowedCareAction(a) { return CARE_ACTIONS.indexOf(a) >= 0 }

// How long an unconfirmed press may be retried under its own key. A press of the same
// button after this window is a new press: reusing the old key would make the backend
// replay the old receipt (no new care applied) while the bar said "Care confirmed".
var RETRY_WINDOW_MS = 120000

// One retry key per button press: the same press retried soon after an unconfirmed
// response (same signature, within RETRY_WINDOW_MS) reuses its key, so the backend
// applies it at most once. Anything else gets a fresh key.
function retryKey(prev, signature, now, seq) {
    if (isObject(prev) && prev.key && prev.signature === signature
            && isNum(prev.expiresAt) && isNum(now) && now <= prev.expiresAt)
        return prev
    return { key: "bar-" + Math.max(0, Math.floor(now)).toString(36) + "-" + Math.max(0, Math.floor(seq)),
             signature: signature, expiresAt: (isNum(now) ? now : 0) + RETRY_WINDOW_MS }
}

// The key to keep after a response: none after a confirmed one; after an unconfirmed
// one (failure, timeout, lost response), the same key for RETRY_WINDOW_MS from now.
function retryAfterResponse(prev, resp, now) {
    if (!isObject(prev) || !prev.key || !isObject(resp) || resp.ok === true || !isNum(now)) return null
    return { key: prev.key, signature: prev.signature, expiresAt: now + RETRY_WINDOW_MS }
}

function newRequestId(now, seq) { return "bar-rq-" + Math.max(0, Math.floor(now)).toString(36) + "-" + Math.max(0, Math.floor(seq)) }

// ---------------------------------------------------------------- registry lookups

function speciesMeta(reg, id) { return reg && isObject(reg.species) && isObject(reg.species[id]) ? reg.species[id] : null }
function speciesName(reg, id) { var m = speciesMeta(reg, id); return m ? str(m.displayName, "Companion") : "Companion" }
function stageInfo(reg, id, stage) {
    var m = speciesMeta(reg, id)
    var list = m && Array.isArray(m.stages) ? m.stages : []
    for (var i = 0; i < list.length; i++) if (isObject(list[i]) && list[i].stage === stage) return list[i]
    return null
}
function stageName(reg, id, stage) { var s = stageInfo(reg, id, stage); return s ? str(s.name, "stage " + stage) : "stage " + stage }
function stageMinLevel(reg, id, stage) {
    var s = stageInfo(reg, id, stage)
    return s && isNum(s.minLevel) ? s.minLevel : DEFAULT_STAGE_MIN_LEVEL[stage - 1]
}
function careLabel(reg, id, action) {
    var m = speciesMeta(reg, id)
    var labels = m && isObject(m.careLabels) ? m.careLabels : {}
    return clampText(str(labels[action], DEFAULT_CARE_LABEL[action] || "Care"), 24)
}

// ---------------------------------------------------------------- formatting

function plural(n, one, many) { return n + " " + (n === 1 ? one : many) }

// "about 2 h 10 min of active time"; long form spells the units (accessible names).
function formatActive(ms, long) {
    if (!isNum(ms) || ms < 0) return ""
    var mins = Math.floor(ms / 60000)
    if (mins < 1) return "less than a minute of active time"
    var h = Math.floor(mins / 60), m = mins % 60
    var parts = []
    if (h > 0) parts.push(long ? plural(h, "hour", "hours") : h + " h")
    if (m > 0) parts.push(long ? plural(m, "minute", "minutes") : m + " min")
    return "about " + parts.join(" ") + " of active time"
}

function pad2(n) { return (n < 10 ? "0" : "") + n }
function formatAsOf(ms) {
    if (!isNum(ms) || ms <= 0) return ""
    var d = new Date(ms)
    return "as of " + pad2(d.getHours()) + ":" + pad2(d.getMinutes())
}

// ---------------------------------------------------------------- status

function needsList(state) {
    var out = []
    var list = state && Array.isArray(state.needs) ? state.needs : []
    for (var i = 0; i < list.length && out.length < 8; i++) {
        var n = list[i]
        if (isObject(n) && NEEDS.indexOf(n.need) >= 0) out.push(n)
    }
    return out
}

// The need that matters most: critical > empty > low, then the shortest window.
function worstNeed(state) {
    var rank = { critical: 3, empty: 2, low: 1 }
    var best = null
    var list = needsList(state)
    for (var i = 0; i < list.length; i++) {
        var n = list[i], r = rank[n.state] || 0
        if (r === 0) continue
        if (!best || r > best.r || (r === best.r && isNum(n.until_critical_ms)
                && (!isNum(best.n.until_critical_ms) || n.until_critical_ms < best.n.until_critical_ms)))
            best = { r: r, n: n }
    }
    return best ? best.n : null
}

function taskState(state) { return state && isObject(state.task) && typeof state.task.state === "string" ? state.task.state : "" }

// Committed status from the last bar-state.json alone.
function statusOf(state) {
    if (!state) return "missing"
    var life = state.life
    if (!isObject(life)) return "no_life"
    if (life.death_pending === true) return "death_pending"
    if (life.fainted === true) return "fainted"
    if (life.status === "critical") return "critical"
    if (life.armed !== true) return "unarmed"
    if (life.status === "warning") return "warning"
    var task = taskState(state)
    if (REVIEW_STATES.indexOf(task) >= 0) return "review"
    if (state.care && state.care.sleeping === true) return "sleeping"
    if (life.paused === true) return "paused"
    if (WORKING_STATES.indexOf(task) >= 0) return "working"
    if (Array.isArray(state.unseen_events) && state.unseen_events.length > 0) return "evolved"
    return "healthy"
}

function severityOf(status) {
    if (status === "critical" || status === "fainted" || status === "death_pending") return "critical"
    if (status === "warning") return "warning"
    return "none"
}

// Leader-only prediction: attended time the recorder committed after the file was
// generated, compared with the Rust-computed windows in bar-state.json. It can only
// escalate to warning or critical; it never predicts a death or a memorial.
function predict(state, attendedSinceMs) {
    var none = { severity: "none", need: "", action: "" }
    if (!state || !isObject(state.life) || !isNum(attendedSinceMs) || attendedSinceMs <= 0) return none
    var life = state.life
    if (life.armed !== true || life.paused === true || life.death_pending === true || life.fainted === true) return none
    var presence = state.presence
    if (!isObject(presence) || presence.wanted !== true || presence.life_id !== life.life_id) return none
    var t = attendedSinceMs
    // Rust discards the remaining nap first.
    if (state.care && state.care.sleeping === true && isNum(state.care.sleep_remaining_ms))
        t -= Math.max(0, state.care.sleep_remaining_ms)
    if (t <= 0) return none
    var best = none
    var list = needsList(state)
    for (var i = 0; i < list.length; i++) {
        var n = list[i], sev = "none"
        if (isNum(n.until_critical_ms) && n.until_critical_ms >= 0 && t >= n.until_critical_ms) sev = "critical"
        else if (isNum(n.until_warning_ms) && n.until_warning_ms >= 0 && t >= n.until_warning_ms) sev = "warning"
        if (SEVERITY_RANK[sev] > SEVERITY_RANK[best.severity])
            best = { severity: sev, need: n.need, action: str(n.action, NEED_ACTION[n.need]) }
    }
    return best
}

function statusWord(status, state, predictedAction, predicted) {
    var worst = worstNeed(state)
    var action = predictedAction || (worst ? str(worst.action, NEED_ACTION[worst.need]) : "")
    var prefix = predicted ? "≈ " : ""
    switch (status) {
    case "missing": return "set up"
    case "no_life": return "adopt"
    case "death_pending": return "memorial"
    case "fainted": return "fainted"
    case "critical": return prefix + (action ? action + "!!" : "urgent")
    case "unarmed": return "set up"
    case "warning": return prefix + (action ? action + "!" : "check")
    case "review": return "review"
    case "sleeping": return "napping"
    case "paused": return "paused"
    case "working": return "working"
    case "evolved": return "grew"
    default: return "ok"
    }
}

function taskLine(state) {
    var t = taskState(state)
    if (!t) return ""
    return "Task: " + t.replace(/_/g, " ") + " (simulation)."
}

// opts: {registry, attendedSinceMs (leader only, else 0/undefined)}
function summarize(state, cfg, opts) {
    opts = opts || {}
    cfg = cfg || config({})
    var reg = opts.registry || null
    var committed = statusOf(state)
    var status = committed
    var predicted = false
    var pred = predict(state, opts.attendedSinceMs)
    if (pred.severity === "critical" && ["death_pending", "fainted", "critical", "unarmed", "missing", "no_life"].indexOf(committed) < 0) {
        status = "critical"; predicted = true
    } else if (pred.severity === "warning" && STATUS_ORDER.indexOf(committed) > STATUS_ORDER.indexOf("warning")) {
        status = "warning"; predicted = true
    }
    var life = state && isObject(state.life) ? state.life : null
    var name = life ? clampText(str(life.name, "Your companion"), 24) : ""
    var label = ""
    if (cfg.label !== "none") {
        if (!state) label = "CodePet"
        else if (!life) label = "adopt"
        else label = cfg.label === "status" && status !== "healthy"
            ? name + " · " + statusWord(status, state, predicted ? pred.action : "", predicted) : name
    }
    return {
        status: status,
        committedStatus: committed,
        predicted: predicted,
        glyph: STATUS_GLYPH[status] || "",
        severity: severityOf(status),
        label: label,
        accessibleName: accessibleName(state, status, predicted, pred, reg)
    }
}

function needSentence(n, long) {
    if (!n) return ""
    var label = NEED_LABEL[n.need] || "A need"
    var action = str(n.action, NEED_ACTION[n.need])
    var words = label.toLowerCase() + " " + (n.state === "critical" ? "urgent" : n.state === "empty" ? "empty" : "low")
    if (isNum(n.until_critical_ms) && n.until_critical_ms > 0)
        return words + "; " + action + " within " + formatActive(n.until_critical_ms, long) + "."
    return words + "; " + action + " will help."
}

function accessibleName(state, status, predicted, pred, reg) {
    if (!state) return "CodePet: not set up yet. Open CodePet once to set up."
    var life = state.life
    if (!isObject(life)) {
        var latest = state.memorial && typeof state.memorial.latest_name === "string" && state.memorial.latest_name
            ? " " + clampText(state.memorial.latest_name, 24) + " is remembered in the memorial." : ""
        return "CodePet: no companion right now." + latest + " You can adopt a new companion whenever you're ready."
    }
    var name = clampText(str(life.name, "Your companion"), 24)
    var stage = isNum(life.stage) ? life.stage : 1
    var out = "CodePet: " + name + ", " + stageName(reg, str(life.appearance, ""), stage) + ", level " + (isNum(life.level) ? life.level : 1) + "."
    var worst = worstNeed(state)
    switch (status) {
    case "death_pending": out += " " + name + "'s grace time has run out. They will pass away when the current task is finished; your work is kept."; break
    case "fainted": out += " Fainted. Care will revive them; work waits until then."; break
    case "critical":
        if (predicted) out += " Estimated: " + name + " may need you urgently. Open CodePet to check."
        else out += " Urgent: " + (worst ? needSentence(worst, true) : "open CodePet to check.")
        break
    case "unarmed": out += " Not set up: time isn't counting until you choose the life rules."; break
    case "warning":
        if (predicted) out += " Estimated warning: " + (NEED_LABEL[pred.need] || "a need").toLowerCase() + " may be low. Open CodePet to check."
        else out += " Warning: " + (worst ? needSentence(worst, true) : "open CodePet to check.")
        break
    case "review": out += " A result is waiting for your review."; break
    case "sleeping": out += " Napping."; break
    case "paused": out += " Paused: time isn't counting."; break
    case "evolved": out += " " + name + " grew!"; break
    case "healthy": out += " Doing well."; break
    }
    var task = taskLine(state)
    return out + (task ? " " + task : "")
}

// Tooltip: a few short plain-text lines.
function tooltip(state, cfg, opts) {
    opts = opts || {}
    var sum = summarize(state, cfg, opts)
    if (!state) return "CodePet\nOpen CodePet once to set up."
    var lines = []
    var life = state.life
    if (isObject(life)) {
        var reg = opts.registry || null
        lines.push(clampText(str(life.name, "Your companion"), 24) + " · " + stageName(reg, str(life.appearance, ""), isNum(life.stage) ? life.stage : 1)
            + " · Level " + (isNum(life.level) ? life.level : 1))
    } else {
        lines.push("CodePet · no companion")
    }
    lines.push((sum.glyph ? sum.glyph + " " : "") + lifecycleText(state, { predicted: sum.predicted ? sum.status : "", attendedSinceMs: opts.attendedSinceMs }))
    var task = taskLine(state)
    if (task) lines.push(task)
    var asOf = formatAsOf(state.generated_at_ms)
    if (asOf) lines.push(asOf + " · simulation only")
    lines.push("Click: care · right-click: desktop pet · middle-click: tasks")
    return lines.join("\n")
}

// The life status line, using the lifecycle copy rules.
// opts.predicted: "warning" | "critical" | "" (from summarize, leader only).
// Same words as qml/shared/LifeText.js pendingText() (test_bar_model.mjs checks they match):
// the companion has not passed away yet; it will once the current task is finished.
function pendingText(name) {
    return name + "'s grace time has run out. They will pass away when the current task is finished. "
        + "The task and its result are kept safe: review, accept, discard or cancel it as usual. New work waits until then."
}

function lifecycleText(state, opts) {
    opts = opts || {}
    if (!state) return "Open CodePet once to set up."
    var life = state.life
    if (!isObject(life)) {
        var latest = state.memorial && typeof state.memorial.latest_name === "string" && state.memorial.latest_name
            ? clampText(state.memorial.latest_name, 24) + " is remembered in the memorial. " : ""
        return latest + "No companion right now. You can adopt a new companion whenever you're ready."
    }
    var name = clampText(str(life.name, "Your companion"), 24)
    var worst = worstNeed(state)
    var crit = isObject(state.critical) ? state.critical : null
    var critNeed = crit && NEEDS.indexOf(crit.need) >= 0 ? crit.need : (worst ? worst.need : "thirst")
    var critAction = crit ? str(crit.action, NEED_ACTION[critNeed]) : NEED_ACTION[critNeed]
    if (life.death_pending === true)
        return pendingText(name)
    if (life.fainted === true)
        return name + " fainted from " + NEED_NOUN[critNeed] + ". " + ACTION_WORD[critAction] + " will revive them; work waits until then."
    if (life.status === "critical") {
        var text = name + " urgently needs " + critAction + ". Care now keeps them safe."
        var remaining = crit && isNum(crit.remaining_ms) ? crit.remaining_ms
            : crit && isNum(crit.grace_ms) && isNum(crit.elapsed_ms) ? crit.grace_ms - crit.elapsed_ms : NaN
        if (life.mode === "permadeath" && isNum(remaining) && remaining > 0)
            text += " Without " + critAction + ", " + name + " may pass away after " + formatActive(remaining) + "."
        return text
    }
    if (opts.predicted === "critical")
        return "≈ " + name + " may need you urgently. Open CodePet to check."
    if (life.armed !== true)
        return "Paused: time isn't counting (not set up). Open Life to choose how " + name + "'s needs work."
    if (life.status === "warning" && worst) {
        var w = name + " is getting " + NEED_FEELING[worst.need] + ". " + ACTION_WORD[str(worst.action, NEED_ACTION[worst.need])] + " will help."
        if (isNum(worst.until_critical_ms) && worst.until_critical_ms > 0) {
            var t = formatActive(worst.until_critical_ms)
            w += " " + t.charAt(0).toUpperCase() + t.slice(1) + " before it becomes serious."
        }
        return w
    }
    if (opts.predicted === "warning")
        return "≈ " + name + " may be getting low on something. Open CodePet to check."
    if (life.paused === true) return "Paused: time isn't counting (on a break)."
    if (taskState(state)) return "Paused: time isn't counting (task running)."
    if (state.care && state.care.sleeping === true) return name + " is napping. Needs wait while they rest."
    return name + " is doing well."
}

// "grows into a Grand Inspector at level 4 (280 XP to go)", or "" at the final stage.
function nextEvolutionText(state, reg) {
    var life = state && isObject(state.life) ? state.life : null
    if (!life) return ""
    var stage = isNum(life.stage) ? life.stage : 1
    if (stage >= 3) return "fully grown"
    var id = str(life.appearance, "")
    var next = stage + 1
    var text = "grows into " + article(stageName(reg, id, next)) + " at level " + stageMinLevel(reg, id, next)
    if (isNum(life.next_stage_xp) && isNum(life.xp) && life.next_stage_xp > life.xp)
        text += " (" + (life.next_stage_xp - life.xp) + " XP to go)"
    return text
}

function article(name) {
    if (/^stage /.test(name)) return name
    return (/^[AEIOU]/i.test(name) ? "an " : "a ") + name
}

function headerText(state, reg) {
    var life = state && isObject(state.life) ? state.life : null
    if (!life) return "CodePet"
    var parts = [clampText(str(life.name, "Your companion"), 24), stageName(reg, str(life.appearance, ""), isNum(life.stage) ? life.stage : 1),
                 "Level " + (isNum(life.level) ? life.level : 1), (isNum(life.xp) ? life.xp : 0) + " XP"]
    var next = nextEvolutionText(state, reg)
    if (next) parts.push(next)
    return parts.join(" · ")
}

function badgesText(state) {
    var life = state && isObject(state.life) ? state.life : null
    if (!life) return ""
    var b = []
    b.push(life.mode === "gentle" ? "Gentle" : "Permadeath")
    if (life.pace === "playtest") b.push("Playtest")
    if (life.paused === true) b.push("Paused")
    if (life.armed !== true) b.push("Not set up")
    return b.join(" · ")
}

// Four need rows for the popup.
function needRows(state, reg) {
    var rows = []
    var appearance = state && isObject(state.life) ? str(state.life.appearance, "") : ""
    var byNeed = {}
    var list = needsList(state)
    for (var i = 0; i < list.length; i++) byNeed[list[i].need] = list[i]
    for (var j = 0; j < NEEDS.length; j++) {
        var need = NEEDS[j]
        var n = byNeed[need] || {}
        var meter = state && isObject(state.meters) && isNum(state.meters[need]) ? state.meters[need] : NaN
        var stateText = n.state === "critical" ? "urgent" : n.state === "empty" ? "empty" : n.state === "low" ? "low" : "fine"
        rows.push({
            need: need,
            label: NEED_LABEL[need],
            action: NEED_ACTION[need],
            actionLabel: careLabel(reg, appearance, NEED_ACTION[need]),
            percent: isNum(meter) ? Math.max(0, Math.min(100, Math.round(meter / 100))) : -1,
            stateText: stateText,
            timeText: isNum(n.until_critical_ms) && n.until_critical_ms > 0 && n.state !== "critical"
                ? "serious after " + formatActive(n.until_critical_ms) : ""
        })
    }
    return rows
}

function readinessText(state) {
    var care = state && isObject(state.care) ? state.care : null
    if (!care) return ""
    if (care.ready_to_work === true) return "Ready to work."
    var reasons = Array.isArray(care.readiness_reasons) ? care.readiness_reasons.slice(0, 8) : []
    var clean = []
    for (var i = 0; i < reasons.length; i++) if (typeof reasons[i] === "string") clean.push(clampText(reasons[i], 160))
    return clean.length ? "Not ready: " + clean.join(" · ") : "Not ready yet."
}

// Remaining nap at `now`, estimated from the committed file (display only).
function sleepRemainingMs(state, now) {
    var care = state && isObject(state.care) ? state.care : null
    if (!care || care.sleeping !== true || !isNum(care.sleep_remaining_ms)) return -1
    var since = isNum(state.generated_at_ms) && isNum(now) ? Math.max(0, now - state.generated_at_ms) : 0
    return Math.max(0, care.sleep_remaining_ms - since)
}

function formatClock(ms) {
    if (!isNum(ms) || ms < 0) return ""
    var s = Math.ceil(ms / 1000)
    return Math.floor(s / 60) + ":" + pad2(s % 60)
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

function grewText(name, reg, appearance, stage) {
    return "★ " + clampText(str(name, "Your companion"), 24) + " grew into " + article(stageName(reg, str(appearance, ""), stage)) + "!"
}

// The life-panel launcher verb label: Adopt… with no life, Set up… when unarmed.
function lifeButtonLabel(state) {
    var life = state && isObject(state.life) ? state.life : null
    if (!life) return "Adopt…"
    if (life.armed !== true) return "Set up…"
    return "Life & memorial…"
}
