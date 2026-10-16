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
        kit.check(treats === (face.treats_visible ? 1 : 0), label + ": Treats only when the jar can be given")
        var pet = kit.find(app, "cp.pet")
        kit.check(pet && kit.px16(pet.height) >= 159.5, label + ": the pet is 160 px")
        if (face.state === "off") kit.check(tiles === 0 && rounds === 0 && kit.find(app, "cp.turnOn").visible, label + ": Off shows Turn on only")
        if (face.state === "asleep" || face.state === "paused") kit.check(tiles === 0 && kit.find(app, "cp.wake").visible, label + ": asleep shows Wake up only")
        if (face.state === "fainted") kit.check(tiles === 2 && rounds === 0 && tile("food") && tile("water"), label + ": fainted shows the two tiles that revive")
        if (["off", "asleep", "paused", "fainted"].indexOf(face.state) < 0) kit.check(tiles === 4 && rounds === 3, label + ": 4 tiles and 3 round buttons")
        var bubble = kit.find(app, "cp.thought")
        kit.check(!bubble.visible || kit.px16(bubble.height) <= 72.5, label + ": thought bubble <= 72 px")
    }).concat([
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
            kit.check(kit.runningTimers(app).length === 0, "Calm mode: no animation clock " + JSON.stringify(kit.runningTimers(app)))
            ctl.cfg = { label: "name", calm: false, overlayOnLoad: false }
            Style.fontBaseSize = 16
            var doc = kit.fixture("idle-happy")
            doc.meters = { hunger: 6000, thirst: 6000, mental_health: 6000, hygiene: 6000 }
            ctl.barState = doc
        },
        // ---- K5: a care tap reacts at once, even when nothing changes.
        function () {
            var before = ctl.calls.length
            var t0 = Date.now()
            tile("food").tapped()
            var dt = Date.now() - t0
            var word = Model.careWord(kit.registry, "cat", "food")
            kit.check(app.bubbleText === "Yum! " + word + "!" && dt < 150, "food tap: bubble '" + app.bubbleText + "' in " + dt + " ms")
            kit.check(app.actionState === "food", "food tap starts the food animation")
            kit.check(ctl.calls.length === before + 1 && ctl.last("pet.care").params.action === "food", "food tap sends pet.care food")
        },
        function () {
            var doc = kit.fixture("idle-happy")
            doc.meters = { hunger: 10000, thirst: 6000, mental_health: 6000, hygiene: 6000 }
            ctl.barState = doc
        },
        function () {
            tile("food").tapped()
            kit.check(app.bubbleText === "Ginger is full!", "a full pet says so at once: " + app.bubbleText)
            ctl.responses = { "pet.care": { changed: false } }
            tile("play").tapped()
            kit.check(app.bubbleText.indexOf("Wheee!") === 0, "play reacts: " + app.bubbleText)
        },
        function () {
            kit.check(app.bubbleText === "Ginger played already!", "changed:false swaps to the full line: " + app.bubbleText)
            ctl.responses = { "pet.care": { ok: false, error: { code: "INVALID_STATE" } } }
            tile("wash").tapped()
        },
        function () {
            kit.check(app.bubbleText === "Oops! Please try again.", "a failed tap says so: " + app.bubbleText)
            ctl.responses = {}
            ctl.hold = true
            tile("water").tapped()
            kit.check(app.bubbleText.indexOf("Ahh!") === 0, "a tap reacts while the request is still running")
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
            kit.check(t.text === "Fresh water" && w.text === "Clean tank", "goldfish tiles: " + t.text + " / " + w.text)
            kit.check(tile("food").icon === "care-aquatic/food", "goldfish uses the aquatic icons")
            kit.check(text("cp.statusLine") === "Ginger's tank is a bit dirty.", "goldfish line: " + text("cp.statusLine"))
            tile("water").tapped()
            kit.check(app.bubbleText === "Ahh! Fresh water!", "goldfish water bubble: " + app.bubbleText)
        },
        function () { win.load("robot-bolt") },
        function () {
            kit.check(tile("food").text === "Charge" && tile("water").text === "Cool down" && tile("wash").text === "Polish",
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
            kit.check(app.bubbleText === "Yum! Ginger is growing!", "Treats bubble: " + app.bubbleText)
        },
        function () { win.load("jar-full") },
        function () { kit.check(kit.find(app, "cp.treats").visible, "a full jar still offers Treats") },
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
        function () { kit.find(app, "cp.round.helpers").tapped() },
        function () {
            kit.check(app.screen === "helpers", "Helpers opens: " + app.screen)
            kit.check(kit.find(app, "cp.back").visible, "‹ Back is shown on Helpers")
            kit.find(app, "cp.header").back()
        },
        function () {
            kit.check(app.screen === "home" && !kit.find(app, "cp.back").visible, "‹ Back returns Home, which has no Back")
            kit.find(app, "cp.round.change").tapped()
        },
        function () { kit.check(app.screen === "change", "Change opens"); app.home(); kit.find(app, "cp.round.more").tapped() },
        function () { kit.check(app.screen === "more", "More opens"); app.home() },
        // ---- rename through the name button.
        function () { kit.find(app, "cp.nameButton").Accessible.pressAction() },
        function () {
            kit.check(app.editing && kit.find(app, "cp.nameInput"), "the name button opens the name field")
            kit.all(app, "rename field @16")
            app.finishEdit("x".repeat(30))
        },
        function () {
            kit.check(app.editing && text("cp.nameError") === "That name is too long.", "a long name is refused: " + text("cp.nameError"))
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
