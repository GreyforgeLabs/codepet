// The desktop pet and the coding apps (SPEC-v0.3 §3.8, §6.4, K5, K9) with the shipped
// art and icons: the overlay follows the shared order of petstate.js (bubbles and
// animations), celebrates once, and a click always hops, shows a heart and asks for Home.
// Every priority vector is replayed against petstate.js in test_petstate.mjs; this smoke
// checks that the scene draws what the face says.
import QtQuick
import "../shared"

Item {
    id: test
    width: 1000; height: 700
    property int failures: 0
    property int checks: 0
    property int homes: 0
    property int phase: 0
    readonly property real now: 1792123900000

    function doc(over) {
        var d = {
            schema: "codepet.bar.v2", state_version: 10, db_id: "db-" + "a".repeat(24),
            life: {life_id: "life-" + "1".repeat(24), ordinal: 1, name: "Ginger", appearance: "cat", stage: 1,
                   level: 1, xp: 0, status: "healthy", mode: "gentle", armed: true, pace: "steady",
                   paused: false, fainted: false, death_pending: false},
            meters: {hunger: 10000, thirst: 10000, mental_health: 10000, hygiene: 10000},
            needs: [{need: "hunger", action: "food", state: "ok"}, {need: "thirst", action: "water", state: "ok"},
                    {need: "mental_health", action: "play", state: "ok"}, {need: "hygiene", action: "wash", state: "ok"}],
            critical: null,
            care: {sleeping: false, sleep_remaining_ms: 120000, sleep_started_at_ms: null, sleep_due: false, bathroom_due: false},
            task: null, unseen_events: [], memorial: {count: 0, latest_name: null},
            enabled: true, agent: agent({}), treats: {jar: 0, full: false}, clients: {}
        }
        for (var k in over || {}) d[k] = over[k]
        return d
    }
    function agent(over) {
        var a = {state: "idle", apps: [], stale_at_ms: null, finished_seq: 0, finished_at_ms: null, finished_app: null,
                 finished_treat: false, finished_jar_full: false, hello_seq: 0, hello_at_ms: null, hello_app: null,
                 test: null, error_at_ms: null, error_app: null, helpers_busy: false}
        for (var k in over || {}) a[k] = over[k]
        return a
    }
    function app(id, state, since) {
        return {id: id, state: state, since_ms: since || now - 60000, stale_at_ms: now + 540000}
    }
    function lifeWith(over) {
        var l = doc().life
        for (var k in over) l[k] = over[k]
        return l
    }
    function needs(action, state) {
        var n = doc().needs
        for (var i = 0; i < n.length; i++) if (n[i].action === action) n[i].state = state
        return n
    }

    DesktopPet { id: pet; width: test.width; height: test.height; fixedNowMs: test.now; onHomeRequested: test.homes += 1 }
    // A second scene on the real clock, for the single-shot wake timer.
    DesktopPet { id: live; width: 600; height: 400 }

    function check(value, label) {
        checks += 1
        if (!value) failures += 1
        console.log((value ? "PASS " : "FAIL ") + label)
    }
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
    function icons(scene) { return scene.bubbleIcons.join(",") }
    // Every icon the bubble names exists in the shipped icon set.
    function iconsLoad(scene) {
        var row = find(scene, "desktopPet.bubble")
        if (!row || !row.visible) return scene.bubbleIcons.length === 0
        var ok = true
        for (var i = 0; i < scene.bubbleIcons.length; i++) {
            var probe = Qt.createQmlObject('import QtQuick; Image { asynchronous: false }', test)
            probe.source = Qt.resolvedUrl("../assets/icons/" + scene.bubbleIcons[i])
            if (probe.status !== Image.Ready) ok = false
            probe.destroy()
        }
        return ok
    }
    function show(over) {
        pet.clearTransients()
        pet.heartMsLeft = 0
        pet.barState = doc(over)
        pet.clearTransients()
    }

    Timer { interval: 0; running: true; onTriggered: test.runAll() }
    function runAll() {
        // ---- the shared order reaches the scene ----
        show({agent: agent({state: "working", apps: [app("claude-code", "working")], stale_at_ms: now + 540000})})
        check(pet.face.state === "working" && pet.displayState === "working", "an app working: the working loop")
        check(icons(pet) === "apps/app_claude_code.webp" && iconsLoad(pet), "bubble = the app tile")
        check(!pet.canRoam && !pet.walking, "the working pet sits")
        show({agent: agent({state: "working", stale_at_ms: now + 540000,
                            apps: [app("claude-code", "working", now - 9000), app("codex", "working", now - 8000),
                                   app("opencode", "working", now - 7000), app("grok-cli", "working", now - 6000)]})})
        check(icons(pet) === "apps/app_claude_code.webp,apps/app_codex.webp,apps/app_opencode.webp", "at most three app tiles")
        var busy = {state: "waiting", stale_at_ms: now + 540000,
                    apps: [app("claude-code", "working"), app("codex", "waiting")]}
        show({agent: agent(busy), needs: needs("food", "empty")})
        check(pet.face.state === "waiting" && pet.displayState === "waiting", "an app needs you beats a need at now and working")
        check(icons(pet) === "status/bubble_wait.webp,apps/app_codex.webp" && iconsLoad(pet), "a ! bubble with the waiting app's tile")
        var bobs = {}
        for (var i = 0; i < 6; i++) { pet.cosmeticTick(); bobs[pet.bobOffset] = true }
        check(bobs["-3"] === true && bobs["0"] === true, "the waiting pet bobs gently")
        pet.reducedMotion = true
        check(pet.bobOffset === 0, "no bob in Calm mode")
        pet.reducedMotion = false
        show({needs: needs("food", "empty"), agent: agent({state: "working", apps: [app("grok-cli", "working")],
                                                          stale_at_ms: now + 540000})})
        check(pet.face.state === "need_now" && pet.displayState === "needs_care", "a need at now beats working")
        check(icons(pet) === "care-floor/food.webp" && iconsLoad(pet), "bubble = the need's care icon")
        show({life: lifeWith({appearance: "robot", name: "Bolt"}), needs: needs("food", "critical")})
        check(icons(pet) === "care-robot/food.webp" && iconsLoad(pet), "a robot's need uses the robot icons")
        show({life: lifeWith({appearance: "goldfish"}), needs: needs("wash", "low")})
        check(pet.face.state === "need_soon" && icons(pet) === "care-aquatic/wash.webp" && iconsLoad(pet),
              "a goldfish's need at soon uses the aquatic icons")
        check(pet.canRoam, "a need at soon still roams")
        show({life: lifeWith({fainted: true, status: "critical"}), agent: agent(busy)})
        check(pet.face.state === "fainted" && pet.displayState === "sleep" && icons(pet) === "status/bandage.webp",
              "fainted beats an app that needs you: sleep + bandage")
        show({agent: agent({helpers_busy: true})})
        check(pet.face.state === "idle" && icons(pet) === "status/robot_helpers.webp" && iconsLoad(pet),
              "background helpers: a small robot picture, no face change")
        show({})
        check(pet.face.state === "idle" && icons(pet) === "" && pet.canRoam, "idle: wanders, no bubble")
        check(!find(pet, "desktopPet.bubble").visible, "no bubble drawn when there is nothing to say")

        // ---- oops: the error frame for 3 s, then idle with no bubble ----
        show({agent: agent({error_at_ms: now - 1000, error_app: "grok-cli"})})
        check(pet.face.state === "oops" && pet.displayState === "error", "oops: the error frame")
        pet.fixedNowMs = now + 2000
        check(pet.face.overlay === "idle" && pet.displayState === "idle", "after 3 s the pet is idle again")
        pet.fixedNowMs = now

        // ---- celebrations play once and move the face on ----
        var fin = agent({finished_seq: 5, finished_at_ms: now - 24000, finished_app: "claude-code", finished_treat: true})
        pet.clearTransients()
        pet.barState = doc({agent: fin, treats: {jar: 1, full: false}})
        check(pet.transientState === "celebrate" && pet.displayState === "celebrate", "a finished turn celebrates")
        check(icons(pet) === "care-floor/star.webp" && iconsLoad(pet), "with a star bubble")
        check(pet.seen.finished_seq === 5 && pet.face.state === "idle" && pet.face.bar === "star",
              "it is recorded once shown; the face moves on (treats wait: the bar shows a star)")
        pet.clearTransients()
        pet.barState = doc({agent: fin, treats: {jar: 1, full: false}, state_version: 11})
        check(pet.transientState === "", "the same finish never celebrates twice")
        var hello = agent({hello_seq: 2, hello_at_ms: now - 5000, hello_app: "claude-code"})
        pet.barState = doc({agent: hello, state_version: 12})
        check(pet.transientState === "celebrate" && pet.seen.hello_seq === 2, "the first event from an app celebrates once")
        pet.clearTransients()
        pet.barState = doc({unseen_events: [{seq: 9, stage: 2}], life: lifeWith({stage: 2}), state_version: 13})
        check(pet.transientState === "celebrate" && icons(pet) === "care-floor/star.webp,status/confetti.webp",
              "growing up celebrates with star and confetti")
        check(pet.grewSeq === 9 && pet.face.state !== "celebrate", "growth plays once here (Home acknowledges it)")
        pet.clearTransients()
        pet.barState = doc({unseen_events: [{seq: 9, stage: 2}], life: lifeWith({stage: 2}), state_version: 14})
        check(pet.transientState === "", "the same growth never replays")

        // ---- a click always reacts (K5) ----
        show({})
        var before = homes
        check(pet.petClicked(), "a click on the pet is handled")
        check(homes === before + 1, "a click asks the host for Home once")
        check(icons(pet) === "care-floor/heart.webp" && iconsLoad(pet), "with a heart bubble")
        check(find(pet, "desktopPet.cosmeticTimer").running, "the heart bubble ends on its own")
        pet.reducedMotion = true
        pet.heartMsLeft = 0
        pet.petClicked()
        check(homes === before + 2 && icons(pet) === "care-floor/heart.webp" && pet.hopOffset === 0,
              "Calm mode: the bubble only, no hop")
        for (i = 0; i < 4; i++) pet.cosmeticTick()
        check(pet.heartMsLeft === 0 && icons(pet) === "", "the heart goes away")
        pet.reducedMotion = false
        show({care: {sleeping: true, sleep_remaining_ms: 0, sleep_started_at_ms: null, sleep_due: false, bathroom_due: false}})
        check(pet.petClicked() && homes === before + 3, "a sleeping pet still reacts and opens Home")
        show({enabled: false})
        check(!pet.petClicked() && homes === before + 3 && !pet.shown, "Off: no pet to click")
        show({})

        // ---- the single-shot wake timer (real clock) ----
        var t0 = Date.now()
        live.barState = doc({agent: agent({finished_seq: 1, finished_at_ms: t0 - 1500, finished_app: "codex"})})
        var wake = find(live, "desktopPet.wakeTimer")
        check(live.face.state === "idle" && wake.running && wake.interval <= 600, "inside the 2 s grace: one wake armed at its end")
        test.phase = 1
        later.start()
    }
    Timer {
        id: later
        interval: 900
        onTriggered: {
            var wake = test.find(live, "desktopPet.wakeTimer")
            test.check(live.seen.finished_seq === 1, "the wake timer re-read the clock and the pet celebrated")
            test.check(live.transientState === "celebrate" && !wake.running,
                       "once celebrated nothing is left to wait for: no wake armed")
            live.barState = test.doc({enabled: false})
            test.check(!wake.running && !test.find(live, "desktopPet.cosmeticTimer").running, "Off stops both timers (K9)")
            console.log("Overlay agent smoke: " + test.checks + " checks, " + test.failures + " failures")
            Qt.exit(test.failures > 0 ? 1 : 0)
        }
    }
}
