import QtQuick
import QtQuick.Window
import "../../omarchy-plugin"

Window {
    id: win
    width: 720; height: 1000; visible: true
    SmokeKit { id: kit }
    FakeCtl { id: ctl; registry: kit.registry; runtimeBase: kit.repo + "/" }
    PopupApp { id: app; ctl: ctl; width: implicitWidth }
    function home() { return kit.find(app, "cp.body").item }
    function text(name) { var item = kit.find(app, name); return item ? item.text : "" }
    Component.onCompleted: kit.run([
        function () {
            ctl.barState = kit.fixture("grew-up")
            ctl.workState = { readiness: { ready: true, energy: 100 }, task: { status: "running" } }
        },
        function () {
            kit.check(text("cp.statusLine").indexOf("grew") >= 0, "growth is visible ahead of work readiness")
            ctl.barState = kit.fixture("idle-treats")
            ctl.hold = true
            app.bubbleText = ""
        },
        function () {
            home().giveTreat()
            kit.check(app.bubbleText.indexOf("growing") < 0 && app.actionState === "", "no treat success before response")
            ctl.release(); ctl.hold = false
        },
        function () {
            kit.check(app.bubbleText.indexOf("+5 XP") >= 0, "confirmed treat shows exact reward")
            kit.check(text("cp.treats.count").indexOf("/10") >= 0, "finite jar count stays visible")
            app.clockMs = 1000
            home().care("play")
        },
        function () {
            app.clockMs = 3500
            ctl.responses = { "pet.care": { changed: false } }
            home().care("play")
        },
        function () {
            var pet = kit.find(app, "cp.pet")
            kit.check(app.actionState === "play" && pet.startMs === 3500, "confirmed already-full Play still restarts the animation")
            kit.check(app.bubbleText === app.s("react.full.play", { name: app.life.name }), "already-full Play keeps truthful copy")
            var state = kit.fixture("idle-happy"); state.treats.jar = 0; ctl.barState = state
        },
        function () {
            home().giveTreat()
            kit.check(text("cp.treats.count") === "Treats 0/10", "empty jar remains visible")
            kit.check(app.bubbleText.indexOf("empty") >= 0, "empty jar gives useful feedback")
            var before = ctl.count("pet.care")
            home().care("food")
            kit.check(ctl.count("pet.care") === before + 1, "Snack is free care, separate from work")
        }
    ])
}
