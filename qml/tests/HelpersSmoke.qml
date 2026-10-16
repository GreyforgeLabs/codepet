// Helpers of the bar plugin popup (SPEC-v0.3 §2.3, §3.5, §9.3 HelpersSmoke): every row state
// of the helpers_rows fixtures at base sizes 12, 16 and 20, the hint shown at once, Connect
// all without Codex, the row sheets, Stop watching with its question, and the Codex picture
// guide (trust pages and the hooks-off page).
import QtQuick
import QtQuick.Window
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

    readonly property var fixtures: ["clients-mixed", "clients-codex-hooks-off", "clients-codex-new", "clients-disconnect-running",
        "clients-failed-while-connected", "idle-happy"]
    function load(name, doc) {
        ctl.now = 1792123900000
        ctl.barState = doc || kit.fixture(name)
        app.home()
        app.go("helpers", {})
    }
    function helpers() { return kit.find(app, "cp.body").item }
    function find(name) { return kit.find(app, name) }
    function shown(name) { var o = kit.find(app, name); return !!(o && kit.shown(o, app)) }
    function hasText(s) { return kit.texts(app).some(function (t) { return t.text === s }) }

    Component.onCompleted: kit.run(kit.each([12, 16, 20], win.fixtures, function (size, name) {
        win.load(name)
    }, function (size, name) {
        var label = "helpers " + name + " @" + size
        kit.check(app.screen === "helpers", label + ": screen")
        kit.all(app, label)
        var rows = Model.helperRows(ctl.barState.clients, ctl.now, -new Date(ctl.now).getTimezoneOffset())
        var shownRows = Model.CLIENTS.filter(function (c) { return win.shown("cp.helperRow." + c) })
        kit.check(JSON.stringify(shownRows) === JSON.stringify(rows.map(function (r) { return r.id })), label + ": rows " + JSON.stringify(shownRows))
        for (var i = 0; i < rows.length; i++) {
            var want = app.s(rows[i].row, { app: rows[i].id })
            kit.check(win.hasText(want), label + ": " + rows[i].id + " says " + want)
            if (rows[i].hint) kit.check(win.hasText(app.s(rows[i].hint, { app: rows[i].id })), label + ": the hint shows at once for " + rows[i].id)
        }
        kit.check(win.shown("cp.connectAll") === (Model.connectAll(ctl.barState.clients).length > 0), label + ": Connect all only with something to connect")
        if (rows.length === 0) kit.check(win.shown("cp.helpersNone"), label + ": nothing installed says so")
    }).concat(kit.each([12, 16, 20], ["claude-code", "codex", "opencode", "hermes", "grok-cli"], function (size, id) {
        var doc = kit.fixture("clients-mixed")
        win.load("clients-mixed", doc)
        win.helpers().openSheet(id)
    }, function (size, id) {
        var label = "sheet " + id + " @" + size
        kit.all(app, label)
        var row = Model.helperRow(id, ctl.barState.clients[id], ctl.now, -new Date(ctl.now).getTimezoneOffset())
        kit.check(win.shown("cp.sheetStep") === !!row.step, label + ": next step " + row.step)
        kit.check(win.shown("cp.stopWatching") === row.stop_watching, label + ": Stop watching " + row.stop_watching)
        kit.check(win.shown("cp.sheetClose"), label + ": Close")
    })).concat([
        // ---- opening Helpers re-checks the files, nothing else
        function () { ctl.calls = []; win.load("clients-mixed") },
        function () { kit.check(JSON.stringify(ctl.methods()) === '["clients.status"]', "opening Helpers sends clients.status only: " + JSON.stringify(ctl.methods())) },
        // ---- Connect all never includes Codex
        function () {
            win.find("cp.connectAll").tapped()
            var c = ctl.last("clients.connect")
            kit.check(c && JSON.stringify(c.params.clients) === '["hermes"]', "Connect all: " + JSON.stringify(c ? c.params : null))
        },
        // ---- a waiting Claude Code row: the sheet adds /reload-plugins (K1 exception)
        function () {
            var doc = kit.fixture("clients-mixed")
            doc.clients["claude-code"].first_seen_ms = null
            doc.clients["claude-code"].last_seen_ms = null
            win.load("clients-mixed", doc)
        },
        function () {
            kit.check(win.hasText("… Waiting for Claude Code") && win.hasText("Open a new Claude Code window."), "Claude Code waits with the hint")
            win.helpers().openSheet("claude-code")
        },
        function () {
            kit.check(win.hasText("Or type /reload-plugins."), "the Claude Code sheet offers /reload-plugins")
            kit.kidCopy(app, "claude waiting sheet")
            win.find("cp.stopWatching").tapped()
        },
        function () {
            kit.check(win.hasText("Stop watching Claude Code?"), "Stop watching asks first")
            kit.all(app, "stop question @20")
            win.find("cp.keep").tapped()
        },
        function () {
            kit.check(win.helpers().view === "sheet" && ctl.count("clients.disconnect") === 0, "Keep changes nothing")
            win.find("cp.stopWatching").tapped()
        },
        function () { win.find("cp.stop").tapped() },
        function () {
            var c = ctl.last("clients.disconnect")
            kit.check(c && c.params.client === "claude-code", "Stop sends clients.disconnect claude-code")
            kit.find(app, "cp.header").back()
        },
        function () {
            kit.check(app.screen === "helpers" && win.helpers().view === "list", "‹ Back from a sheet returns to the list")
            kit.find(app, "cp.header").back()
        },
        function () { kit.check(app.screen === "home", "‹ Back from the list returns Home") },
        // ---- failed Hermes: Try again
        function () { win.load("clients-mixed") },
        function () { win.helpers().openSheet("hermes") },
        function () {
            kit.check(win.hasText("Hermes is too old."), "the reason shows on the sheet")
            win.find("cp.sheetStep").tapped()
            kit.check(JSON.stringify(ctl.last("clients.connect").params.clients) === '["hermes"]', "Try again connects Hermes")
        },
        // ---- Codex trust: Show me -> the picture guide
        function () { win.helpers().openSheet("codex") },
        function () {
            kit.check(win.find("cp.sheetStep").text === "Show me", "Codex one more step: Show me")
            win.find("cp.sheetStep").tapped()
        },
        function () {
            kit.check(win.helpers().view === "guide", "the guide opens")
            var ok = true
            return 1
        }
    ]).concat([0, 1, 2, 3, 4].map(function (page) {
        return function () {
            win.helpers().guidePage = page
            Qt.callLater(function () {
                kit.check(win.hasText(app.s("codex.step" + (page + 1))), "guide page " + (page + 1) + ": " + app.s("codex.step" + (page + 1)))
                kit.check(win.shown("cp.codexPicture"), "guide page " + (page + 1) + " has a picture")
                kit.sizes(app, "guide page " + (page + 1))
                kit.fits(app, "guide page " + (page + 1))
            })
        }
    })).concat([
        function () { win.find("cp.guideNext").tapped() },
        function () {
            kit.check(win.helpers().view === "sheet", "Done on the last page returns to the sheet")
        },
        // ---- Codex hooks turned off: the off page with Try again
        function () { win.load("clients-codex-hooks-off") },
        function () { win.helpers().openSheet("codex") },
        function () { win.find("cp.sheetStep").tapped() },
        function () {
            kit.check(win.hasText("Codex has hooks turned off."), "the hooks-off page")
            kit.sizes(app, "codex off page")
            win.find("cp.guideRetry").tapped()
        },
        function () {
            kit.check(JSON.stringify(ctl.last("clients.connect").params.clients) === '["codex"]', "Try again connects Codex")
            kit.check(win.helpers().view === "sheet", "and returns to the sheet")
        },
        // ---- a stray refusal never reaches a process
        function () { kit.check(!ctl.request("task.start", {}, null), "task.* refused") }
    ]))
}
