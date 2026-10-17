pragma ComponentBehavior: Bound
import QtQuick
import "WorkModel.js" as Work

// One controller serves the desktop card and the explicit work window. The worker
// outlives either view; snapshots contain no private project or finding text.
QtObject {
    id: controller
    property string binary: "codepet"
    property var pet: null
    property bool calm: false
    property bool opened: false
    property bool automatic: false
    property var summary: null
    property var state: null
    property string error: ""
    property string dismissedTask: ""
    property string draftKey: ""
    property string draftRequest: ""
    property string fixKey: ""
    property string fixRequest: ""
    property string attemptedScoutKey: ""
    property string summaryTaskKey: ""
    property bool refreshPending: false
    readonly property bool busy: rpc.busy || careRpc.busy
    readonly property var task: state ? state.task : null
    readonly property var scheduleState: summary || state
    function consume(response) {
        if (!response.ok) { error = String((response.error || {}).message || "Something went wrong."); return }
        var next = response.result
        if (!next || !next.readiness || !next.provider) { error = "CodePet returned an incomplete answer."; return }
        if (next.task && next.task.id === dismissedTask && Work.finished(next.task)) next.task = null
        state = next
        // Preparation refusals have no task; retain their private reason across get/reopen.
        error = next.scout ? String(next.scout.last_error || "") : ""
    }
    function refresh() {
        if (busy) { refreshPending = true; return }
        refreshPending = false
        rpc.request("get", {}, consume)
    }
    function action(method, params) { if (!busy) rpc.request(method, params, consume) }
    function start(kind, project, prompt) {
        if (busy) return
        var key = JSON.stringify([kind, project, prompt])
        if (key !== draftKey || !draftRequest) { draftKey = key; draftRequest = identity() }
        rpc.request("start", { request_id: draftRequest, kind: kind, project: project, prompt: prompt, network: true }, function(response) {
            controller.consume(response)
            if (response.ok) { controller.draftKey = ""; controller.draftRequest = "" }
        })
    }
    function identity() { return "work-" + Date.now().toString(36) + "-" + Math.random().toString(36).slice(2) }
    function fix() {
        if (busy || !Work.finding(task) || !task.review_digest) return
        var key = task.id + ":" + task.review_digest
        if (key !== fixKey) { fixKey = key; fixRequest = identity() }
        action("fix", { task_id: task.id, review_digest: task.review_digest, request_id: fixRequest })
    }
    function newJob() {
        if (!Work.finished(task)) return
        dismissedTask = task.id
        state = Object.assign({}, state, { task: null })
        error = ""
    }
    function care(action) {
        if (busy) return
        careRpc.request(action === "sleep" ? "pet.sleep" : "pet.care", action === "sleep" ? {} : { action: action }, function(response) {
            if (!response.ok) controller.error = String((response.error || {}).message || "That care action could not finish.")
            else Qt.callLater(controller.refresh)
        })
    }
    function scoutKey(s) {
        return s ? JSON.stringify([s.scout, s.readiness, s.provider, s.task ? [s.task.id, s.task.status] : null]) : ""
    }
    function armScout() {
        deadline.stop()
        var s = scheduleState
        if (!automatic || busy || !Work.scoutEligible(s) || scoutKey(s) === attemptedScoutKey) return
        deadline.interval = Math.max(20, Math.min(2147483647, s.scout.next_due_ms - Date.now()))
        deadline.start()
    }
    function scout() {
        var s = scheduleState
        if (!automatic || busy || !Work.scoutEligible(s)) return
        attemptedScoutKey = scoutKey(s)
        rpc.request("scout", {}, function(response) {
            controller.consume(response)
            // A refused due attempt is not rearmed against the same snapshot.
            // A follow-up read sees persisted cooldown or changed readiness.
            controller.refreshPending = true
        })
    }
    onSummaryChanged: {
        var nextTask = summary ? summary.task : null
        var key = JSON.stringify(nextTask ? [nextTask.id, nextTask.status, nextTask.has_finding] : null)
        var changed = key !== summaryTaskKey
        summaryTaskKey = key
        if (opened && summary) {
            // get republishes this file, including a ticking sleep countdown. Merge
            // public care data directly; only a task transition needs hydration.
            state = Object.assign({}, state || summary, {
                readiness: summary.readiness, provider: summary.provider, scout: summary.scout
            })
            if (changed) refresh()
        } else if (summary) state = summary
        armScout()
    }
    onOpenedChanged: if (opened) refresh()
    onAutomaticChanged: armScout()
    onScheduleStateChanged: Qt.callLater(armScout)
    onStateChanged: Qt.callLater(armScout)
    onBusyChanged: if (!busy) {
        if (refreshPending) Qt.callLater(refresh)
        else Qt.callLater(armScout)
    }
    property WorkRpc transport: WorkRpc { id: rpc; binary: controller.binary }
    property HostAdapter careTransport: HostAdapter { id: careRpc; clientTag: "work-care"; command: [controller.binary, "rpc"] }
    property Timer pollTimer: Timer {
        objectName: "work.poll"
        interval: 1200; repeat: true
        running: controller.opened && (Work.active(controller.task) || !!(controller.state && controller.state.readiness.sleeping))
        onTriggered: controller.refresh()
    }
    property Timer scoutTimer: Timer { id: deadline; objectName: "work.scoutDeadline"; repeat: false; onTriggered: controller.scout() }
}
