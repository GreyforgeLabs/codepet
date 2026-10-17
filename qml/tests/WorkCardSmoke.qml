pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Window
import "../shared"

Window {
    id: win
    width: 360; height: 560; visible: true; color: "#11151b"
    property int failures: 0
    property int checks: 0
    property int phase: 0
    property int reviews: 0
    property int homes: 0
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
        property string error: ""
        property var pet: ({name: "Pengu", appearance: "penguin", stage: 1, level: 2, xp: 123, adopted_at_ms: Date.now()-43*3600000})
        property var calls: []
        property var state: ({readiness: {ready: true, reason: "Ready", energy: 100}, provider: {available: true}, scout: {enabled: true, next_due_ms: 0}, task: null})
        function fix() { calls = calls.concat([{method: "fix"}]) }
        function action(method, params) { calls = calls.concat([{method: method, params: params}]) }
    }
    WorkCard {
        id: card
        x: 40; y: 20; width: 280; height: Math.min(implicitHeight, win.height-40)
        controller: ctl
        onReviewRequested: win.reviews++
        onHomeRequested: win.homes++
    }
    Timer {
        interval: 100; repeat: true; running: true
        onTriggered: {
            if (win.phase === 0) {
                win.check(card.heading === "Your local repo buddy", "idle card has no folder or task form")
                win.check(win.find(card, "workCard.stats").text === "Lv 2 · 123 XP · 1 day old", "level, exact XP and durable age stay visible")
                win.check(!win.find(card, "workCard.primary").visible, "idle card does not invent a Fix action")
                ctl.state = Object.assign({}, ctl.state, {task: {id: "scout-1", source_kind: "scout", status: "scouted", has_finding: true, project: "/fixture/repo", review_digest: "digest", finding: {title: "<b>An empty input may fail</b>", evidence: "Unverified evidence", path: "src/main.rs", line: 3, check: "Proposed check"}}})
            } else if (win.phase === 1) {
                win.check(card.heading === "Suspected bug · repo", "candidate is explicitly suspected")
                win.check(win.find(card, "workCard.message").textFormat === Text.PlainText, "finding markup stays literal")
                win.find(card, "workCard.primary").tapped()
                win.check(ctl.calls.length === 1 && ctl.calls[0].method === "fix", "Fix uses controller bound-finding action")
                card.expanded = true; win.height = 200
            } else if (win.phase === 2) {
                win.check(card.implicitHeight > card.height, "short expanded card scrolls instead of clipping controls")
                ctl.state = Object.assign({}, ctl.state, {task: {id: "fix-1", source_kind: "scout_fix", status: "running", project: "/fixture/repo", progress: "Checking the candidate"}})
            } else if (win.phase === 3) {
                win.find(card, "workCard.primary").tapped()
                win.check(ctl.calls[1].method === "cancel" && ctl.calls[1].params.task_id === "fix-1", "Stop uses active task identity")
                ctl.state = Object.assign({}, ctl.state, {task: Object.assign({}, ctl.state.task, {status: "review"})})
            } else if (win.phase === 4) {
                win.find(card, "workCard.primary").tapped()
                win.check(win.reviews === 1, "Review opens explicit result review, never applies")
                card.expanded = true
                win.find(card, "workCard.checks").tapped()
                win.check(ctl.calls[2].method === "scout-settings" && ctl.calls[2].params.enabled === false, "automatic checks have a pause control")
                ctl.pet = {name: "Pengu", level: 1, xp: 0}
            } else if (win.phase === 5) {
                win.check(win.find(card, "workCard.stats").text.indexOf("Age unavailable") >= 0, "old export never invents a birth time")
                ctl.state = Object.assign({}, ctl.state, {task: Object.assign({}, ctl.state.task, {status: "applying"})})
            } else if (win.phase === 6) {
                win.check(win.find(card, "workCard.primary").text === "Review", "interrupted apply stays reachable through Review")
                win.find(card, "workCard.primary").tapped()
                win.check(win.reviews === 2 && ctl.calls.length === 3, "interrupted apply opens review without sending cancel")
                ctl.state = Object.assign({}, ctl.state, {task: null, scout: {enabled: true, next_due_ms: 1000, notice: "The last bug check could not start."}})
            } else if (win.phase === 7) {
                win.check(win.find(card, "workCard.message").text === "The last bug check could not start.", "preflight refusal remains visible without a task")
                console.log("WorkCardSmoke: " + win.checks + " checks, " + win.failures + " failures")
                Qt.exit(win.failures ? 1 : 0)
            }
            win.phase++
        }
    }
}
