// Shared presentation rules for the task window. Provider text is always plain text.
var KINDS = ["bug", "research", "build"]
function clientId(state) { return state && state.provider ? String(state.provider.id || "codex") : "" }
function clientName(state) { return state && state.provider ? String(state.provider.name || "Codex") : "Checking…" }
function taskClientName(task) {
    if (!task) return ""
    if (task.client_name) return String(task.client_name)
    return ({codex:"Codex", "claude-code":"Claude Code", opencode:"OpenCode", muse:"Muse", grok:"Grok", hermes:"Hermes", pi:"pi"})[task.client_id || "codex"] || String(task.client_id)
}
function kind(value) { return KINDS.indexOf(value) >= 0 ? value : "build" }
function active(task) { return !!task && ["preparing", "running", "applying"].indexOf(task.status) >= 0 }
function executing(task) { return !!task && ["preparing", "running"].indexOf(task.status) >= 0 }
function review(task) { return !!task && ["review", "applying"].indexOf(task.status) >= 0 }
function finished(task) { return !!task && (["accepted", "cancelled", "failed", "discarded"].indexOf(task.status) >= 0 || (task.status === "scouted" && !task.has_finding)) }
function title(value) { return { bug: "Fix a bug", research: "Research", build: "Build something" }[kind(value)] }
function reward(value) { return { bug: "A fixed bug feeds your pet.", research: "New knowledge quenches its thirst.", build: "Building something lifts its mood." }[kind(value)] }
function stateTitle(task) {
    if (!task) return "What shall we work on?"
    if (finished(task)) return previousTitle(task)
    return { preparing: "Getting ready", running: "Your pet is working", review: "Ready for your review",
        applying: "Finish applying this work", scouted: "Repository check finished", accepted: "Work accepted!", cancelled: "Work stopped",
        failed: "This job needs another try", discarded: "Work set aside" }[task.status] || "Checking this job"
}
function canStart(state, busy, project, prompt) {
    return !busy && !!state && !!state.readiness && state.readiness.ready === true
        && !!state.provider && state.provider.available === true && !state.task
        && typeof project === "string" && project.trim().charAt(0) === "/"
        && typeof prompt === "string" && prompt.trim().length > 0
}
function canAccept(task, busy, checked) {
    return review(task) && task.source_kind !== "scout" && !busy && checked && typeof task.review_digest === "string" && task.review_digest.length > 0
}
function energy(state) {
    var value = state && state.readiness ? state.readiness.energy : null
    return typeof value === "number" && isFinite(value) ? Math.max(0, Math.min(100, Math.floor(value))) : null
}
function filePath(url) {
    var raw = String(url)
    if (raw.indexOf("file:///") !== 0) return ""
    try { return decodeURIComponent(raw.slice(7)) } catch (e) { return "" }
}

function finding(task) { return !!task && task.status === "scouted" && task.has_finding === true }
function badge(task) { return finding(task) || review(task) }
function scoutEligible(state) {
    return !!state && !!state.scout && state.scout.enabled === true
        && typeof state.scout.next_due_ms === "number" && isFinite(state.scout.next_due_ms)
        && !!state.readiness && state.readiness.ready === true
        && !!state.provider && state.provider.available === true
        && (!state.task || finished(state.task))
}
function repoName(task) {
    var path = task && typeof task.project === "string" ? task.project.replace(/\/$/, "") : ""
    return path ? path.slice(path.lastIndexOf("/") + 1) : "Local repositories"
}

function previousTitle(task) {
    if (!task) return "Previous check · Could not start"
    var type = task.source_kind === "scout" ? "check" : task.kind === "bug" ? "repair" : task.kind === "research" ? "research" : "build"
    var outcome = {failed: "Couldn’t finish", accepted: "Accepted", cancelled: "Stopped", discarded: "Set aside", scouted: "No finding"}[task.status] || "Finished"
    return "Previous " + type + " · " + outcome
}
function previousMessage(task) {
    if (!task) return ""
    if (task.status === "failed") return String(task.error || "This job ended without a result. No retry was started.")
    if (task.status === "accepted") return "+20 XP · " + (task.kind === "research" ? "Thirst restored." : task.kind === "build" ? "Mood lifted." : "A well-earned meal.")
    if (task.status === "scouted") return "No suspected bug was reported in this check."
    return task.status === "cancelled" ? "Stopped without accepting a result." : "Result set aside."
}
function startedText(task, now) {
    var at = task && task.started_at_ms
    if (typeof at !== "number" || !isFinite(at) || at <= 0 || at > now || Math.floor(at) !== at)
        return "Start time unavailable"
    var minutes = Math.floor((now - at) / 60000)
    if (minutes < 1) return "Started just now"
    var value = minutes >= 1440 ? Math.floor(minutes / 1440) : minutes >= 60 ? Math.floor(minutes / 60) : minutes
    var unit = minutes >= 1440 ? "day" : minutes >= 60 ? "hour" : "minute"
    return "Started " + value + " " + unit + (value === 1 ? "" : "s") + " ago"
}

function meterText(meters) {
    function percent(value) { return typeof value === "number" && isFinite(value) ? Math.floor(Math.max(0, Math.min(10000, value)) / 100) + "%" : "—" }
    return "Food " + percent(meters && meters.hunger) + " · Water " + percent(meters && meters.thirst)
        + " · Mood " + percent(meters && meters.mental_health) + " · Clean " + percent(meters && meters.hygiene)
}

function startedDate(task) {
    var at = task && task.started_at_ms
    if (typeof at !== "number" || !isFinite(at) || at <= 0 || at > 8640000000000000 || Math.floor(at) !== at)
        return "Start time unavailable"
    return "Started " + new Date(at).toLocaleString()
}
