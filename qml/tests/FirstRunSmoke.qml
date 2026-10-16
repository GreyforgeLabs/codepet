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
    // A second popup that a step closes (destroys) while its request is in flight, as the
    // shell's Loader does on an outside click.
    readonly property var fakeCtl: ctl
    Loader {
        id: shortLived
        active: false
        sourceComponent: Component {
            PopupApp { ctl: win.fakeCtl; width: implicitWidth }
        }
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
            kit.check(win.shown("cp.connectWaiting.claude-code") && win.find("cp.connectWaiting.claude-code").text === "… Waiting for Claude Code",
                      "ready: the row waits for the app's first event")
            kit.all(app, "connect ready @16")
        },
        function () {
            var doc = kit.fixture("ready-check-done")
            doc.clients["claude-code"].first_seen_ms = doc.generated_at_ms
            ctl.barState = doc
        },
        function () { kit.check(!win.shown("cp.connectWaiting.claude-code"), "seen: no more waiting line") },
        // ---- a refused Connect (BUSY) fails its rows at once, with Try again
        function () {
            win.load("idle-happy")
            ctl.responses = { "clients.connect": { ok: false, error: { code: "BUSY" } } }
            app.go("connect", { offered: ["claude-code", "grok-cli"] })
        },
        function () {
            var doc = kit.fixture("idle-happy")
            doc.clients = kit.fixture("clients-mixed").clients
            doc.clients["claude-code"].connected = false
            doc.clients["grok-cli"].connected = false
            ctl.barState = doc
            win.find("cp.connect").tapped()
        },
        function () {
            kit.check(win.shown("cp.connectRetry") && !win.shown("cp.connectDone"), "refused: Try again, no Done")
            kit.check(win.find("cp.connectReason.grok-cli").visible && win.find("cp.connectReason.grok-cli").text === "Oops! Please try again.",
                      "refused: the row says so")
            kit.all(app, "connect refused @16")
            ctl.responses = {}
            var before = ctl.count("clients.connect")
            win.find("cp.connectRetry").tapped()
            kit.check(ctl.count("clients.connect") === before + 1, "Try again sends Connect again")
        },
        function () { kit.check(!win.shown("cp.connectRetry"), "after Try again the rows wait (no stale refusal)") },
        // ---- a row that had failed before Connect does not read failed again until its new
        // job has run (the export shows the job, then the result)
        function () {
            win.load("idle-happy")
            var doc = kit.fixture("idle-happy")
            doc.clients = kit.fixture("clients-mixed").clients
            ctl.barState = doc
            kit.check(doc.clients.hermes.failed_reason === "too_old", "clients-mixed: Hermes failed before")
            app.go("connect", { offered: ["hermes"] })
        },
        function () { win.find("cp.connect").tapped() },
        function () {
            kit.check(!win.shown("cp.connectRetry") && !win.find("cp.connectReason.hermes").visible, "right after Connect the old failure is not shown")
            var doc = JSON.parse(JSON.stringify(ctl.barState))
            doc.clients.hermes.job = { id: "job-0123456789abcdef01234567", kind: "connect", state: "running" }
            ctl.barState = doc
        },
        function () {
            var doc = JSON.parse(JSON.stringify(ctl.barState))
            doc.clients.hermes.job = null
            ctl.barState = doc
        },
        function () {
            kit.check(win.shown("cp.connectRetry") && win.find("cp.connectReason.hermes").text === "Hermes is too old.",
                      "after its job the same failure is the new result: Try again")
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
        function () {
            win.load("fresh-no-pet")
            ctl.calls = []
            ctl.launches = []
            ctl.responses = { "pet.start": { ok: false, error: { code: "ALREADY_ARMED" } }, "clients.status": kit.status("owner-first-run") }
        },
        function () { win.find("cp.go").tapped(); return 60 },
        function () {
            kit.check(app.screen === "first_run" && app.page === "", "ALREADY_ARMED: no error, routes by the export (" + app.screen + ")")
            kit.check(ctl.count("clients.status") === 0 && ctl.launches.length === 0, "ALREADY_ARMED: just Home, no Connect card and no launch " + JSON.stringify(ctl.methods()))
            ctl.responses = {}
        },
        // ---- the popup closes while pet.start is in flight: the answer reaches no destroyed
        // screen, and the desktop pet still comes on (the widget's own follow-up)
        function () {
            ctl.calls = []
            ctl.launches = []
            ctl.settings = []
            ctl.cfg = { label: "name", presence: true, overlayOnLoad: false, calm: false }
            ctl.responses = { "clients.status": kit.status("owner-first-run") }
            ctl.hold = true
            shortLived.active = true
        },
        function () {
            kit.find(shortLived.item, "cp.go").tapped()
            kit.check(ctl.count("pet.start") === 1, "Let's go! sent from the short-lived popup")
            shortLived.active = false
        },
        function () {
            ctl.hold = false
            ctl.release()
            kit.check(ctl.launches.indexOf("show") >= 0 && ctl.settings.some(function (x) { return x.key === "overlayOnLoad" && x.value === true }),
                      "closed popup: the desktop pet still comes on")
            return 60
        },
        function () {
            kit.check(ctl.count("clients.status") === 0, "closed popup: its follow-up (clients.status) is not sent")
            ctl.responses = {}
        },
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
