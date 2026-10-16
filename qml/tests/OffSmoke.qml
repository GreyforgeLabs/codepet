// The Off look (SPEC-v0.3 §3.7, §7.3, K9, §9.3 OffSmoke): Off from any page shows the
// dimmed sleeping pet with one Turn on and zero running popup timers; On routes afresh to
// Home; Off before any pet keeps the picker with Turn on; the face asks for no wake-up.
import QtQuick
import QtQuick.Window
import qs.Commons
import "../../omarchy-plugin"

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

    function shown(name) { var o = kit.find(app, name); return !!(o && kit.shown(o, app)) }

    Component.onCompleted: kit.run([
        function () { Style.fontBaseSize = 16; ctl.barState = kit.fixture("working-one"); app.go("helpers", {}) },
        function () {
            kit.check(app.screen === "helpers" && kit.runningTimers(app).indexOf("codepetPopup.frames") >= 0, "On: Helpers open, the clock runs")
            ctl.barState = kit.fixture("off")
        },
        function () {
            kit.check(app.screen === "off_look" && app.page === "", "Off from Helpers shows the Off look at once")
            kit.check(kit.runningTimers(app).length === 0, "Off: zero running timers " + JSON.stringify(kit.runningTimers(app)))
            kit.check(ctl.face.wake_at_ms === null && ctl.face.state === "off", "Off: the face asks for no wake-up")
            var pet = kit.find(app, "cp.pet")
            kit.check(pet.dim && pet.petState === "sleep", "the pet is dimmed and asleep")
            kit.check(kit.texts(app).some(function (t) { return t.text === "Ginger is resting." }), "Ginger is resting.")
            kit.check(win.shown("cp.turnOn") && !win.shown("cp.tiles") && !win.shown("cp.round.more") && !win.shown("cp.treats"), "only Turn on")
            var power = kit.find(app, "cp.power")
            kit.check(power.visible && !power.checked && power.textOff === "Off", "the power switch shows Off")
            kit.all(app, "off look")
            app.say("x", "food")
        },
        function () {
            kit.check(!kit.find(app, "cp.tiles").visible, "no care while Off")
            ctl.barState = kit.fixture("working-one")
        },
        function () { kit.check(app.screen === "home", "On again routes Home, not back to Helpers (" + app.screen + ")") },
        function () { ctl.barState = kit.fixture("off-treats") },
        function () {
            kit.check(!win.shown("cp.treats"), "no Treats while Off")
            kit.check(kit.runningTimers(app).filter(function (n) { return n !== "codepetPopup.bubble" }).length === 0, "Off with treats: no clock")
            ctl.barState = kit.fixture("off-no-pet")
        },
        function () {
            kit.check(app.screen === "first_run_off" && kit.find(app, "cp.go").text === "Turn on", "Off before any pet: the picker with Turn on")
            kit.check(kit.runningTimers(app).filter(function (n) { return n !== "codepetPopup.bubble" }).length === 0, "Off picker: no clock")
            kit.all(app, "first run off")
        }
    ])
}
