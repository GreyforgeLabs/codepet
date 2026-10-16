// How it works (SPEC-v0.3 §3.6, §9.3 HelpSmoke): an adult page that may scroll. Hardcore is
// turned on only behind the pet's typed name and a happy pet, and off with one tap; Count
// background helpers; Forget coding history asks first.
import QtQuick
import QtQuick.Window
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
        snapshot: ({ count_helpers: false })
    }
    PopupApp {
        id: app
        ctl: ctl
        width: implicitWidth
    }

    function load(name, edit) {
        ctl.now = 1792123900000
        var doc = kit.fixture(name)
        if (edit) edit(doc)
        ctl.barState = doc
        app.home()
        app.go("help", {})
    }
    function find(name) { return kit.find(app, name) }
    function shown(name) { var o = kit.find(app, name); return !!(o && o.visible) }
    // Help scrolls: a line below the fold still counts.
    function hasText(s) { return win.allText().indexOf(s) >= 0 }
    function allText() { var out = []; kit.walk(app, function (o) { if (kit.isText(o) && o.visible) out.push(String(o.text)); return true }); return out }

    Component.onCompleted: kit.run(kit.each([12, 16, 20], ["idle-happy"], function (size, name) {
        win.load(name)
    }, function (size, name) {
        var label = "help @" + size
        kit.check(app.screen === "help" && win.find("cp.helpFlick"), label + ": Help is a scrolling page")
        kit.sizes(app, label)
        kit.fits(app, label)
        var t = win.allText()
        for (var k of ["help.rules", "help.grownup", "help.apps", "help.privacy", "help.rules.body", "help.apps.body", "help.privacy.body",
                       "help.forget.body", "help.helpers.body"])
            kit.check(t.indexOf(app.s(k)) >= 0, label + ": shows " + k)
    }).concat([
        // ---- Hardcore on: warning, typed name, then pet.rules permadeath
        function () { win.load("idle-happy") },
        function () {
            var hc = win.find("cp.help.hardcore")
            kit.check(hc && !hc.checked, "Hardcore starts off for a gentle pet")
            hc.toggled(true)
            kit.check(ctl.count("pet.rules") === 0, "turning Hardcore on asks first")
        },
        function () {
            kit.check(win.shown("cp.help.hardcoreAsk") && win.hasText("Hardcore: Ginger can leave forever."), "the warning shows")
            kit.check(!win.shown("cp.help.hardcoreOn"), "no Turn on Hardcore before the name")
            win.find("cp.help.hardcoreName").text = "Ginger"
        },
        function () {
            kit.check(win.shown("cp.help.hardcoreOn"), "Turn on Hardcore after the typed name")
            ctl.responses = { "pet.rules": { ok: false, error: { code: "INVALID_STATE" } } }
            win.find("cp.help.hardcoreOn").tapped()
            var c = ctl.last("pet.rules")
            kit.check(c && c.params.mode === "permadeath" && c.params.confirm_name === "Ginger" && c.params.acknowledge_rules === true,
                      "pet.rules permadeath with the name " + JSON.stringify(c ? c.params : null))
        },
        function () {
            kit.check(win.hasText("Make Ginger happy first."), "an unhappy pet: Make Ginger happy first.")
            ctl.responses = {}
        },
        // ---- Hardcore off: one tap
        function () { win.load("idle-happy", function (d) { d.life.mode = "permadeath" }) },
        function () {
            var hc = win.find("cp.help.hardcore")
            kit.check(hc.checked, "Hardcore shows On for a permadeath pet")
            hc.toggled(false)
            var c = ctl.last("pet.rules")
            kit.check(c && c.params.mode === "gentle", "one tap turns Hardcore off")
        },
        // ---- helpers and Forget
        function () {
            win.find("cp.help.helpers").toggled(true)
            kit.check(ctl.last("app.set_helpers").params.enabled === true, "Count background helpers sends app.set_helpers")
            win.find("cp.help.forget").tapped()
        },
        function () {
            kit.check(win.hasText("Forget coding history?") && ctl.count("agent.forget_history") === 0, "Forget asks first")
            win.find("cp.help.forgetYes").tapped()
            kit.check(ctl.count("agent.forget_history") === 1, "Forget sends agent.forget_history")
        },
        // ---- Help from the first-run picker (no pet yet): no Hardcore row
        function () { ctl.barState = kit.fixture("fresh-no-pet"); app.home() },
        function () { kit.find(app, "cp.rulesHelp").tapped() },
        function () {
            kit.check(app.screen === "help" && !win.shown("cp.help.hardcore"), "Help from first run, without Hardcore")
            kit.find(app, "cp.header").back()
        },
        function () { kit.check(app.screen === "first_run", "‹ Back returns to the picker") }
    ]))
}
