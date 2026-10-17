pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Window
import "../../omarchy-plugin"
Window {
    id: win
    width: 720; height: 1000; visible: true
    property int phase: 0
    SmokeKit { id: kit }
    FakeCtl { id: ctl; registry: kit.registry; runtimeBase: kit.repo + "/" }
    PopupApp { id: app; ctl: ctl; width: implicitWidth }
    function state(phase, message) {
        ctl.updateState = {schema:"codepet.update.v1",current:"0.4.0",latest:"0.4.1",state:phase,message:message || "",checked_at:123}
    }
    function find(name) { return kit.find(app,name) }
    Component.onCompleted: { ctl.barState = kit.fixture("idle-happy"); app.go("more",{}) }
    Timer {
        interval: 100; repeat: true; running: true
        onTriggered: {
            if (win.phase === 0) {
                win.find("cp.more.updates").tapped()
            } else if (win.phase === 1) {
                kit.check(app.screen === "updates" && ctl.updateCalls[0] === "open", "More opens Updates and checks through host")
                win.state("available", "An update is ready.")
            } else if (win.phase === 2) {
                kit.all(app,"Updates available")
                kit.check(win.find("cp.updates.current").text === "Installed: 0.4.0" && win.find("cp.updates.latest").text === "Latest: 0.4.1", "installed and latest versions are visible")
                kit.check(win.find("cp.updates.action").text === "Update", "one explicit Update action")
                win.find("cp.updates.action").tapped()
                kit.check(ctl.updateCalls[1] === "install", "Update asks host to install")
                ctl.updateBusy = true; ctl.updateOperation = "install"
                win.state("downloading", "Downloading the release.")
            } else if (win.phase === 3) {
                kit.check(!win.find("cp.updates.action").visible, "active update cannot be duplicated")
                kit.check(win.find("cp.updates.heading").text === "Starting your update…", "startup has explanatory progress")
                ctl.updateBusy = false; ctl.updateOperation = ""; win.state("installing", "Installing the verified release.")
            } else if (win.phase === 4) {
                kit.check(!win.find("cp.updates.action").visible && win.find("cp.updates.heading").text === "Installing CodePet…", "durable installer progress remains visible after helper returns")
                win.state("error", "Finish or cancel your current job, then try again.")
            } else if (win.phase === 5) {
                kit.check(win.find("cp.updates.message").text.indexOf("Finish or cancel") === 0, "blocked work has actionable explanation")
                kit.check(win.find("cp.updates.message").textFormat === Text.PlainText, "helper messages remain literal text")
                win.find("cp.updates.action").tapped()
                kit.check(ctl.updateCalls[2] === "check", "error offers Check again")
                win.state("succeeded", "CodePet is updated. Your pet is safe.")
            } else if (win.phase === 6) {
                kit.check(win.find("cp.updates.heading").text === "CodePet is updated", "durable success is clear")
                app.back()
            } else if (win.phase === 7) {
                kit.check(ctl.updateCalls[3] === "close", "leaving Updates stops host polling")
                ctl.barState = kit.fixture("off"); app.go("updates",{})
            } else if (win.phase === 8) {
                kit.check(app.screen === "updates", "explicit Updates remains reachable while pet is Off")
                ctl.updateSupported = false
                win.state("error", "Updates are unavailable with a test state or custom backend.")
            } else if (win.phase === 9) {
                kit.check(!win.find("cp.updates.action").visible, "test/custom backend page offers no production install")
                kit.check(win.find("cp.updates.message").text.indexOf("unavailable") >= 0, "test mode refusal is explained")
                kit.finish()
            }
            win.phase++
        }
    }
}
