// DesktopPet scene smoke (SPEC-v0.3 §3.8) against the generated fixture pack: the
// roaming scene, hit box, sleep transitions, scheduler economy and Calm mode. The pet
// draws a bar-state.json document and has no transport at all. The shared-order bubbles
// and the click are in OverlayAgentSmoke.qml.
import QtQuick
import "../shared"
import "../shared/PetCatalog.js" as PetCatalog
import "fixtures/pets/PetRegistry.js" as FixtureRegistry

Item {
    id: test
    width: 1000; height: 700
    property int failures: 0
    property int checks: 0
    property int homes: 0
    readonly property string base: String(Qt.resolvedUrl("fixtures/pets/"))
    readonly property real hitScale: 112 / 192
    readonly property real now: 1792123900000

    // A bar-state.json (codepet.bar.v2) document: an armed, happy cat at stage 2.
    function doc(over) {
        var d = {
            schema: "codepet.bar.v2", state_version: 10, db_id: "db-" + "a".repeat(24),
            life: {life_id: "life-" + "1".repeat(24), ordinal: 1, name: "Ginger", appearance: "cat", stage: 2,
                   level: 2, xp: 120, status: "healthy", mode: "gentle", armed: true, pace: "steady",
                   paused: false, fainted: false, death_pending: false},
            meters: {hunger: 10000, thirst: 10000, mental_health: 10000, hygiene: 10000},
            needs: [{need: "hunger", action: "food", state: "ok"}, {need: "thirst", action: "water", state: "ok"},
                    {need: "mental_health", action: "play", state: "ok"}, {need: "hygiene", action: "wash", state: "ok"}],
            critical: null,
            care: {sleeping: false, sleep_remaining_ms: 120000, sleep_started_at_ms: null, sleep_due: false, bathroom_due: false},
            task: null, unseen_events: [], memorial: {count: 0, latest_name: null},
            enabled: true,
            agent: {state: "idle", apps: [], stale_at_ms: null, finished_seq: 0, finished_at_ms: null, finished_app: null,
                    finished_treat: false, finished_jar_full: false, hello_seq: 0, hello_at_ms: null, hello_app: null,
                    test: null, error_at_ms: null, error_app: null, helpers_busy: false},
            treats: {jar: 0, full: false}, clients: {}
        }
        for (var k in over || {}) d[k] = over[k]
        return d
    }
    function lifeWith(over) {
        var l = doc().life
        for (var k in over) l[k] = over[k]
        return l
    }

    DesktopPet { id: pet; width: test.width; height: test.height; fixedNowMs: test.now; onHomeRequested: test.homes += 1 }
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

    // Runs after every component completed (the scene initializes in its own onCompleted).
    Timer { interval: 0; running: true; onTriggered: test.runAll() }
    function runAll() {
        PetCatalog.install(FixtureRegistry.REGISTRY, test.base)
        var seen, t, i

        // ---- nothing to draw ----
        check(!pet.shown && !pet.active && pet.petHit.width === 0, "no file yet: nothing drawn, no input")
        check(!timerOf(pet).running && !find(pet, "desktopPet.wakeTimer").running, "no file yet: no timer runs")
        pet.barState = doc({life: null})
        check(!pet.shown && pet.face.state === "no_pet" && pet.petHit.width === 0, "no living pet: nothing on the desktop")
        pet.barState = doc({enabled: false})
        check(!pet.shown && pet.face.state === "off" && !timerOf(pet).running && !find(pet, "desktopPet.wakeTimer").running,
              "Off: nothing drawn and zero running timers (the host exits)")

        // ---- the roaming scene ----
        pet.barState = doc({})
        check(pet.shown && pet.petId === "cat" && pet.stage === 2 && spriteUrl(pet).indexOf(base + "cat/s2/") === 0,
              "appearance and stage come from the life")
        check(near(pet.petHit.x, pet.petX + 30 * hitScale) && near(pet.petHit.y, pet.floorY + 60 * hitScale)
              && near(pet.petHit.width, 112 * hitScale) && near(pet.petHit.height, 124 * hitScale),
              "petHit equals the scaled manifest hit box")
        check(timerOf(pet).running && timerOf(pet).interval >= 200, "a visible awake pet runs one scheduler")
        pet.walking = true; pet.destination = 600; pet.petX = 100; pet.timerInterval = 200
        pet.cosmeticTick()
        check(pet.petX === 104 && pet.displayState === "locomotion", "movement is a bounded local floor step (manifest 4 px/tick)")
        check(!pet.facingLeft && spriteUrl(pet).indexOf(base + "cat/s2/b0") === 0, "walking right uses unmirrored frames")
        pet.facingLeft = true
        pet.walking = true; pet.destination = 0; pet.petX = 300; pet.timerInterval = 200
        pet.cosmeticTick()
        check(pet.displayState === "locomotion" && pet.displayMirrored && find(pet, "desktopPet.sprite").mirrored,
              "walking left mirrors the frame")
        check(near(pet.petHit.x, pet.petX + 50 * hitScale), "a mirrored frame mirrors petHit about the canvas centre")
        pet.destination = 0; pet.petX = 300; pet.baseElapsedMs = 0
        seen = run(pet, 8)
        check(seen.join(",") === "locomotion:1,locomotion:2,locomotion:3,locomotion:0,locomotion:1,locomotion:2,locomotion:3,locomotion:0",
              "walk frames cycle through all four without skipping: " + seen.join(","))
        check(pet.petX === 300 - 4 * 8, "each 200 ms tick moves the manifest's 4 px")
        pet.stopMoving(); pet.facingLeft = false
        pet.width = 180; pet.height = 160
        pet.petX = 999; pet.clampPosition()
        check(pet.petX + pet.petSize <= pet.width, "position clamps to the scene after a resize")
        pet.width = test.width; pet.height = test.height

        // Aquatic: the habitat never roams; locomotion is an in-tank swim.
        pet.barState = doc({life: lifeWith({appearance: "goldfish", stage: 1})})
        pet.petX = 321
        pet.facingLeft = true
        pet.idleTicks = 0; pet.resting = true; pet.timerInterval = 200
        seen = run(pet, 30)
        check(pet.petX === 321, "aquatic petX stays fixed")
        check(seen.indexOf("locomotion:1") >= 0, "aquatic locomotion swims in the tank")
        check(!pet.displayMirrored && !find(pet, "desktopPet.sprite").mirrored, "aquatic frames never mirror")
        t = FixtureRegistry.REGISTRY.species.goldfish.geometry.water.outer
        check(near(pet.petHit.x, 321 + t[0] * hitScale) && near(pet.petHit.width, t[2] * hitScale),
              "aquatic petHit is the habitat (water.outer)")
        check(pet.careSet === "aquatic", "the goldfish uses the aquatic care icons")
        pet.facingLeft = false
        pet.stopMoving()
        pet.barState = doc({})

        // ---- sleep ----
        pet.barState = doc({care: {sleeping: true, sleep_remaining_ms: 120000, sleep_started_at_ms: null,
                                   sleep_due: false, bathroom_due: false}})
        check(pet.transientState === "sleep_enter" && pet.displayState === "sleep_enter", "falling asleep plays sleep_enter")
        check(timerOf(pet).running, "the scheduler runs to finish the transition while asleep")
        seen = run(pet, 3)
        check(pet.transientState === "" && pet.displayState === "sleep" && !timerOf(pet).running,
              "sleep_enter completes, then the sleeping pet is static: " + seen.join(","))
        pet.barState = doc({})
        check(pet.transientState === "sleep_exit", "waking plays sleep_exit")
        run(pet, 3)
        check(pet.transientState === "" && pet.displayState === "idle", "back to idle after waking")
        pet.barState = doc({life: lifeWith({fainted: true, status: "critical"})})
        check(pet.displayState === "sleep_enter" || pet.displayState === "sleep", "a fainted pet sleeps (with a bandage bubble)")
        pet.clearTransients()
        check(pet.displayState === "sleep" && pet.bubbleIcons.join(",") === "status/bandage.webp", "fainted: sleep pose and bandage")
        pet.barState = doc({})
        pet.clearTransients()

        // ---- a hopper moves only on its airborne frames ----
        var hopReg = JSON.parse(JSON.stringify(FixtureRegistry.REGISTRY))
        hopReg.species.cat.states.locomotion.airborneFrames = [2]
        delete hopReg.species.cat.locomotion.airborneFrames
        PetCatalog.install(hopReg, test.base)
        pet.barState = doc({life: lifeWith({appearance: "goldfish"})}) // re-read the gait
        pet.barState = doc({})
        pet.clearTransients()
        pet.walking = true; pet.facingLeft = false; pet.destination = 600; pet.petX = 100
        pet.baseElapsedMs = 0; pet.timerInterval = 200
        var hops = []
        for (i = 0; i < 8; i++) { var x0 = pet.petX; pet.cosmeticTick(); hops.push(pet.petX - x0) }
        check(hops.join(",") === "0,0,4,0,0,0,4,0", "a hopper moves only on its airborne frames: " + hops.join(","))
        pet.stopMoving()
        // Stage-aware input mask: a young pet uses its own (smaller) box.
        hopReg.species.cat.geometry.stageHitBoxes = {"1": [50, 90, 80, 94], "2": [40, 76, 100, 108], "3": [30, 60, 112, 124]}
        PetCatalog.install(hopReg, test.base)
        pet.petX = 200; pet.facingLeft = false
        pet.barState = doc({life: lifeWith({appearance: "goldfish"})})
        pet.barState = doc({life: lifeWith({stage: 1})})
        check(near(pet.petHit.x, 200 + 50 * hitScale) && near(pet.petHit.y, pet.floorY + 90 * hitScale)
              && near(pet.petHit.width, 80 * hitScale) && near(pet.petHit.height, 94 * hitScale),
              "a stage-1 pet's petHit is the scaled stage-1 box")
        pet.barState = doc({life: lifeWith({stage: 3})})
        check(near(pet.petHit.width, 112 * hitScale) && near(pet.petHit.height, 124 * hitScale),
              "growing to stage 3 switches to the stage-3 box")
        PetCatalog.install(FixtureRegistry.REGISTRY, test.base)
        pet.barState = doc({life: lifeWith({appearance: "goldfish"})})
        pet.barState = doc({})
        pet.clearTransients()

        // 200 look switches reference only the current pack.
        var looks = ["cat", "goldfish", "ghost"], foreign = 0
        for (i = 0; i < 200; i++) {
            var look = looks[i % 3], st = 1 + (i % 3)
            pet.barState = doc({life: lifeWith({appearance: look, stage: st})})
            if (spriteUrl(pet).indexOf(base + look + "/s" + st + "/") !== 0) foreign += 1
        }
        check(foreign === 0, "over 200 switches only the current pack is referenced")
        pet.barState = doc({})
        pet.clearTransients()

        // A lower transient queues behind a running one and plays afterwards.
        pet.startTransient("food", 0)
        pet.startTransient("celebrate", 0)
        check(pet.transientState === "food" && pet.queuedTransient && pet.queuedTransient.state === "celebrate",
              "celebrate queues behind a running higher transient")
        run(pet, 6)
        check(pet.transientState === "celebrate", "and plays when it ends")
        pet.clearTransients()

        // ---- Calm mode ----
        pet.reducedMotion = true
        check(!timerOf(pet).running, "Calm mode stops the scheduler")
        pet.startTransient("food", 0)
        check(timerOf(pet).running && pet.timerInterval === 2400 && pet.frame === 1
              && spriteUrl(pet) === base + "cat/s2/c00.webp",
              "Calm mode: a transient shows its reducedFrame and ends with one wake")
        pet.cosmeticTick()
        check(pet.transientState === "" && !timerOf(pet).running, "then the scheduler stops again")
        pet.reducedMotion = false

        // ---- inactive surface ----
        pet.surfaceActive = false
        check(pet.petHit.width === 0 && !timerOf(pet).running && !pet.petClicked(),
              "an inactive surface exposes no input, stops the scheduler and ignores clicks")
        pet.surfaceActive = true

        // ---- ten simulated idle minutes: <= 5 updates/s, long idle pauses ----
        pet.clearTransients()
        var elapsed = 0, wakes = 0, minInterval = 1e9, maxInterval = 0, offGrid = 0, stillWakes = 0, stillMs = 0
        while (elapsed < 600000) {
            var iv = pet.timerInterval
            minInterval = Math.min(minInterval, iv)
            maxInterval = Math.max(maxInterval, iv)
            if (iv % 200 !== 0) offGrid += 1
            if (!pet.walking) { stillWakes += 1; stillMs += iv }
            elapsed += iv
            wakes += 1
            pet.cosmeticTick()
        }
        console.log("idle wake rate: " + wakes + " wakes in 10 min; " + stillWakes + " while still over "
                    + Math.round(stillMs / 1000) + " s = " + (stillWakes / (stillMs / 60000)).toFixed(1) + "/min")
        check(minInterval >= 200 && offGrid === 0, "every wake is on the 200 ms grid (<= 5 updates/s)")
        check(maxInterval > 8000, "a whole idle pause is a single wake (max " + maxInterval + " ms)")
        check(wakes < 3000, "fewer wakes than a fixed 200 ms tick: " + wakes + " in 10 min")
        check(stillWakes / (stillMs / 60000) <= 24,
              "still-pet wakes stay low: " + (stillWakes / (stillMs / 60000)).toFixed(1) + "/min")
        // A single-frame hold (a need at "now" cannot roam) has nothing to animate.
        pet.barState = doc({needs: [{need: "hunger", action: "food", state: "empty"}]})
        pet.stopMoving(); pet.resting = false; pet.baseElapsedMs = 0; pet.updateInterval()
        var holdWakes = 0, holdMs = 0, holdFrames = {}
        while (holdMs < 120000) {
            holdMs += pet.timerInterval
            holdWakes += 1
            pet.cosmeticTick()
            holdFrames[pet.displayState + ":" + pet.frame] = true
        }
        check(Object.keys(holdFrames).join(",") === "needs_care:0" && holdWakes <= 16,
              "a single-frame hold wakes only once per pause: " + holdWakes + " wakes in 2 min")
        check(!pet.walking && !pet.canRoam, "a need at now never roams")
        check(homes === 0, "nothing here opened the popup")

        // A restart sees the same document and plays no transition.
        var restarted = petComponent.createObject(test, {barState: doc({care: {sleeping: true, sleep_remaining_ms: 0,
            sleep_started_at_ms: null, sleep_due: false, bathroom_due: false}}), fixedNowMs: test.now})
        check(restarted.transientState === "" && restarted.displayState === "sleep", "a restart while asleep plays no transition")
        restarted.destroy()

        PetCatalog.install()
        console.log("Overlay smoke: " + checks + " checks, " + failures + " failures")
        Qt.exit(failures > 0 ? 1 : 0)
    }
}
