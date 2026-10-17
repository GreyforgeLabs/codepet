// Shared presentation rules for the task window. Provider text is always plain text.
var KINDS = ["bug", "research", "build"]
function kind(value) { return KINDS.indexOf(value) >= 0 ? value : "build" }
function active(task) { return !!task && ["preparing", "running", "applying"].indexOf(task.status) >= 0 }
function review(task) { return !!task && ["review", "applying"].indexOf(task.status) >= 0 }
function finished(task) { return !!task && (["accepted", "cancelled", "failed", "discarded"].indexOf(task.status) >= 0 || (task.status === "scouted" && !task.has_finding)) }
function title(value) { return { bug: "Fix a bug", research: "Research", build: "Build something" }[kind(value)] }
function reward(value) { return { bug: "A fixed bug feeds your pet.", research: "New knowledge quenches its thirst.", build: "Building something lifts its mood." }[kind(value)] }
function stateTitle(task) {
    if (!task) return "What shall we work on?"
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
