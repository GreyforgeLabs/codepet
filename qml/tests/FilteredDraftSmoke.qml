// Verifies the editor reviews the saved sanitized instructions, not its original.
import QtQuick
import "../shared"

Item {
    id: top
    width: 700; height: 1000
    property int failures: 0
    property int step: 0
    QtObject {
        id: host
        property bool busy: false
        property string lastError: ""
        property var snapshot: ({care: {revision: 1, ready_to_work: true}, providers: [{id: "mock", label: "Mock", available: true}]})
        function request(method, params, callback) {
            busy = true
            Qt.callLater(function () {
                var result = {}
                if (method === "task.create")
                    result = {task_id: "filtered-draft", revision: 1, instructions: "Explain [REDACTED]"}
                else if (method === "task.preflight")
                    result = {task_id: "filtered-draft", revision: 1, preflight_token: "test-only", expires_at_ms: Date.now() + 10000, summary: {}}
                else if (method === "state.get")
                    result = host.snapshot
                host.busy = false
                callback({ok: true, result: result})
            })
        }
    }
    TaskForm { id: form; host: host; width: 680 }
    function check(condition, label) {
        console.log((condition ? "PASS " : "FAIL ") + label)
        if (!condition) failures += 1
    }
    Timer {
        interval: 150; repeat: true; running: true
        onTriggered: {
            top.step += 1
            if (top.step === 1) {
                form.setFields("mental_health", "mock", "/tmp/test", ["docs"], "Explain token=fixture-secret", "artifact", 0)
                form.saveAndPreflight()
            }
            if (top.step === 4) {
                top.check(form.draftParams().instructions === "Explain [REDACTED]", "editor adopts authoritative filtered instructions")
                top.check(form.formError.indexOf("filtered") >= 0, "filtering visibly disclosed before acknowledgement")
                top.check(form.preflightFresh && !form.dirty, "fresh preflight describes saved filtered revision")
                // v0.2 life gates mirror Rust's order: no life → death pending → critical.
                var base = host.snapshot
                top.check(form.readyToWork, "a v0.1-shaped ready snapshot (no life key) stays ready")
                host.snapshot = Object.assign({}, base, {life: null})
                top.check(!form.readyToWork && form.readinessText === "Adopt a companion to start work.", "no living companion blocks Start")
                host.snapshot = Object.assign({}, base, {life: {name: "Ginger", status: "critical", death_pending: true, fainted: false}})
                top.check(!form.readyToWork && form.readinessText.indexOf("current task") >= 0, "a pending death blocks new work")
                host.snapshot = Object.assign({}, base, {life: {name: "Ginger", status: "critical", death_pending: false, fainted: false},
                                                         lifecycle: {critical: {need: "thirst"}, needs: []}})
                top.check(!form.readyToWork && form.readinessText === "Ginger urgently needs water; care before starting work.",
                          "critical blocks Start with the need: " + form.readinessText)
                host.snapshot = Object.assign({}, base, {life: {name: "Ginger", status: "healthy", death_pending: false, fainted: false}})
                top.check(form.readyToWork, "a healthy life with ready care allows Start")
                console.log("DONE failures=" + top.failures)
                Qt.exit(top.failures === 0 ? 0 : 3)
            }
        }
    }
}
