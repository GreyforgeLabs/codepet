// Test-only successful care host. Product preview remains immutable.
import QtQuick
import "../shared"

Item {
    id: top
    width: 700; height: 900
    property int step: 0
    property int failures: 0
    QtObject {
        id: host
        property bool busy: false
        property string lastError: ""
        property int actions: 0
        property var methods: []
        property var snapshot: ({meters: {hunger: 7000, thirst: 7000, mental_health: 7000, hygiene: 7000},
            care: {revision: 1, sleeping: false, sleep_due: false, sleep_remaining_ms: 120000, bathroom_due: false,
                ready_to_work: false, readiness_reasons: ["Needs care"], welcome_care_available: true}})
        function request(method, params, callback, requestId) {
            busy = true
            methods = methods.concat([method])
            Qt.callLater(function () {
                var result = host.snapshot
                if (method === "pet.care") {
                    host.actions += 1
                    var saved = JSON.parse(JSON.stringify(host.snapshot))
                    var key = {food: "hunger", water: "thirst", play: "mental_health", wash: "hygiene"}[params.action]
                    saved.meters[key] = 10000
                    saved.care.revision += 1
                    host.snapshot = saved
                    result = {request_id: requestId, action: params.action, changed: true, meters: saved.meters, care: saved.care}
                }
                host.busy = false
                callback({ok: true, result: result})
            })
        }
    }
    PetCare { id: care; width: 680; host: host; reducedMotion: true }
    function check(condition, label) {
        console.log((condition ? "PASS " : "FAIL ") + label)
        if (!condition) failures += 1
    }
    Timer {
        interval: 200; repeat: true; running: true
        onTriggered: {
            top.step += 1
            if (top.step === 1) care.startRoutine()
            if (top.step === 3) {
                top.check(host.actions === 1 && host.snapshot.meters.hunger === 10000, "routine awaits confirmed first care action")
                top.check(care.careTrail.length === 1, "confirmed care appears in separate bounded activity trail")
                top.check(!host.snapshot.care.ready_to_work && care.readinessText === "Needs care", "care feedback never computes readiness locally")
                care.stopRoutine()
            }
            if (top.step === 14) {
                top.check(host.actions === 1 && !care.routineRunning, "Stop prevents later routine actions while retaining confirmed care")
                care.startRoutine()
            }
            if (top.step === 16) {
                top.check(host.actions === 2 && host.snapshot.meters.thirst === 10000, "new routine skips already full bar")
                care.visible = false
            }
            if (top.step === 28) {
                top.check(host.actions === 2 && !care.routineRunning, "hiding cancels remaining routine interactions")
                top.check(host.methods.indexOf("task.start") === -1, "care routine never starts work")
                care.visible = true
                care.startRoutine()
            }
            if (top.step === 30) {
                host.busy = true // A deliberately slow unrelated Refresh during the pause.
            }
            if (top.step === 41) {
                top.check(host.actions === 3 && host.snapshot.meters.hygiene === 7000, "busy request never silently consumes the next care step")
                top.check(!care.routineRunning, "busy between actions does not strand a running routine")
                top.check(care.feedback.indexOf("busy") >= 0, "busy stop is visibly explained with a recovery action")
                host.busy = false
                console.log("DONE failures=" + top.failures)
                Qt.exit(top.failures === 0 ? 0 : 3)
            }
        }
    }
}
