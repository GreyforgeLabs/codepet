import QtQuick
import QtQuick.Window
import "../omarchy"

Window {
    id: win
    width: 820; height: 900; visible: true
    property int failures: 0
    property int checks: 0
    property int phase: 0
    function check(value, label) { checks++; if (!value) failures++; console.log((value ? "PASS " : "FAIL ") + label) }
    function find(root, name) {
        if (!root) return null
        if (root.objectName === name) return root
        var kids = [].concat(root.children || [], root.resources || [])
        for (var i = 0; i < kids.length; i++) { var r = find(kids[i], name); if (r) return r }
        return null
    }
    QtObject {
        id: ctl
        property bool busy: false
        property bool calm: true
        property string error: ""
        property var pet: ({ name: "Pengu", appearance: "penguin", stage: 1 })
        property var calls: []
        property var state: ({ readiness: { ready: true, reason: "Ready for a job", energy: 100, sleeping: false }, provider: { name: "Codex", available: true }, task: null })
        function start(kind, project, prompt) { calls = calls.concat([{ method: "start", kind: kind, project: project, prompt: prompt }]) }
        function action(method, params) { calls = calls.concat([{ method: method, params: params }]) }
        function refresh() {}
        function care(action) { calls = calls.concat([{ method: "care", action: action }]) }
        function newJob() { state = Object.assign({}, state, { task: null }) }
    }
    WorkPanel { id: panel; anchors.fill: parent; controller: ctl }
    Timer {
        interval: 100; repeat: true; running: true
        onTriggered: {
            if (win.phase === 0) {
                win.check(!win.find(panel, "work.start").enabled, "empty form cannot start")
                panel.project = "/home/test/project"
                panel.prompt = "Fix the broken empty-input case"
                panel.selectedKind = "bug"
            } else if (win.phase === 1) {
                win.check(win.find(panel, "work.start").enabled, "ready pet and complete form can start")
                panel.start()
                win.check(ctl.calls.length === 1 && ctl.calls[0].kind === "bug", "Start preserves selected task kind")
                ctl.state = Object.assign({}, ctl.state, { readiness: { ready: false, energy: 0, sleeping: true, reason: "Time for a two-minute nap" } })
            } else if (win.phase === 2) {
                panel.start()
                win.check(ctl.calls.length === 1 && !win.find(panel, "work.start").enabled, "tired pet cannot bypass readiness")
                ctl.state = Object.assign({}, ctl.state, { task: { id: "job-1", kind: "bug", project: "/project", prompt: "Fix it", status: "running", progress: "Checking files", output: "<b>literal output</b>", changes: [] } })
            } else if (win.phase === 3) {
                win.check(win.find(panel, "work.stop").visible, "active job remains stoppable while pet is tired")
                win.check(win.find(panel, "work.output").textFormat === Text.PlainText, "provider output is plain text")
                win.find(panel, "work.stop").clicked()
                win.check(ctl.calls[1].method === "cancel" && ctl.calls[1].params.task_id === "job-1", "Stop targets durable job identity")
                ctl.state = Object.assign({}, ctl.state, { task: Object.assign({}, ctl.state.task, { status: "review", review_digest: "digest-a", changes: [{ path: "hello.js", before: "old text", after: "new text" }], reward_xp: 20 }) })
            } else if (win.phase === 4) {
                win.check(!win.find(panel, "work.accept").enabled, "accept waits for review acknowledgement")
                panel.verificationAcknowledged = true
            } else if (win.phase === 5) {
                win.check(win.find(panel, "work.accept").enabled, "review acknowledgement enables acceptance even if tired")
                panel.accept()
                win.check(ctl.calls[2].method === "accept" && ctl.calls[2].params.review_digest === "digest-a", "accept binds the reviewed digest")
                ctl.state = Object.assign({}, ctl.state, { task: Object.assign({}, ctl.state.task, { review_digest: "digest-b" }) })
            } else if (win.phase === 6) {
                win.check(!panel.verificationAcknowledged && !win.find(panel, "work.accept").enabled, "changed review resets acknowledgement")
                ctl.state = Object.assign({}, ctl.state, { task: Object.assign({}, ctl.state.task, { status: "failed", error: "Provider stopped before a result." }) })
            } else if (win.phase === 7) {
                win.check(win.find(panel, "work.new").visible && !win.find(panel, "work.accept").visible, "failed jobs show recovery, not acceptance")
                win.width = 480; win.height = 400
                ctl.state = Object.assign({}, ctl.state, { task: null })
            } else if (win.phase === 8) {
                var scroll = win.find(panel, "work.scroll")
                win.check(scroll.contentHeight > scroll.height, "short window scrolls to keep every control reachable")
                win.check(scroll.contentWidth <= scroll.width, "narrow window keeps content within its viewport")
                console.log("WorkSmoke: " + win.checks + " checks, " + win.failures + " failures")
                Qt.exit(win.failures ? 1 : 0)
            }
            win.phase++
        }
    }
}
