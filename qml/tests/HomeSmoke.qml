// Home and the Off look of the bar plugin popup (SPEC-v0.3 §3.2, §3.7, §9.3 HomeSmoke):
// every bar-v2 fixture that routes to Home, at font base sizes 12, 16 and 20, against K1-K6,
// K9 and K13; then taps (K5), the per-habitat copy (the owner's goldfish Ginger, the robot),
// Treats, Wake up, Turn on, the power switch, rename and Calm mode.
import QtQuick
import QtQuick.Window
import qs.Commons
import "../../omarchy-plugin"
import "../../omarchy-plugin/CodePetBar.js" as Model

Window {
    id: win
    width: 720
    height: 1200
    visible: true

    SmokeKit { id: kit }
    FakeCtl {
        id: ctl
        registry: kit.registry
        runtimeBase: kit.repo + "/"
    }
    PopupApp {
        id: app
        ctl: ctl
        width: implicitWidth
    }

    readonly property var vectors: kit.json(kit.contracts + "agent_vectors.json").priority.vectors
    readonly property var homeFixtures: ["asleep", "fainted", "finished-treat", "finished-while-working", "goldfish-ginger-owner",
        "grew-up", "hello", "helpers-busy", "idle-happy", "idle-treats", "jar-full", "need-food-critical", "need-food-soon",
        "need-water-now", "off", "off-treats", "oops", "paused", "ready-check-done", "robot-bolt", "several-needs",
        "stale-working", "test-failed", "test-running", "waiting", "working-many", "working-one", "working-two"]

    function load(name) {
        var v = null
        for (var i = 0; i < win.vectors.length; i++) if (win.vectors[i].fixture === name) { v = win.vectors[i]; break }
        ctl.now = v ? v.now : 1792123900000
        ctl.seen = v ? v.seen : { finished_seq: 0, hello_seq: 0, test_at_ms: 0 }
        ctl.barState = kit.fixture(name)
        app.home()
    }
    function count(kind) {
        var n = 0
        var list = kit.controls(app)
        for (var i = 0; i < list.length; i++) if (list[i].item.cpKind === kind) n++
        return n
    }
    function tile(need) { return kit.find(app, "cp.tile." + need) }
    function text(name) { var o = kit.find(app, name); return o ? String(o.text) : "" }
    function bubbleClearsHead(label) {
        var pet = kit.find(app, "cp.pet")
        var speech = kit.find(app, "cp.speech")
        kit.check(speech && speech.visible, label + ": bubble visible")
        var top = speech.mapToItem(pet, 0, 0).y
        kit.check(top >= pet.height * 0.4, label + ": bubble stays below the head (" + Math.round(top) + "/" + Math.round(pet.height) + ")")
        kit.check(kit.px16(speech.height) < 72 && kit.px16(speech.width) <= 160, label + ": bubble is small (" + Math.round(kit.px16(speech.width)) + "x" + Math.round(kit.px16(speech.height)) + ")")
    }

    Component.onCompleted: kit.run(kit.each([12, 16, 20], win.homeFixtures, function (size, name) {
        win.load(name)
    }, function (size, name) {
        var label = name + " @" + size
        kit.check(app.screen === (name.indexOf("off") === 0 ? "off_look" : "home"), label + ": routes to " + app.screen)
        kit.all(app, label)
        kit.oneLine(app, label)
        var tiles = win.count("tile"), rounds = win.count("round"), treats = win.count("treats")
        kit.check(tiles <= 4 && rounds <= 3 && treats <= 1, label + ": K4 at most 4 tiles, 3 round, 1 Treats")
        var face = ctl.face
        kit.check(treats === (["off", "asleep", "paused", "fainted"].indexOf(face.state) < 0 ? 1 : 0), label + ": finite jar remains visible including zero")
        var pet = kit.find(app, "cp.pet")
        kit.check(pet && kit.px16(pet.height) >= 159.5, label + ": the pet is 160 px")
        if (face.state === "off") kit.check(tiles === 0 && rounds === 0 && kit.find(app, "cp.turnOn").visible, label + ": Off shows Turn on only")
        if (face.state === "asleep" || face.state === "paused") kit.check(tiles === 0 && kit.find(app, "cp.wake").visible, label + ": asleep shows Wake up only")
        if (face.state === "fainted") kit.check(tiles === 2 && rounds === 0 && tile("food") && tile("water"), label + ": fainted shows the two tiles that revive")
        if (["off", "asleep", "paused", "fainted"].indexOf(face.state) < 0) kit.check(tiles === 4 && rounds === 3, label + ": 4 tiles and 3 round buttons")
        var progress = kit.find(app, "cp.progress")
        kit.check(progress && progress.visible, label + ": level and XP are visible on Home")
        kit.check(text("cp.progress.level") === "Level " + ctl.barState.life.level, label + ": exact level")
        kit.check(text("cp.progress.xp") === ctl.barState.life.xp + " XP", label + ": exact total XP")
        kit.check(text("cp.progress.age") === "Age unavailable", label + ": old export never invents a birth date")
        var careNeeds = ["food", "water", "play", "wash"]
        for (var n = 0; n < careNeeds.length; n++) {
            var careTile = tile(careNeeds[n])
            if (!careTile || !careTile.visible) continue
            var meter = kit.find(careTile, "cp.meter.value")
            var raw = ctl.barState.meters[Model.METER_OF[careNeeds[n]]]
            kit.check(meter && meter.text === Math.floor(raw / 100) + "%", label + ": " + careNeeds[n] + " percentage")
            var track = kit.find(careTile, "cp.meter.track")
            var fill = kit.find(careTile, "cp.meter.fill")
            kit.check(track && fill && Math.abs(fill.width - track.width * raw / 10000) < 0.5, label + ": " + careNeeds[n] + " bar fill")
        }
    }).concat([
        function () {
            win.load("idle-treats")
            var doc = ctl.barState
            doc.life.adopted_at_ms = ctl.now - 43 * 3600000
            ctl.barState = JSON.parse(JSON.stringify(doc))
        },
        function () {
            kit.check(text("cp.progress.age") === "Living for 1 day", "durable birth time shows wall age beside XP")
            kit.check(text("cp.progress.birth").indexOf("Born ") === 0, "exact birth date remains available on hover")
            kit.all(app, "age beside XP with Treats")
        },
        function () {
            win.load("idle-happy")
            ctl.workState = { readiness: { ready: true, energy: 67 }, task: null }
        },
        function () {
            kit.check(text("cp.energyLine") === "Energy 67% · Ready to work", "Home shows numeric energy and readiness as secondary text")
            kit.all(app, "Home with work readiness")
            ctl.workState = { readiness: { ready: false, energy: 67 }, task: { status: "review" } }
        },
        function () {
            kit.check(text("cp.statusLine") === "Your job is ready to review", "owned review is distinct from observed client activity")
            kit.find(app, "cp.round.work").tapped()
            kit.check(ctl.workLaunches[ctl.workLaunches.length - 1] === "compact", "existing job reachable when not ready for another")
            ctl.workState = null
            win.load("fainted")
        },
        function () {
            tile("food").tapped()
            kit.check(ctl.last("pet.care").params.action === "food", "fainted pet has free food recovery without work")
        },
        // Numeric stats remain visible after final evolution; missing data is not zero.
        function () {
            win.load("idle-happy")
            var doc = ctl.barState
            doc.life.stage = 3; doc.life.level = 5; doc.life.xp = 4294967295; doc.life.next_stage_xp = null
            ctl.barState = JSON.parse(JSON.stringify(doc))
        },
        function () {
            kit.check(text("cp.progress.xp") === "4294967295 XP", "final evolution keeps exact total XP")
            kit.check(text("cp.progress.next") === "Fully evolved", "final evolution has no invented next target")
            kit.all(app, "maximum XP @20")
            var doc = ctl.barState
            doc.meters = {}
            ctl.barState = JSON.parse(JSON.stringify(doc))
        },
        function () {
            var meter = kit.find(tile("food"), "cp.meter.value")
            kit.check(meter && meter.text === "—", "unknown care displays a dash, not zero")
        },
        // ---- K9: Off has zero running timers in the popup; On animates while open.
        function () { win.load("off"); return 120 },
        function () {
            kit.check(kit.runningTimers(app).length === 0, "Off: zero running popup timers " + JSON.stringify(kit.runningTimers(app)))
            var on = kit.find(app, "cp.turnOn")
            kit.check(on && on.visible, "Off shows Turn on")
            on.tapped()
        },
        function () {
            kit.check(ctl.last("app.set_enabled") && ctl.last("app.set_enabled").params.enabled === true,
                      "Turn on: " + JSON.stringify(ctl.methods()))
            win.load("idle-happy"); return 120
        },
        function () {
            kit.check(kit.runningTimers(app).indexOf("codepetPopup.frames") >= 0, "On and open: the frame clock runs")
            ctl.popupVisible = false
        },
        function () {
            kit.check(kit.runningTimers(app).length === 0, "closed popup: no running timers " + JSON.stringify(kit.runningTimers(app)))
            ctl.popupVisible = true
            ctl.cfg = { label: "name", calm: true, overlayOnLoad: false }
        },
        function () {
            kit.check(kit.runningTimers(app).filter(function (name) { return name !== "codepetPopup.age" }).length === 0, "Calm mode: only the minute age clock " + JSON.stringify(kit.runningTimers(app)))
            ctl.cfg = { label: "name", calm: false, overlayOnLoad: false }
            Style.fontBaseSize = 16
            var doc = kit.fixture("idle-happy")
            doc.meters = { hunger: 6000, thirst: 6000, mental_health: 6000, hygiene: 6000 }
            ctl.barState = doc
        },
        // Useful work opens its real task window; care rewards wait for acceptance.
        function () {
            var before = ctl.calls.length
            tile("food").tapped()
            kit.check(ctl.last("pet.care").params.action === "food", "Snack sends free care")
            kit.check(ctl.calls.length === before + 1, "Snack sends exactly one care request")
        },
        function () {
            var doc = kit.fixture("idle-happy")
            doc.meters = { hunger: 10000, thirst: 6000, mental_health: 6000, hygiene: 6000 }
            ctl.barState = doc
        },
        function () {
            ctl.responses = { "pet.care": { changed: false } }
            tile("play").tapped()
            kit.check(app.bubbleText === "One moment…", "Play waits for confirmation")
        },
        function () {
            kit.check(app.bubbleText === "Ginger played already!", "changed:false swaps to the full line: " + app.bubbleText)
            win.bubbleClearsHead("full play")
            ctl.responses = { "pet.care": { ok: false, error: { code: "INVALID_STATE" } } }
            tile("wash").tapped()
        },
        function () {
            kit.check(app.bubbleText === "Oops! Please try again.", "a failed tap says so: " + app.bubbleText)
            ctl.responses = {}
            ctl.hold = true
            tile("play").tapped()
            kit.check(app.bubbleText === "One moment…", "Play does not claim success while pending")
            ctl.hold = false
            ctl.release()
        },
        // ---- per-habitat copy: the owner's goldfish Ginger and the robot.
        function () {
            win.load("goldfish-ginger-owner")
            var doc = ctl.barState
            doc.meters.thirst = 5000
            ctl.barState = JSON.parse(JSON.stringify(doc))
        },
        function () {
            var t = tile("water"), w = tile("wash")
            kit.check(t.text === "Fresh water" && t.detail === "" && w.text === "Clean tank", "goldfish work and care labels")
            kit.check(tile("food").icon === "care-aquatic/food", "goldfish uses the aquatic icons")
            kit.check(text("cp.statusLine") === "Ginger's tank is dirty!", "goldfish line: " + text("cp.statusLine"))
            tile("water").tapped()
            kit.check(ctl.last("pet.care").params.action === "water", "water is free recovery care")
        },
        function () { win.load("robot-bolt") },
        function () {
            kit.check(tile("food").text === "Charge" && tile("food").detail === "" && tile("water").text === "Cool down" && tile("wash").text === "Polish",
                      "robot tiles: " + tile("food").text + " / " + tile("water").text + " / " + tile("wash").text)
            kit.check(tile("food").icon === "care-robot/food", "robot uses the robot icons")
        },
        // ---- Treats.
        function () { win.load("idle-treats") },
        function () {
            var jar = kit.find(app, "cp.treats")
            kit.check(jar && jar.visible, "Treats shows with treats in the jar")
            jar.Accessible.pressAction()
        },
        function () {
            var c = ctl.last("treats.give")
            kit.check(c && c.params.life_id === ctl.barState.life.life_id, "Treats sends treats.give for the life")
            kit.check(app.bubbleText === "Yum! +5 XP", "Treats bubble: " + app.bubbleText)
            win.bubbleClearsHead("treat")
        },
        function () { win.load("jar-full") },
        function () { kit.check(kit.find(app, "cp.treats").visible, "a full jar still offers Treats") },
        // ---- the full jar wiggles and says so (K5): Home opening on the unseen finished turn,
        // a turn finishing while Home is open, and the idle status line.
        function () { win.load("idle-happy"); app.go("more", {}) },
        function () {
            ctl.seen = { finished_seq: 0, hello_seq: 0, test_at_ms: 0 }
            ctl.barState = kit.fixture("jar-full")
            kit.check(ctl.face.line === "act.done_full", "jar-full: the finished turn is unseen (" + ctl.face.line + ")")
            app.home()
        },
        function () {
            kit.check(kit.find(app, "cp.body").item.jarWiggling === true, "Home opening on a finished turn with a full jar wiggles the jar")
            kit.check(text("cp.statusLine") === "All done! The jar is full!", "and says so: " + text("cp.statusLine"))
            return 700
        },
        function () {
            ctl.seen = { finished_seq: 2, hello_seq: 0, test_at_ms: 0 }
            kit.check(ctl.face.state === "idle", "seen: idle (" + ctl.face.state + ")")
            kit.check(text("cp.statusLine") === "The treat jar is full!", "idle with a full jar: the treats.full line (" + text("cp.statusLine") + ")")
            kit.check(kit.find(app, "cp.body").item.jarWiggling === false, "the wiggle is over")
            var doc = kit.fixture("jar-full")
            doc.agent.finished_seq = 3
            doc.agent.finished_at_ms = ctl.now - 10000
            ctl.barState = doc
        },
        function () {
            kit.check(kit.find(app, "cp.body").item.jarWiggling === true, "a turn finishing while Home is open wiggles the full jar")
            return 700
        },
        function () {
            var doc = kit.fixture("idle-treats")
            ctl.seen = { finished_seq: 99, hello_seq: 0, test_at_ms: 0 }
            ctl.barState = doc
        },
        function () {
            var doc = kit.fixture("idle-treats")
            doc.agent.finished_seq = 100
            ctl.barState = doc
        },
        function () {
            kit.check(kit.find(app, "cp.body").item.jarWiggling === false, "a jar that is not full does not wiggle")
            kit.check(text("cp.statusLine") !== "The treat jar is full!", "nor says it is full")
        },
        // ---- Wake up (asleep -> pet.wake, paused -> pet.pause false).
        function () { win.load("asleep") },
        function () { kit.find(app, "cp.wake").tapped() },
        function () { kit.check(ctl.count("pet.wake") === 1, "Wake up sends pet.wake") ; win.load("paused") },
        function () { kit.find(app, "cp.wake").tapped() },
        function () {
            var c = ctl.last("pet.pause")
            kit.check(c && c.params.paused === false, "Wake up on a paused pet sends pet.pause false")
        },
        // ---- the power switch on Home turns CodePet off; round buttons open pages.
        function () { win.load("idle-happy") },
        function () {
            var power = kit.find(app, "cp.power")
            kit.check(power && power.visible && power.checked, "the power switch shows On")
            power.toggled(false)
            kit.check(ctl.last("app.set_enabled").params.enabled === false, "the power switch sends app.set_enabled false")
        },
        function () { kit.find(app, "cp.round.work").tapped() },
        function () {
            kit.check(ctl.workLaunches[ctl.workLaunches.length - 1] === "compact", "Work opens compact bug card")
            app.go("helpers", {})
        },
        function () {
            kit.check(kit.find(app, "cp.back").visible, "‹ Back is shown on Helpers")
            kit.find(app, "cp.header").back()
        },
        function () {
            kit.check(app.screen === "home" && !kit.find(app, "cp.back").visible, "‹ Back returns Home, which has no Back")
            kit.find(app, "cp.round.change").tapped()
        },
        function () { kit.check(app.screen === "change", "Change opens"); app.home(); kit.find(app, "cp.round.more").tapped() },
        function () { kit.check(app.screen === "more", "More opens"); app.home() },
        // ---- a failure away from Home is shown where the kid is (the notice over the screen).
        function () { ctl.responses = { "pet.sleep": { ok: false, error: { code: "INVALID_STATE" } } }; app.go("more", {}) },
        function () { kit.find(app, "cp.more.bedtime").tapped() },
        function () {
            var n = kit.find(app, "cp.notice")
            kit.check(app.screen === "more" && n && kit.shown(n, app) && n.text === "Oops! Please try again.",
                      "a failed Bedtime says so on More: " + (n ? n.visible + " " + n.text : "none"))
            kit.kidCopy(app, "notice on More")
            kit.fits(app, "notice on More")
            ctl.responses = {}
            app.home()
        },
        function () {
            var n = kit.find(app, "cp.notice")
            kit.check(!n.visible && app.bubbleText === "Oops! Please try again.", "on Home the word bubble says it instead")
        },
        // ---- rename through the name button.
        function () { kit.find(app, "cp.nameButton").Accessible.pressAction() },
        function () {
            kit.check(app.editing && kit.find(app, "cp.nameInput"), "the name button opens the name field")
            kit.all(app, "rename field @16")
            app.finishEdit("x".repeat(30))
        },
        function () {
            kit.check(app.editing && text("cp.nameError") === "That name is too long.", "a long name is refused: " + text("cp.nameError"))
            app.finishEdit("   ")
        },
        function () {
            kit.check(app.editing && text("cp.nameError") === "Type a name", "an empty name asks for one: " + text("cp.nameError"))
            app.finishEdit("  Pip ")
        },
        function () {
            var c = ctl.last("pet.rename")
            kit.check(!app.editing && c && c.params.name === "Pip" && c.params.life_id === ctl.barState.life.life_id, "Done sends pet.rename Pip")
        },
        // ---- a request the bar never sends is refused before anything runs.
        function () {
            var got = null
            ctl.request("task.start", {}, function (r) { got = r })
            kit.check(got && got.error.code === "CLIENT_REFUSED", "task.* is refused")
        }
    ]))
}
