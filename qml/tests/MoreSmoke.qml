// More, Memories, Say goodbye and the bar cell's right-click menu (SPEC-v0.3 §3.1, §3.6,
// §9.3 MoreSmoke): at most six rows, kid sizes and copy at base sizes 12, 16 and 20, the
// desktop-pet and Calm switches, Bedtime only while awake, the typed-name goodbye.
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
    PopupApp {
        id: menu
        objectName: "menuApp"
        ctl: ctl
        mode: "menu"
        width: implicitWidth
        y: 900
    }

    function load(name, page) {
        ctl.now = 1792123900000
        ctl.barState = kit.fixture(name)
        app.home()
        if (page) app.go(page, {})
    }
    function find(name) { return kit.find(app, name) }
    function shown(name) { var o = kit.find(app, name); return !!(o && kit.shown(o, app)) }
    function hasText(s) { return kit.texts(app).some(function (t) { return t.text === s }) }
    function rows() {
        var n = 0
        var list = kit.controls(app)
        kit.walk(app, function (o) { if (o.visible && o.cpKind === "row") n++; return o.visible })
        return n
    }

    readonly property var cases: [
        { fixture: "idle-happy", page: "more" }, { fixture: "asleep", page: "more" },
        { fixture: "idle-happy", page: "memories" }, { fixture: "idle-happy", page: "goodbye" }
    ]

    Component.onCompleted: kit.run(kit.each([12, 16, 20], win.cases, function (size, c) {
        win.load(c.fixture, c.page)
    }, function (size, c) {
        var label = c.page + " (" + c.fixture + ") @" + size
        kit.check(app.screen === c.page, label + ": screen " + app.screen)
        kit.all(app, label)
        if (c.page === "more") {
            kit.check(win.rows() <= 6, label + ": at most six rows (" + win.rows() + ")")
            kit.check(win.shown("cp.more.bedtime") === (c.fixture !== "asleep"), label + ": Bedtime only while awake")
        }
    }).concat(kit.each([12, 16, 20], ["idle-happy", "off"], function (size, name) {
        win.load(name)
    }, function (size, name) {
        var label = "menu (" + name + ") @" + size
        kit.check(menu.screen === "menu", label + ": menu screen")
        kit.all(menu, label)
        kit.check(kit.find(menu, "cp.menu.helpers").visible === (name !== "off"), label + ": Helpers only while On")
    })).concat([
        // ---- the switches and Bedtime
        function () { win.load("idle-happy", "more") },
        function () {
            var desk = win.find("cp.more.desktop")
            kit.check(!desk.checked, "desktop pet starts off (overlayOnLoad false)")
            desk.toggled(true)
            kit.check(ctl.cfg.overlayOnLoad === true && ctl.launches[ctl.launches.length - 1] === "show", "Pet on my desktop on: setting + show")
        },
        function () {
            win.find("cp.more.desktop").toggled(false)
            kit.check(ctl.cfg.overlayOnLoad === false && ctl.launches[ctl.launches.length - 1] === "hide", "Pet on my desktop off: setting + hide")
            win.find("cp.more.calm").toggled(true)
            kit.check(ctl.cfg.calm === true && app.calm, "Calm mode on")
            win.find("cp.more.calm").toggled(false)
            win.find("cp.more.bedtime").tapped()
        },
        function () { kit.check(ctl.count("pet.sleep") === 1 && app.screen === "home", "Bedtime: pet.sleep, then Home") },
        // ---- pages from More
        function () { win.load("idle-happy", "more") },
        function () { win.find("cp.more.help").tapped() },
        function () { kit.check(app.screen === "help", "How it works opens Help"); app.back() },
        function () { kit.check(app.screen === "more", "‹ Back returns to More"); win.find("cp.more.memories").tapped() },
        function () {
            kit.check(app.screen === "memories" && ctl.count("pet.memorial") >= 1, "Memories asks for the memorial")
            kit.check(win.hasText("Ginger came home."), "the pet of today")
            ctl.responses = { "pet.memorial": { entries: [{ life_id: "life-old", name: "Pip", species: "dog", appearance: "dog", ordinal: 1,
                mode: "gentle", pace: "steady", final_level: 2, final_stage: 2, stage_name_key: "dog.grown", life_xp: 90, tasks_accepted: 0,
                adopted_at_ms: 1790000000000, ended_at_ms: 1790500000000, end_cause: "retired", final_meters: {}, attended_ms: 1 }],
                next_before_ordinal: null } }
            app.back()
        },
        function () { win.find("cp.more.memories").tapped() },
        function () {
            kit.check(win.hasText("Pip has gone.") && win.hasText("A while ago"), "a past pet, with a when-word")
            kit.kidCopy(app, "memories with a past pet")
            ctl.responses = {}
            app.back()
        },
        // ---- Say goodbye: the button appears only for the typed name
        function () { win.find("cp.more.goodbye").tapped() },
        function () {
            kit.check(app.screen === "goodbye" && !win.shown("cp.goodbye"), "Say goodbye waits for the name")
            win.find("cp.goodbyeName").text = "Gin"
        },
        function () {
            kit.check(!win.shown("cp.goodbye"), "a wrong name shows no Say goodbye")
            win.find("cp.goodbyeName").text = " Ginger "
        },
        function () {
            kit.check(win.shown("cp.goodbye"), "the typed name shows Say goodbye")
            kit.all(app, "goodbye typed @20")
            win.find("cp.goodbye").tapped()
            var c = ctl.last("pet.retire")
            kit.check(c && c.params.confirm_name === "Ginger" && c.params.life_id === ctl.barState.life.life_id, "Say goodbye sends pet.retire")
        },
        function () { win.load("idle-happy", "goodbye") },
        function () { var n = ctl.count("pet.retire"); win.find("cp.goodbyeKeep").tapped(); kit.check(ctl.count("pet.retire") === n && app.screen === "home", "Keep keeps") },
        // ---- the right-click menu
        function () { win.load("idle-happy") },
        function () {
            kit.find(menu, "cp.menu.power").toggled(false)
            kit.check(ctl.last("app.set_enabled").params.enabled === false, "menu: CodePet off")
            kit.find(menu, "cp.menu.helpers").tapped()
            kit.find(menu, "cp.menu.more").tapped()
            kit.check(JSON.stringify(ctl.pages) === '["helpers","more"]', "menu: Helpers and More open the popup there " + JSON.stringify(ctl.pages))
            kit.find(menu, "cp.menu.desktop").toggled(true)
            kit.check(ctl.cfg.overlayOnLoad === true && ctl.launches[ctl.launches.length - 1] === "show", "menu: Pet on my desktop")
        }
    ]))
}
