// First run, welcome back, Change, the memorial card and the Connect card of the bar plugin
// popup (SPEC-v0.3 §2.1, §3.3, §3.4, §9.3 FirstRunSmoke): the cat is preselected, Let's go!
// alone sends pet.start, the name follows the species until typed, no Back / stage row /
// stars on first run, the sticky footer is always inside the card, the Connect card never
// offers Codex and is skipped when nothing is found, and closing it counts as Not now.
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
    }
    PopupApp {
        id: app
        ctl: ctl
        width: implicitWidth
    }

    function load(name) {
        ctl.now = 1792123900000
        ctl.barState = kit.fixture(name)
        app.meetNew = false
        app.home()
    }
    function find(name) { return kit.find(app, name) }
    function shown(name) { var o = kit.find(app, name); return !!(o && kit.shown(o, app)) }
    function picker() { return kit.find(app, "cp.body").item }
    function freshCall(method) { return ctl.last(method) }

    readonly property var screens: [
        { fixture: "fresh-no-pet", screen: "first_run" },
        { fixture: "off-no-pet", screen: "first_run_off" },
        { fixture: "owner-unarmed-welcome", screen: "welcome_back" },
        { fixture: "memorial", screen: "memorial" },
        { fixture: "idle-happy", screen: "change", page: "change" },
        { fixture: "idle-happy", screen: "connect", page: "connect", offered: ["claude-code", "opencode", "hermes", "grok-cli"] },
        { fixture: "idle-happy", screen: "connect", page: "connect", offered: ["grok-cli"] },
        { fixture: "test-running", screen: "connect", page: "connect", offered: ["claude-code"], sent: true },
        { fixture: "ready-check-done", screen: "connect", page: "connect", offered: ["claude-code"], sent: true },
        { fixture: "test-failed", screen: "connect", page: "connect", offered: ["claude-code"], sent: true }
    ]

    Component.onCompleted: kit.run(kit.each([12, 16, 20], win.screens, function (size, c) {
        win.load(c.fixture)
        if (c.page) app.go(c.page, { offered: c.offered || [] })
        if (c.sent) {
            var card = win.picker()
            card.sentIds = c.offered
            card.sent = true
        }
    }, function (size, c) {
        var label = c.screen + " (" + c.fixture + (c.offered ? " " + c.offered.length + " apps" : "") + (c.sent ? " sent" : "") + ") @" + size
        kit.check(app.screen === c.screen, label + ": screen " + app.screen)
        kit.all(app, label)
        if (c.screen === "first_run" || c.screen === "welcome_back" || c.screen === "first_run_off") {
            kit.check(!win.shown("cp.back") && !win.shown("cp.stages") && !win.shown("cp.stars"), label + ": no Back, stage row or stars")
            kit.check(win.shown("cp.go") && win.shown("cp.power"), label + ": footer and power switch")
        }
    }).concat([
        // ---- the connect card after Connect: rows, the ready line, Try again
        function () { win.load("ready-check-done"); app.go("connect", { offered: ["claude-code"] }) },
        function () { var card = win.picker(); card.sentIds = ["claude-code"]; card.sent = true },
        function () {
            var line = win.find("cp.connectLine")
            kit.check(line && line.text === "Ginger is ready!", "ready check done: " + (line ? line.text : "none"))
            kit.check(kit.texts(app).some(function (t) { return t.text === "Open a new Claude Code window." }), "ready: the open-a-window hint")
            kit.check(win.shown("cp.connectDone") && !win.shown("cp.connectRetry"), "ready: Done, no Try again")
            kit.all(app, "connect ready @16")
        },
        function () { win.load("test-failed"); app.go("connect", { offered: ["claude-code"] }) },
        function () { var card = win.picker(); card.sentIds = ["claude-code"]; card.sent = true },
        function () {
            kit.check(win.shown("cp.connectRetry"), "a failed ready check offers Try again")
            kit.check(kit.texts(app).some(function (t) { return t.text === "Something blocked it." }), "the reason line shows")
            kit.all(app, "connect failed @16")
            win.find("cp.connectRetry").tapped()
            var c = ctl.last("clients.connect")
            kit.check(c && JSON.stringify(c.params.clients) === '["claude-code"]', "Try again connects the failed app again")
        },
        // ---- first run: the cat is preselected and Let's go! alone starts it
        function () { win.load("fresh-no-pet"); ctl.calls = []; ctl.responses = { "clients.status": kit.status("owner-first-run") } },
        function () {
            var p = win.picker()
            kit.check(p.selected === "cat" && win.find("cp.portrait.cat").chosen, "the cat is preselected")
            kit.check(p.shownName === "Ginger", "the name is the cat's: " + p.shownName)
            win.picker().pick("dog")
        },
        function () {
            var p = win.picker()
            kit.check(p.selected === "dog" && p.shownName === "Biscuit", "the name follows the species: " + p.shownName)
            kit.check(kit.find(app, "cp.rules").visible && kit.texts(app).some(function (t) { return t.text === "Feed me when I'm hungry!" }), "rule line 1")
            win.picker().pick("robot")
        },
        function () {
            kit.check(kit.texts(app).some(function (t) { return t.text === "Charge me when I'm low!" }), "robot rule line")
            win.picker().pick("cat")
        },
        function () {
            var t0 = Date.now()
            win.find("cp.go").tapped()
            var c = ctl.last("pet.start")
            kit.check(c && JSON.stringify(c.params) === JSON.stringify({ species: "cat", mode: "gentle", acknowledge_rules: true }),
                      "Let's go! alone sends pet.start for the cat: " + JSON.stringify(c ? c.params : null))
            return 60
        },
        function () {
            kit.check(ctl.settings.some(function (s) { return s.key === "overlayOnLoad" && s.value === true }), "Let's go! turns the desktop pet on")
            kit.check(ctl.launches.indexOf("show") >= 0, "Let's go! shows the desktop pet")
            kit.check(ctl.count("clients.status") === 1, "then clients.status")
            ctl.barState = kit.fixture("idle-happy")
        },
        function () {
            kit.check(app.screen === "connect", "apps found: the Connect card follows (" + app.screen + " " + JSON.stringify(ctl.methods()) + ")")
            var card = win.picker()
            kit.check(JSON.stringify(card.offered) === JSON.stringify(["claude-code", "opencode", "hermes", "grok-cli"]), "offered " + JSON.stringify(card.offered))
            kit.check(!win.find("cp.connectRow.codex"), "Codex is never on the card")
            kit.check(kit.texts(app).some(function (t) { return t.text === "Your apps will remember Ginger." }), "privacy.3 with Claude Code ticked")
            win.find("cp.connectRow.claude-code").Accessible.pressAction()
            win.find("cp.connectRow.hermes").Accessible.pressAction()
        },
        function () {
            kit.check(!kit.texts(app).some(function (t) { return t.text === "Your apps will remember Ginger." }), "no privacy.3 without Claude Code and Hermes")
            win.find("cp.connect").tapped()
            var c = ctl.last("clients.connect")
            kit.check(c && JSON.stringify(c.params.clients) === JSON.stringify(["opencode", "grok-cli"]), "Connect sends the ticked apps " + JSON.stringify(c ? c.params : null))
        },
        function () {
            kit.check(win.shown("cp.connectDone"), "after Connect: Done")
            win.find("cp.connectDone").tapped()
        },
        function () { kit.check(app.screen === "home", "Done goes Home") },
        // ---- nothing found: the card is skipped; Not now; ALREADY_ARMED opens Home
        function () { win.load("fresh-no-pet"); ctl.responses = { "clients.status": kit.status("nothing-installed") } },
        function () { win.find("cp.go").tapped(); return 60 },
        function () { ctl.barState = kit.fixture("idle-happy") },
        function () { kit.check(app.screen === "home", "nothing installed: no Connect card (" + app.screen + ")") },
        function () { app.go("connect", { offered: ["grok-cli"] }) },
        function () { var before = ctl.calls.length; win.find("cp.notNow").tapped(); kit.check(ctl.calls.length === before && app.screen === "home", "Not now changes nothing and goes Home") },
        function () { win.load("fresh-no-pet"); ctl.responses = { "pet.start": { ok: false, error: { code: "ALREADY_ARMED" } }, "clients.status": kit.status("all-connected") } },
        function () { win.find("cp.go").tapped(); return 60 },
        function () { kit.check(app.screen === "first_run" && app.page === "", "ALREADY_ARMED: no error, routes by the export (" + app.screen + ")") ; ctl.responses = {} },
        // ---- a typed name sticks across species changes and goes into pet.start
        function () { win.load("fresh-no-pet") },
        function () { win.find("cp.pickerName").Accessible.pressAction() },
        function () {
            kit.check(app.editing, "the name button opens the name field")
            app.finishEdit("Mochi")
        },
        function () { win.picker().pick("owl") },
        function () {
            kit.check(win.picker().shownName === "Mochi", "a typed name is kept: " + win.picker().shownName)
            win.find("cp.go").tapped()
            var c = ctl.last("pet.start")
            kit.check(c && c.params.species === "owl" && c.params.name === "Mochi" && c.params.life_id === undefined, "pet.start owl Mochi")
        },
        // ---- welcome back: the goldfish is preselected, the name is kept, Let's go! arms her
        function () {
            var doc = kit.fixture("owner-unarmed-welcome")
            win.load("owner-unarmed-welcome")
        },
        function () {
            var p = win.picker()
            var life = ctl.barState.life
            kit.check(p.selected === life.appearance, "welcome back preselects " + life.appearance)
            kit.check(kit.find(app, "cp.header").title === "Welcome back, Ginger!", "welcome title: " + kit.find(app, "cp.header").title)
            win.picker().pick("cat")
        },
        function () {
            kit.check(win.picker().shownName === "Ginger", "an existing life keeps its name")
            win.find("cp.go").tapped()
            var c = ctl.last("pet.start")
            kit.check(c && c.params.life_id === ctl.barState.life.life_id && c.params.species === "cat" && c.params.mode === "gentle"
                      && c.params.name === undefined, "Let's go! arms the existing life gently: " + JSON.stringify(c ? c.params : null))
        },
        // ---- first run while Off: Turn on, never Let's go!
        function () { win.load("off-no-pet") },
        function () {
            kit.check(win.find("cp.go").text === "Turn on", "Off first run offers Turn on")
            var starts = ctl.count("pet.start")
            win.find("cp.go").tapped()
            kit.check(ctl.last("app.set_enabled").params.enabled === true && ctl.count("pet.start") === starts, "Turn on sends app.set_enabled, no pet.start")
        },
        // ---- Change: stage row, stars, This one!
        function () { win.load("idle-happy"); app.go("change", {}) },
        function () {
            kit.check(win.shown("cp.stages") && win.shown("cp.stars") && win.shown("cp.back"), "Change shows the stage row, stars and Back")
            kit.check(kit.find(app, "cp.header").title === "Turn Ginger into…", "Change title")
            win.picker().pick("dog")
        },
        function () {
            win.find("cp.go").tapped()
            var c = ctl.last("pet.select")
            kit.check(c && c.params.pet_id === "dog" && c.params.life_id === ctl.barState.life.life_id, "This one! sends pet.select dog")
            return 60
        },
        function () { kit.check(app.screen === "home", "This one! goes back Home") },
        // ---- memorial: Meet a new pet opens the picker for this popup only
        function () { win.load("memorial") },
        function () {
            kit.check(kit.texts(app).some(function (t) { return t.text === "Ginger has gone. Thank you for caring." }), "memorial line")
            win.find("cp.meetNew").tapped()
        },
        function () { kit.check(app.screen === "first_run", "Meet a new pet -> first run") }
    ]))
}
