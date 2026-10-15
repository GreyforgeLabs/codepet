// DesktopPet scene smoke (T5, SPEC §17.6) against the generated fixture pack.
// `legacy` runs on the immutable PreviewHost (v0.1 snapshots, no `life` key);
// `pet` runs on a scripted v0.2 host whose responses can be delayed or failed.
import QtQuick
import "../shared"
import "../standalone"
import "../shared/PetCatalog.js" as PetCatalog
import "fixtures/pets/PetRegistry.js" as FixtureRegistry

Item {
    id: test
    width: 1000; height: 700
    property int failures: 0
    property int checks: 0
    property int phase: 0
    property int requestBaseline: 0
    property int legacySleepBaseline: 0
    property int savedFrame: 0
    property int openedPanels: 0
    property int chooserOpened: 0
    property var restarted: null
    readonly property string base: String(Qt.resolvedUrl("fixtures/pets/"))
    readonly property real hitScale: 112 / 192

    PreviewHost { id: host }

    // Scripted v0.2 host. Responses arrive on the next event-loop turn, or are held
    // until release(); `failNext` turns the next mutation into an error.
    QtObject {
        id: fake
        property var snapshot: null
        property bool busy: false
        property string lastError: ""
        property int presentationLocks: 0
        property int seq: 0
        property var calls: []
        property bool hold: false
        property bool failNext: false
        property var queue: []
        signal taskAccepted(var result)
        function methods() { return calls.map(function (c) { return c.method }) }
        function count(method) { return calls.filter(function (c) { return c.method === method }).length }
        function request(method, params, callback, requestId) {
            if (busy) {
                if (callback) callback({ok: false, error: {code: "CLIENT_BUSY", message: "busy"}})
                return
            }
            seq += 1
            calls = calls.concat([{method: method, params: params, id: requestId}])
            busy = true
            var fail = failNext && method !== "state.get"
            if (fail) failNext = false
            var respond = function () {
                fake.busy = false
                var resp
                if (fail) {
                    resp = {ok: false, error: {code: "INVALID_STATE", message: "Not now."}}
                    fake.lastError = "INVALID_STATE: Not now."
                } else if (method === "state.get") {
                    resp = {ok: true, result: fake.snapshot}
                } else if (method === "pet.care") {
                    var saved = JSON.parse(JSON.stringify(fake.snapshot))
                    saved.meters[{food: "hunger", water: "thirst", play: "mental_health", wash: "hygiene", bathroom: "hygiene"}[params.action]] = 10000
                    fake.snapshot = saved
                    resp = {ok: true, result: {request_id: requestId, action: params.action, changed: true, meters: saved.meters, care: saved.care}}
                } else {
                    resp = {ok: true, result: {request_id: requestId, changed: true}}
                }
                if (callback) callback(resp)
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
    function snap(over) {
        var s = {
            schema_version: 3, state_version: 10,
            life: {life_id: "life-1", ordinal: 1, name: "Ginger", species: "cat", appearance: "cat", stage: 2,
                   status: "healthy", mode: "permadeath", armed: true, pace: "steady", paused: false,
                   fainted: false, death_pending: false, revision: 1},
            meters: {hunger: 7000, thirst: 7000, mental_health: 7000, hygiene: 7000},
            care: {revision: 1, sleeping: false, sleep_due: false, sleep_remaining_ms: 120000, bathroom_due: false,
                   ready_to_work: false, readiness_reasons: ["Needs care"], welcome_care_available: false},
            lifecycle: {needs: [], critical: null},
            active_task: null, unseen_events: [], memorial: {count: 0, latest: null}
        }
        for (var k in over || {}) s[k] = over[k]
        return s
    }
    function lifeWith(over) {
        var l = snap().life
        for (var k in over) l[k] = over[k]
        return l
    }

    DesktopPet { id: legacy; width: test.width; height: test.height; host: host; onTaskPanelRequested: test.openedPanels += 1 }
    DesktopPet { id: pet; width: test.width; height: test.height; host: fake; onChooserRequested: test.chooserOpened += 1 }
    Component { id: petComponent; DesktopPet { width: 600; height: 400 } }

    function check(value, label) {
        checks += 1
        if (!value) failures += 1
        console.log((value ? "PASS " : "FAIL ") + label)
    }
    function near(a, b) { return Math.abs(a - b) < 0.01 }
    function find(obj, name) {
        if (!obj) return null
        if (obj.objectName === name) return obj
        var kids = [].concat(obj.children || []).concat(obj.resources || [])
        for (var i = 0; i < kids.length; i++) {
            var found = find(kids[i], name)
            if (found) return found
        }
        if (obj.contentItem && kids.indexOf(obj.contentItem) === -1) return find(obj.contentItem, name)
        return null
    }
    function spriteUrl(scene) { return String(find(scene, "desktopPet.sprite").targetUrl) }
    function timerOf(scene) { return find(scene, "desktopPet.cosmeticTimer") }
    // Runs the scheduler for `ticks` wakes (as the Timer would) and returns the states seen.
    function run(scene, ticks) {
        var seen = []
        for (var i = 0; i < ticks; i++) {
            scene.cosmeticTick()
            seen.push(scene.displayState + ":" + scene.frame)
        }
        return seen
    }

    Component.onCompleted: {
        PetCatalog.install(FixtureRegistry.REGISTRY, test.base)
        fake.snapshot = test.snap({})
    }

    Timer {
        interval: 120; repeat: true; running: true
        onTriggered: {
            test.phase += 1
            var t, i, seen
            switch (test.phase) {
            case 1:
                // ---- legacy PreviewHost scene ----
                test.check(host.seq === 0, "legacy scene startup launches no RPC")
                test.check(legacy.menuHit.width === 0 && legacy.propsHit.width === 0, "collapsed menu and props take no input")
                test.check(!legacy.schedulerRunning, "no snapshot yet: the scheduler is idle")
                host.snapshot = host.buildSnapshot("idle")
                test.check(legacy.petId === "cat" && legacy.displayState === "idle", "a v0.1 snapshot (no life key) shows the migrated cat")
                test.check(test.near(legacy.petHit.width, 112 * test.hitScale) && test.near(legacy.petHit.x, legacy.petX + 30 * test.hitScale),
                           "legacy pet hit box is the scaled manifest box")
                legacy.selected = true
                test.check(legacy.menuHit.width === 260, "selecting pet exposes menu input")
                legacy.width = 180; legacy.height = 160
                test.check(legacy.menuHit.x >= 0 && legacy.menuHit.y >= 0 && legacy.menuHit.width <= 180 && legacy.menuHit.height <= 160, "short narrow scene clamps menu")
                legacy.petX = 999; legacy.clampPosition()
                test.check(legacy.petX + legacy.petSize <= legacy.width, "position clamps to screen after resize")
                legacy.width = test.width; legacy.height = test.height; legacy.selected = false
                legacy.walking = true; legacy.destination = 600; legacy.petX = 100; legacy.timerInterval = 200
                legacy.cosmeticTick()
                test.check(legacy.petX === 104 && legacy.displayState === "locomotion", "movement is a bounded local floor step (manifest 4 px/tick)")
                test.check(!legacy.facingLeft && test.spriteUrl(legacy).indexOf(test.base + "cat/s1/b0") === 0, "walking right uses unmirrored s1 frames")
                test.check(host.seq === 0, "movement never sends RPC")
                legacy.reducedMotion = true
                test.check(!legacy.walking && !test.timerOf(legacy).running, "reduced motion stops walking and the scheduler")
                legacy.reducedMotion = false
                legacy.surfaceActive = false
                legacy.selected = true
                legacy.requestCare("food")
                test.check(host.seq === 0, "inactive control cannot send care")
                test.check(legacy.petHit.width === 0 && legacy.menuHit.width === 0 && !test.timerOf(legacy).running,
                           "inactive surface exposes no input and stops the scheduler")
                legacy.surfaceActive = true; legacy.selected = true
                host.fixture = "ready"; host.snapshot = host.buildSnapshot("ready")
                host.busy = true
                test.requestBaseline = host.seq
                legacy.requestCare("food")
                test.check(host.seq === test.requestBaseline, "busy host rejects care without overlapping RPC")
                host.busy = false
                legacy.taskPanelRequested()
                test.check(test.openedPanels === 1 && host.seq === test.requestBaseline, "opening task panel emits signal without starting work")
                legacy.requestCare("food")
                break
            case 3:
                test.check(host.lastError.indexOf("PREVIEW_ONLY") === 0, "care remains an explicit host request and preview refuses mutation")
                test.check(host.snapshot.meters.hunger === 10000, "UI does not mutate authoritative meters")
                test.check(legacy.transientState === "error", "a refused request plays the error pose")
                test.check(legacy.transientDurationMs === 2000, "the error pose lasts 2 s")
                host.fixture = "sleeping"; host.snapshot = host.buildSnapshot("sleeping")
                test.check(legacy.transientState === "error" || legacy.transientState === "sleep_enter", "falling asleep is a transition")
                legacy.clearTransients()
                test.requestBaseline = host.seq
                legacy.requestCare("food")
                test.check(host.seq === test.requestBaseline, "sleeping snapshot disables explicit food")
                test.check(legacy.displayState === "sleep" && !test.timerOf(legacy).running, "asleep: static sleep pose, scheduler stopped")
                test.legacySleepBaseline = host.seq

                // ---- v0.2 scene ----
                test.check(fake.seq === 0, "DesktopPet startup sends no RPC")
                test.check(pet.petId === "cat" && pet.stage === 2 && test.spriteUrl(pet).indexOf(test.base + "cat/s2/") === 0,
                           "appearance and stage come from the life")
                test.check(test.near(pet.petHit.x, pet.petX + 30 * test.hitScale) && test.near(pet.petHit.y, pet.floorY + 60 * test.hitScale)
                           && test.near(pet.petHit.width, 112 * test.hitScale) && test.near(pet.petHit.height, 124 * test.hitScale),
                           "petHit equals the scaled manifest hit box")
                test.check(pet.propsHit.width === 0 && pet.propsHit.height === 0, "propsHit is always empty")
                pet.facingLeft = true
                pet.walking = true; pet.destination = 0; pet.petX = 300; pet.timerInterval = 200
                pet.cosmeticTick()
                test.check(pet.displayState === "locomotion" && pet.displayMirrored && test.find(pet, "desktopPet.sprite").mirrored,
                           "walking left mirrors the frame")
                test.check(test.near(pet.petHit.x, pet.petX + 50 * test.hitScale), "a mirrored frame mirrors petHit about the canvas centre")
                pet.destination = 0; pet.petX = 300
                seen = test.run(pet, 8)
                test.check(seen.join(",") === "locomotion:2,locomotion:3,locomotion:0,locomotion:1,locomotion:2,locomotion:3,locomotion:0,locomotion:1",
                           "walk frames cycle through all four without skipping: " + seen.join(","))
                test.check(pet.petX === 300 - 4 * 8, "each 200 ms tick moves the manifest's 4 px")
                pet.stopMoving(); pet.facingLeft = false
                test.check(test.timerOf(pet).running && test.timerOf(pet).interval >= 200, "visible awake pet runs one scheduler")
                // Aquatic: the habitat never roams; locomotion is an in-tank swim.
                fake.snapshot = test.snap({life: test.lifeWith({appearance: "goldfish", stage: 1})})
                pet.petX = 321
                pet.facingLeft = true
                pet.idleTicks = 0; pet.resting = true; pet.timerInterval = 200
                seen = test.run(pet, 30)
                test.check(pet.petX === 321, "aquatic petX stays fixed")
                test.check(seen.indexOf("locomotion:1") >= 0, "aquatic locomotion swims in the tank")
                test.check(!pet.displayMirrored && !test.find(pet, "desktopPet.sprite").mirrored, "aquatic frames never mirror")
                t = FixtureRegistry.REGISTRY.species.goldfish.geometry.water.outer
                test.check(test.near(pet.petHit.x, 321 + t[0] * test.hitScale) && test.near(pet.petHit.width, t[2] * test.hitScale),
                           "aquatic petHit is the habitat (water.outer)")
                pet.facingLeft = false
                fake.snapshot = test.snap({})
                // Sleep transitions.
                fake.snapshot = test.snap({care: {revision: 2, sleeping: true, sleep_due: true, sleep_remaining_ms: 120000}})
                test.check(pet.transientState === "sleep_enter" && pet.displayState === "sleep_enter", "falling asleep plays sleep_enter")
                test.check(test.timerOf(pet).running, "the scheduler runs to finish the transition while asleep")
                seen = test.run(pet, 3)
                test.check(pet.transientState === "" && pet.displayState === "sleep" && !test.timerOf(pet).running,
                           "sleep_enter completes, then the sleeping pet is static: " + seen.join(","))
                fake.snapshot = test.snap({})
                test.check(pet.transientState === "sleep_exit", "waking plays sleep_exit")
                test.run(pet, 3)
                test.check(pet.transientState === "" && pet.displayState === "idle", "back to idle after waking")
                // Pending / confirmed care choreography.
                pet.selected = true
                fake.hold = true
                test.requestBaseline = fake.seq
                pet.requestCare("food")
                test.check(fake.seq === test.requestBaseline + 1 && fake.calls[fake.calls.length - 1].method === "pet.care",
                           "care sends one pet.care")
                test.check(pet.pendingCare === "food" && pet.displayState === "food" && pet.frame === 0
                           && test.spriteUrl(pet) === test.base + "cat/s2/b03.webp", "pending care holds the approach frame")
                test.check(!test.timerOf(pet).running, "a pending hold needs no wakes")
                pet.requestCare("food")
                test.check(fake.seq === test.requestBaseline + 1, "a double click sends nothing more")
                test.check(test.spriteUrl(pet).indexOf("b04") < 0, "the confirmed-result frame is not shown before the response")
                fake.release()
                test.check(pet.pendingCare === "" && pet.transientState === "food"
                           && test.find(pet, "desktopPet.careController").reactionSeq === 1,
                           "the confirmed response starts the care reaction")
                seen = test.run(pet, 6)
                test.check(seen.join(",") === "food:1,food:2,food:3,food:4,food:5,idle:0", "care reaction plays every frame once, then returns: " + seen.join(","))
                break
            case 5:
                test.check(host.seq === test.legacySleepBaseline && legacy.displayState === "sleep",
                           "sleep display never wakes, refreshes or starts work")
                test.check(fake.count("pet.care") === 1 && fake.count("state.get") === 1, "one care, one confirming refresh")
                // A failed care shows the error pose for 2 s.
                fake.failNext = true
                pet.selected = true
                pet.requestCare("water")
                break
            case 7:
                test.check(pet.transientState === "error" && pet.displayState === "error" && pet.transientDurationMs === 2000,
                           "a failed request plays error for 2 s")
                test.check(test.find(pet, "desktopPet.careController").lastError.indexOf("INVALID_STATE") === 0, "the failure is kept as lastError")
                pet.timerInterval = 1000
                test.run(pet, 2)
                test.check(pet.transientState === "", "the error pose ends")
                // Celebrations: unseen growth shows a badge, celebrates once on click.
                test.requestBaseline = fake.seq
                fake.snapshot = test.snap({unseen_events: [{seq: 5, kind: "evolved", stage: 3, at_ms: 0}]})
                test.check(pet.growthBadge && pet.transientState === "" && fake.seq === test.requestBaseline,
                           "growth arriving from elsewhere shows a static badge, no celebration, no RPC")
                pet.selected = false
                pet.petClicked()
                test.check(pet.transientState === "celebrate", "a click celebrates unseen growth")
                test.check(fake.count("pet.events_seen") === 1 && fake.calls[fake.calls.length - 1].params.through_seq === 5
                           && fake.calls[fake.calls.length - 1].params.life_id === "life-1", "and sends one pet.events_seen")
                break
            case 9:
                pet.petClicked()
                pet.celebrateFromAcceptance({unseen_events: [{seq: 5, kind: "evolved", stage: 3}]})
                test.check(fake.count("pet.events_seen") === 1, "the same growth is never acknowledged twice")
                pet.clearTransients()
                pet.celebrateFromAcceptance({})
                test.check(pet.transientState === "", "already celebrated growth never replays")
                fake.taskAccepted({unseen_events: [{seq: 6, kind: "evolved", stage: 3}]})
                test.check(pet.transientState === "celebrate", "this UI's acceptance response celebrates new growth")
                break
            case 11:
                test.check(fake.count("pet.events_seen") === 2 && fake.calls[fake.calls.length - 1].params.through_seq === 6,
                           "and acknowledges it exactly once")
                // A restart: seen events never celebrate; unseen ones wait for a click.
                fake.snapshot = test.snap({})
                test.requestBaseline = fake.seq
                test.restarted = petComponent.createObject(test, {host: fake})
                test.check(test.restarted.transientState === "" && fake.seq === test.requestBaseline, "a restart with seen events does not celebrate")
                test.restarted.destroy()
                fake.snapshot = test.snap({unseen_events: [{seq: 7, kind: "evolved", stage: 3}]})
                test.restarted = petComponent.createObject(test, {host: fake})
                test.check(test.restarted.transientState === "" && test.restarted.growthBadge && fake.seq === test.requestBaseline,
                           "a restart with unseen growth only shows the badge")
                test.restarted.destroy()
                fake.snapshot = test.snap({})
                // Choosing a look: one pet.select and nothing else.
                test.requestBaseline = fake.seq
                fake.presentationLocks = 1
                test.check(!pet.selectLook("goldfish") && fake.seq === test.requestBaseline, "a running routine blocks look changes")
                fake.presentationLocks = 0
                fake.snapshot = test.snap({life: test.lifeWith({death_pending: true, status: "critical"})})
                test.check(!pet.selectLook("goldfish") && !pet.canSelectLook, "a pending life cannot change looks")
                fake.snapshot = test.snap({})
                test.check(!pet.selectLook("dragon") && !pet.selectLook("cat"), "unknown or current looks send nothing")
                test.check(pet.selectLook("goldfish"), "a valid look is requested")
                test.check(fake.seq === test.requestBaseline + 1 && fake.calls[fake.calls.length - 1].method === "pet.select"
                           && fake.calls[fake.calls.length - 1].params.pet_id === "goldfish"
                           && fake.calls[fake.calls.length - 1].params.life_id === "life-1", "selecting a species sends only pet.select")
                break
            case 13:
                test.check(fake.seq === test.requestBaseline + 1, "no refresh follows a look change")
                // The new look arrives through the snapshot (bar-state merge).
                fake.snapshot = test.snap({life: test.lifeWith({appearance: "goldfish", stage: 3})})
                test.check(test.spriteUrl(pet).indexOf(test.base + "goldfish/s3/") === 0, "the merged snapshot switches the pack")
                // 200 look switches reference only the current pack.
                var looks = ["cat", "goldfish", "ghost"], foreign = 0
                test.requestBaseline = fake.seq
                for (i = 0; i < 200; i++) {
                    var look = looks[i % 3], st = 1 + (i % 3)
                    fake.snapshot = test.snap({life: test.lifeWith({appearance: look, stage: st})})
                    if (test.spriteUrl(pet).indexOf(test.base + look + "/s" + st + "/") !== 0) foreign += 1
                }
                test.check(foreign === 0, "over 200 switches only the current pack is referenced")
                test.check(fake.seq === test.requestBaseline, "switching looks in the snapshot issues no request")
                fake.snapshot = test.snap({})
                // A lower transient queues behind a running one and plays afterwards.
                pet.startTransient("food", 0)
                pet.startTransient("celebrate", 0)
                test.check(pet.transientState === "food" && pet.queuedTransient && pet.queuedTransient.state === "celebrate",
                           "celebrate queues behind a care reaction")
                test.run(pet, 6)
                test.check(pet.transientState === "celebrate", "and plays when the care reaction ends")
                pet.clearTransients()
                // Reduced motion: transients still finish, with one wake and a static frame.
                pet.reducedMotion = true
                test.check(!test.timerOf(pet).running, "reduced motion stops the scheduler")
                pet.startTransient("food", 0)
                test.check(test.timerOf(pet).running && pet.timerInterval === 2400 && pet.frame === 1
                           && test.spriteUrl(pet) === test.base + "cat/s2/c00.webp",
                           "reduced motion: a transient shows its reducedFrame and ends with one wake")
                pet.cosmeticTick()
                test.check(pet.transientState === "" && !test.timerOf(pet).running, "then the scheduler stops again")
                pet.reducedMotion = false
                // Ten simulated idle minutes: no request, ≤ 5 updates/s, long idle pauses.
                test.requestBaseline = fake.seq
                pet.selected = false
                pet.clearTransients()
                var elapsed = 0, wakes = 0, minInterval = 1e9, maxInterval = 0, offGrid = 0
                while (elapsed < 600000) {
                    var iv = pet.timerInterval
                    minInterval = Math.min(minInterval, iv)
                    maxInterval = Math.max(maxInterval, iv)
                    if (iv % 200 !== 0) offGrid += 1
                    elapsed += iv
                    wakes += 1
                    pet.cosmeticTick()
                }
                test.check(fake.seq === test.requestBaseline, "no requests during 10 simulated idle minutes")
                test.check(minInterval >= 200 && offGrid === 0, "every wake is on the 200 ms grid (≤ 5 updates/s)")
                test.check(maxInterval >= 2000, "idle pauses switch to one long wake (max " + maxInterval + " ms)")
                test.check(wakes < 3000, "fewer wakes than a fixed 200 ms tick: " + wakes + " in 10 min")
                // No life: memorial, nothing to care for, Adopt offered.
                fake.snapshot = test.snap({life: null, lifecycle: null})
                test.check(pet.displayState === "memorial" && test.spriteUrl(pet) === test.base + "_memorial.webp", "no life renders the memorial")
                test.check(!test.timerOf(pet).running, "a memorial needs no scheduler")
                test.check(!test.find(pet, "desktopPet.careController").canCare && !test.find(pet, "desktopPet.careController").canSleep,
                           "no life: care and sleep are unavailable")
                pet.selected = true
                test.requestBaseline = fake.seq
                pet.requestCare("food")
                test.check(fake.seq === test.requestBaseline, "no life: care sends nothing")
                test.check(test.find(pet, "desktopPet.adopt").visible && !test.find(pet, "desktopPet.chooseLook").visible,
                           "the menu offers Adopt instead of Choose look")
                test.check(test.find(pet, "desktopPet.status").text.indexOf("No companion") === 0, "the status line is calm")
                test.check(test.near(pet.petHit.width, 112), "the memorial stays clickable")
                fake.snapshot = test.snap({life: test.lifeWith({status: "critical", fainted: true, mode: "gentle"}),
                                           lifecycle: {needs: [], critical: {need: "thirst", action: "water"}}})
                test.check(pet.displayState === "needs_care", "critical or fainted shows needs_care")
                test.check(test.find(pet, "desktopPet.status").text === "Ginger fainted from thirst. Water will revive them; work waits until then.",
                           "the menu status uses the fainted template")
                fake.snapshot = test.snap({active_task: {state: "running"}})
                test.check(pet.displayState === "working", "a running task shows working")
                fake.snapshot = test.snap({active_task: {state: "review_ready"}})
                test.check(pet.displayState === "waiting", "a review shows waiting")
                fake.snapshot = test.snap({active_task: {state: "recovery_required"}})
                test.check(pet.displayState === "error", "recovery_required shows error")
                fake.snapshot = test.snap({})
                test.find(pet, "desktopPet.chooseLook").clicked()
                test.check(test.chooserOpened === 1, "Choose look… asks the host for the chooser")
                PetCatalog.install()
                console.log("Overlay smoke: " + test.checks + " checks, " + test.failures + " failures")
                Qt.exit(test.failures > 0 ? 1 : 0)
            }
        }
    }
}
