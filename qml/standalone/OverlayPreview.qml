import QtQuick
import QtQuick.Controls
import "../shared"

// Portable fixture preview of the v0.3 desktop pet. Deliberately a normal window:
// ordinary Wayland clients cannot promise compositor-global positioning or click-through.
// The pet draws a bar-state.json-shaped document picked below; a click on it opens the
// "Home" window, which stands in for the bar popup (the real overlay calls the bar
// plugin through the Omarchy shell).
ApplicationWindow {
    id: root
    objectName: "overlayPreview"
    width: 1100; height: 720
    minimumWidth: 480; minimumHeight: 420
    property bool reducedMotion: false
    property string face: "happy"
    readonly property var faces: ["happy", "working", "needs you", "hungry", "asleep", "all done"]
    visible: true
    title: "CodePet · desktop pet preview (fixtures only)"
    color: "#17242b"
    palette.window: "#202c33"
    palette.windowText: "#e9eeea"
    palette.text: "#e9eeea"
    palette.button: "#33434a"
    palette.buttonText: "#edf2ed"
    palette.base: "#172127"
    palette.highlight: "#a9d5ba"

    // A bar-state.json document for the picked face (times relative to now).
    function doc(name) {
        var now = Date.now()
        var d = {
            schema: "codepet.bar.v2", state_version: 1, db_id: "db-" + "0".repeat(24),
            life: {life_id: "life-" + "0".repeat(24), ordinal: 1, name: "Ginger", appearance: "cat", stage: 2,
                   level: 2, xp: 120, status: "healthy", mode: "gentle", armed: true, pace: "steady",
                   paused: false, fainted: false, death_pending: false},
            needs: [{need: "hunger", action: "food", state: name === "hungry" ? "empty" : "ok"}],
            care: {sleeping: name === "asleep", sleep_remaining_ms: 0, sleep_started_at_ms: null,
                   sleep_due: false, bathroom_due: false},
            unseen_events: [], enabled: true, treats: {jar: name === "all done" ? 1 : 0, full: false}, clients: {},
            agent: {state: "idle", apps: [], stale_at_ms: null, finished_seq: 0, finished_at_ms: null,
                    finished_app: null, finished_treat: false, finished_jar_full: false, hello_seq: 0,
                    hello_at_ms: null, hello_app: null, test: null, error_at_ms: null, error_app: null,
                    helpers_busy: false}
        }
        if (name === "working" || name === "needs you") {
            var st = name === "working" ? "working" : "waiting"
            d.agent.state = st
            d.agent.apps = [{id: "claude-code", state: st, since_ms: now - 5000, stale_at_ms: now + 600000}]
            d.agent.stale_at_ms = now + 600000
        }
        if (name === "all done") {
            d.agent.finished_seq = 1
            d.agent.finished_at_ms = now - 3000
            d.agent.finished_app = "claude-code"
            d.agent.finished_treat = true
        }
        return d
    }

    Column {
        x: 36; y: 40; spacing: 12
        Label { text: "A little company. Room to work."; font.pixelSize: 30; color: "#e9eeea" }
        Label { text: "Click your pet to open Home."; color: "#a0b4ba" }
        Label { text: "Fixture preview · no saved care, no coding apps, no real desktop input masking"; color: "#7f989f"; font.pixelSize: 12 }
        Row {
            spacing: 12
            ComboBox {
                objectName: "overlayPreview.face"
                model: root.faces
                onActivated: root.face = currentText
            }
            Button { text: pet.visible ? "Hide pet" : "Show pet"; onClicked: pet.visible = !pet.visible }
            CheckBox { text: "Calm mode"; checked: root.reducedMotion; onToggled: root.reducedMotion = checked }
        }
    }
    DesktopPet {
        id: pet
        objectName: "overlayScene"
        anchors.fill: parent
        barState: root.doc(root.face)
        reducedMotion: root.reducedMotion
        surfaceActive: root.visible && root.visibility !== Window.Minimized && visible
        onHomeRequested: homeWindow.show()
        onHideRequested: visible = false
    }
    ApplicationWindow {
        id: homeWindow
        objectName: "overlayHomeWindow"
        width: 480; height: 640
        visible: false
        title: "CodePet · Home (the bar popup in Omarchy)"
        palette: root.palette
        color: "#202c33"
        Label {
            anchors.centerIn: parent
            text: "Home opens in the bar popup."
            color: "#e9eeea"
        }
    }
}
