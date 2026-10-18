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
    property real dismissedScoutDue: -1
    property string draftKey: ""
    property string draftRequest: ""
    property string fixKey: ""
    property string fixRequest: ""
    property string attemptedScoutKey: ""
    property string selectedClientSeen: ""
    property string selectingClient: ""
    property string summaryTaskKey: ""
    property bool refreshPending: false
    readonly property bool busy: rpc.busy || careRpc.busy
    readonly property var task: state ? state.task : null
    readonly property var scheduleState: summary || state
    function visibleState(next) {
        if (next && next.task && next.task.id === dismissedTask && Work.finished(next.task))
            next = Object.assign({}, next, { task: null })
        if (next && next.scout && next.scout.next_due_ms === dismissedScoutDue)
            next = Object.assign({}, next, { scout: Object.assign({}, next.scout, { notice: "", last_error: "" }) })
        return next
    }
    function consume(response) {
        if (!response.ok) { error = String((response.error || {}).message || "Something went wrong."); return }
        var next = response.result
        if (!next || !next.readiness || !next.provider) { error = "CodePet returned an incomplete answer."; return }
        observeClient(next)
        state = visibleState(next)
        // Saved preparation failures belong to Previous check, not a fresh action error.
        error = ""
    }
    function refresh() {
        if (busy) { refreshPending = true; return }
        refreshPending = false
        rpc.request("get", {}, consume)
    }
    function action(method, params) { if (!busy) rpc.request(method, params, consume) }
    function observeClient(next) {
        var id = Work.clientId(next)
        // A choice made in either window must not turn a due check into a launch.
        if (selectedClientSeen && id !== selectedClientSeen) deferDueScout(next)
        selectedClientSeen = id
    }
    function deferDueScout(next) {
        if (next && next.scout && next.scout.next_due_ms <= Date.now()) attemptedScoutKey = scoutKey(next)
    }
    function selectClient(id) {
        if (busy || !state || !Array.isArray(state.clients) || !state.clients.some(function(c) { return c.id === id })) return
        deadline.stop()
        deferDueScout(scheduleState)
        selectingClient = id
        rpc.request("client-select", { client_id: id }, function(response) {
            if (response.ok) controller.deferDueScout(response.result)
            controller.consume(response)
            controller.selectingClient = ""
        })
    }
    function start(kind, project, prompt) {
        if (busy) return
        var client = Work.clientId(state)
        var key = JSON.stringify([client, kind, project, prompt])
        if (key !== draftKey || !draftRequest) { draftKey = key; draftRequest = identity() }
        rpc.request("start", { request_id: draftRequest, client_id: client, kind: kind, project: project, prompt: prompt, network: true }, function(response) {
            controller.consume(response)
            if (response.ok) { controller.draftKey = ""; controller.draftRequest = "" }
        })
    }
    function identity() { return "work-" + Date.now().toString(36) + "-" + Math.random().toString(36).slice(2) }
    function fix() {
        if (busy || !Work.finding(task) || !task.review_digest) return
        var client = Work.clientId(state)
        var key = JSON.stringify([task.id, task.review_digest, client])
        if (key !== fixKey) { fixKey = key; fixRequest = identity() }
        action("fix", { task_id: task.id, review_digest: task.review_digest, request_id: fixRequest, client_id: client })
    }
    function dismiss() {
        if (busy) return
        var id = Work.finished(task) ? task.id : ""
        var scout = state && state.scout
        if (!id && (task || !scout || !scout.notice)) return
        var params = id ? { task_id: id } : { scout_due_ms: scout.next_due_ms }
        rpc.request("dismiss", params, function(response) {
            if (response.ok && id && response.result && response.result.readiness && response.result.provider && !response.result.task) controller.dismissedTask = id
            if (response.ok && !id && response.result && response.result.scout && !response.result.scout.notice)
                controller.dismissedScoutDue = scout.next_due_ms
            controller.consume(response)
        })
    }
    function newJob() { if (Work.finished(task)) dismiss() }
    function care(action) {
        if (busy) return
        careRpc.request(action === "sleep" ? "pet.sleep" : "pet.care", action === "sleep" ? {} : { action: action }, function(response) {
            if (!response.ok) controller.error = String((response.error || {}).message || "That care action could not finish.")
            else Qt.callLater(controller.refresh)
        })
    }
    function scoutKey(s) {
        return s ? JSON.stringify([s.scout ? [s.scout.enabled, s.scout.next_due_ms] : null, s.readiness, s.task && !Work.finished(s.task) ? [s.task.id, s.task.status] : null]) : ""
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
        if (summary) observeClient(summary)
        var nextTask = summary ? summary.task : null
        var key = JSON.stringify([nextTask ? [nextTask.id, nextTask.status, nextTask.has_finding] : null,
            summary && summary.scout ? [summary.scout.next_due_ms, summary.scout.notice] : null])
        var changed = key !== summaryTaskKey
        summaryTaskKey = key
        if (opened && summary) {
            // get republishes this file, including a ticking sleep countdown. Merge
            // public care data directly; only a task transition needs hydration.
            var scout = summary.scout
            // The public export omits private error text. Retain hydrated details only
            // for the same visible attempt; acknowledgement or a new attempt clears it.
            if (scout && scout.notice && state && state.scout && state.scout.next_due_ms === scout.next_due_ms
                    && typeof state.scout.last_error === "string")
                scout = Object.assign({}, scout, { last_error: state.scout.last_error })
            state = visibleState(Object.assign({}, state || summary, {
                readiness: summary.readiness, provider: summary.provider, clients: summary.clients, scout: scout
            }))
            if (changed) refresh()
        } else if (summary) state = visibleState(summary)
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
