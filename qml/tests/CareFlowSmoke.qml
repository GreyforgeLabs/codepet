// Test-only successful care host. Product preview remains immutable.
// Part 2 (`slow`/`flow`) covers the v0.2 choreography against the fixture pack: a
// pending request holds the approach frame, failures surface, double clicks and
// hide/reopen never re-send, a broken pack still commits care, and routines hold a
// presentation lock so looks cannot change mid-sequence.
import QtQuick
import "../shared"
import "../shared/PetCatalog.js" as PetCatalog
import "fixtures/pets/PetRegistry.js" as FixtureRegistry

Item {
    id: top
    width: 700; height: 900
    property int step: 0
    property int failures: 0
    property int step2: 0
    property bool part1Done: false
    property bool part2Done: false
    property string firstKey: ""
    property int mark: 0
    property int mark1: 0
    function finish() {
        if (!part1Done || !part2Done) return
        console.log("DONE failures=" + top.failures)
        Qt.exit(top.failures === 0 ? 0 : 3)
    }
    QtObject {
        id: host
        property bool busy: false
        property string lastError: ""
        property int actions: 0
        property var careActions: []
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
                    host.careActions = host.careActions.concat([params.action])
                    var saved = JSON.parse(JSON.stringify(host.snapshot))
                    var keys = params.action === "routine" ? ["hunger", "thirst", "mental_health", "hygiene"]
                        : [{food: "hunger", water: "thirst", play: "mental_health", wash: "hygiene"}[params.action]]
                    for (var i = 0; i < keys.length; i++) saved.meters[keys[i]] = 10000
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
            // The routine is one user click and one authoritative request; storage fills
            // every bar atomically. The per-step choreography afterwards sends nothing
            // (spec §1: no RPC launched in response to a timer).
            if (top.step === 1) care.startRoutine()
            if (top.step === 3) {
                top.check(host.actions === 1 && host.careActions[0] === "routine", "the routine is one pet.care {action: routine} request")
                top.check(host.snapshot.meters.hunger === 10000 && host.snapshot.meters.hygiene === 10000,
                          "storage fills every bar in that one request")
                top.check(care.careTrail.length === 1, "confirmed care appears in separate bounded activity trail")
                top.check(!host.snapshot.care.ready_to_work && care.readinessText === "Needs care", "care feedback never computes readiness locally")
                top.check(care.routineRunning && care.reactionSeq === 1 && care.lastReaction === "food",
                          "the choreography starts only after the confirmed response")
                top.mark1 = host.methods.length
            }
            if (top.step === 12) {
                top.check(host.methods.length === top.mark1 && host.actions === 1,
                          "the step-by-step choreography never sends a request")
                top.check(care.reactionSeq >= 2, "later steps play cosmetically")
                care.stopRoutine()
                top.check(!care.routineRunning, "Stop ends the choreography; confirmed care is kept")
            }
            if (top.step === 14) {
                host.snapshot = (function () { var x = JSON.parse(JSON.stringify(host.snapshot)); x.meters.thirst = 7000; return x })()
                care.startRoutine()
                care.visible = false
            }
            if (top.step === 28) {
                top.check(host.actions === 2 && host.careActions[1] === "routine" && !care.routineRunning,
                          "hiding stops the choreography and nothing is re-sent")
                top.check(host.methods.indexOf("task.start") === -1, "care routine never starts work")
                care.visible = true
                host.snapshot = (function () { var x = JSON.parse(JSON.stringify(host.snapshot)); x.meters.hygiene = 7000; return x })()
                host.busy = true // A deliberately slow unrelated Refresh.
                care.startRoutine()
            }
            if (top.step === 41) {
                top.check(host.actions === 2 && host.snapshot.meters.hygiene === 7000, "busy request never silently consumes a routine")
                top.check(!care.routineRunning, "a refused routine never strands a running routine")
                host.busy = false
                top.part1Done = true
                top.finish()
            }
        }
    }

    // ---- Part 2: delayed/failing host with v0.2 lives ----
    QtObject {
        id: slow
        property bool busy: false
        property string lastError: ""
        property int presentationLocks: 0
        property var calls: []
        property bool hold: false
        property bool failNext: false
        property var queue: []
        property var snapshot: null
        function count(method) { return calls.filter(function (c) { return c.method === method }).length }
        function last() { return calls[calls.length - 1] }
        function request(method, params, callback, requestId) {
            if (busy) {
                callback({ok: false, error: {code: "CLIENT_BUSY", message: "Another request is still running."}})
                return
            }
            busy = true
            calls = calls.concat([{method: method, params: params, id: requestId}])
            var fail = failNext && method !== "state.get"
            if (fail) failNext = false
            var respond = function () {
                slow.busy = false
                if (fail) {
                    slow.lastError = "INVALID_STATE: Not now."
                    callback({ok: false, error: {code: "INVALID_STATE", message: "Not now."}})
                } else if (method === "pet.care") {
                    var saved = JSON.parse(JSON.stringify(slow.snapshot))
                    var keys = params.action === "routine" ? ["hunger", "thirst", "mental_health", "hygiene"]
                        : [{food: "hunger", water: "thirst", play: "mental_health", wash: "hygiene"}[params.action]]
                    for (var i = 0; i < keys.length; i++) saved.meters[keys[i]] = 10000
                    saved.care.revision += 1
                    slow.snapshot = saved
                    callback({ok: true, result: {request_id: requestId, action: params.action, changed: true, meters: saved.meters, care: saved.care}})
                } else {
                    callback({ok: true, result: slow.snapshot})
                }
            }
            if (hold) queue = queue.concat([respond])
            else Qt.callLater(respond)
        }
        function release() {
            var q = queue
            queue = []
            hold = false
            for (var i = 0; i < q.length; i++) q[i]()
        }
    }
    function lifeSnap(over) {
        var s = {meters: {hunger: 7000, thirst: 7000, mental_health: 7000, hygiene: 7000},
            care: {revision: 1, sleeping: false, sleep_due: false, sleep_remaining_ms: 120000, bathroom_due: false,
                ready_to_work: false, readiness_reasons: ["Needs care"], welcome_care_available: false},
            life: {life_id: "life-1", name: "Ginger", appearance: "cat", stage: 1, status: "warning",
                fainted: false, death_pending: false}}
        for (var k in over || {}) s.life[k] = over[k]
        return s
    }
    function findIn(obj, name) {
        if (!obj) return null
        if (obj.objectName === name) return obj
        var kids = obj.children || []
        for (var i = 0; i < kids.length; i++) {
            var found = findIn(kids[i], name)
            if (found) return found
        }
        return null
    }
    function spriteSrc() { return String(top.findIn(flow, "petCare.sprite").currentSource) }
    PetCare { id: flow; width: 680; y: 0; host: slow; reducedMotion: false }
    Timer {
        interval: 150; repeat: true; running: true
        onTriggered: {
            top.step2 += 1
            var sprite = top.findIn(flow, "petCare.sprite")
            switch (top.step2) {
            case 1:
                PetCatalog.install(FixtureRegistry.REGISTRY, Qt.resolvedUrl("fixtures/pets/"))
                slow.snapshot = top.lifeSnap({})
                break
            case 2:
                top.check(flow.petId === "cat" && flow.canCare, "a living, non-pending life can be cared for")
                top.check(top.findIn(flow, "petCare.food").text === "Kibble", "care labels come from the species manifest")
                slow.hold = true
                flow.careClicked("food")
                top.firstKey = slow.last().id
                top.check(slow.count("pet.care") === 1 && flow.pendingAction === "food", "care is pending until the host answers")
                top.check(flow.spriteState === "food" && flow.spriteFrame === 0 && top.spriteSrc().slice(-15) === "cat/s1/b03.webp",
                          "pending holds frame 0 (approach): " + top.spriteSrc())
                flow.careClicked("food")
                flow.careClicked("water")
                top.check(slow.count("pet.care") === 1, "a double click sends one request")
                top.check(slow.busy && !flow.canCare && flow.reactionSeq === 0, "selection and further care are blocked while pending")
                flow.visible = false
                break
            case 3:
                flow.visible = true
                top.check(slow.count("pet.care") === 1, "hide then reopen does not re-send")
                slow.release()
                top.check(flow.pendingAction === "" && flow.reactionSeq === 1 && flow.lastReaction === "food",
                          "the reaction starts only after confirmation")
                top.check(flow.spriteState === "food", "the confirmed care animates")
                break
            case 5:
                top.check(slow.count("state.get") === 1 && slow.snapshot.meters.hunger === 10000, "confirmed care refreshes once")
                slow.failNext = true
                flow.careClicked("water")
                break
            case 7:
                top.check(flow.failureSeq === 1 && flow.lastError.indexOf("INVALID_STATE") === 0, "failure is surfaced as lastError")
                top.check(flow.feedback.indexOf("Care was not confirmed") === 0 && flow.reactionSeq === 1, "failure shows an error and no reaction")
                top.firstKey = slow.last().method === "state.get" ? slow.calls[slow.calls.length - 2].id : slow.last().id
                flow.careClicked("water")
                break
            case 9:
                var waterCalls = slow.calls.filter(function (c) { return c.method === "pet.care" && c.params.action === "water" })
                top.check(waterCalls.length === 2 && waterCalls[1].id === waterCalls[0].id, "a retry after failure reuses its key")
                top.check(flow.reactionSeq === 2 && flow.lastError === "", "the retry commits")
                // A broken pack: the placeholder shows and care still commits.
                slow.snapshot = (function () { var x = top.lifeSnap({appearance: "ghost"}); x.meters = slow.snapshot.meters; return x })()
                break
            case 10:
                top.check(sprite && sprite.showingPlaceholder && top.spriteSrc().indexOf("_placeholder.webp") > 0,
                          "asset failure shows the placeholder")
                flow.careClicked("play")
                break
            case 12:
                top.check(slow.snapshot.meters.mental_health === 10000 && flow.reactionSeq === 3, "and the care still commits")
                // Routines hold one presentation lock for their whole run.
                flow.startRoutine()
                top.check(flow.routineRunning && slow.presentationLocks === 1, "a running routine holds a presentation lock")
                flow.stopRoutine()
                flow.stopRoutine()
                top.check(slow.presentationLocks === 0, "stopping releases it exactly once")
                break
            case 14:
                slow.snapshot = (function () { var x = top.lifeSnap({}); x.meters = {hunger: 10000, thirst: 10000, mental_health: 10000, hygiene: 9000}; return x })()
                top.mark = slow.count("pet.care")
                flow.startRoutine()
                break
            case 15:
                top.check(flow.routineRunning && slow.presentationLocks === 1, "the lock is held across the routine's choreography")
                var routines = slow.calls.filter(function (c) { return c.method === "pet.care" && c.params.action === "routine" })
                top.check(routines.length >= 1 && slow.count("pet.care") === top.mark + 1, "one routine request per click")
                top.mark = slow.calls.length
                break
            case 30: // after the cosmetic step
                top.check(!flow.routineRunning && slow.presentationLocks === 0, "a completed routine releases its lock")
                top.check(slow.calls.length === top.mark, "the choreography sent nothing")
                slow.snapshot = (function () { var x = top.lifeSnap({}); x.life = null; return x })()
                top.check(!flow.canCare && !flow.canSleep && flow.readinessText === "Adopt a companion to start work",
                          "no life: care and sleep are unavailable")
                top.check(flow.spriteState === "memorial", "no life shows the memorial")
                slow.snapshot = top.lifeSnap({death_pending: true, status: "critical"})
                var before = slow.calls.length
                flow.careClicked("food")
                flow.perform("pet.sleep", {}, "sleep")
                top.check(!flow.canCare && !flow.canSleep && slow.calls.length === before, "a pending life refuses care and sleep")
                top.check(flow.readinessText.indexOf("grace time has run out") >= 0 && flow.readinessText.indexOf("Care opens") < 0
                          && flow.readinessText.indexOf("needs care") < 0,
                          "a pending end never promises that care can still help: " + flow.readinessText)
                break
            case 31:
                // A press long after an unconfirmed one is a new press: the old key would make
                // the backend replay its old receipt and apply no new care.
                slow.snapshot = top.lifeSnap({})
                slow.failNext = true
                top.mark = slow.calls.length
                flow.careClicked("wash")
                break
            case 33:
                var washes = slow.calls.slice(top.mark).filter(function (c) { return c.method === "pet.care" && c.params.action === "wash" })
                top.check(washes.length === 1 && flow.failureSeq >= 2 && flow.retryKey === washes[0].id,
                          "an unconfirmed wash keeps its key for a prompt retry")
                top.check(flow.retryExpiresAt > Date.now() && flow.retryExpiresAt <= Date.now() + 120000,
                          "the key is kept only for the short retry window")
                top.firstKey = washes[0].id
                flow.retryExpiresAt = Date.now() - 1 // the window has passed
                flow.careClicked("wash")
                break
            case 35:
                var washes2 = slow.calls.slice(top.mark).filter(function (c) { return c.method === "pet.care" && c.params.action === "wash" })
                top.check(washes2.length === 2 && washes2[1].id !== top.firstKey,
                          "a later press of the same button gets a new key, never a stale replay")
                top.check(slow.snapshot.meters.hygiene === 10000 && flow.retryKey === "" && flow.retryExpiresAt === 0,
                          "the new press commits and its key is dropped once confirmed")
                PetCatalog.install()
                top.part2Done = true
                top.finish()
            }
        }
    }
}
