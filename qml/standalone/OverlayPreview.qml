import QtQuick
import QtQuick.Controls
import "../shared"

// Portable fixture preview only. This is deliberately a normal window: ordinary
// Wayland clients cannot promise compositor-global positioning or click-through.
ApplicationWindow {
    id: root
    objectName: "overlayPreview"
    width: 1100; height: 720
    minimumWidth: 480; minimumHeight: 420
    property bool reducedMotion: false
    visible: true
    title: "CodePet · overlay scene preview (fixtures only)"
    color: "#17242b"
    palette.window: "#202c33"
    palette.windowText: "#e9eeea"
    palette.text: "#e9eeea"
    palette.button: "#33434a"
    palette.buttonText: "#edf2ed"
    palette.base: "#172127"
    palette.highlight: "#a9d5ba"
    PreviewHost { id: host }
    Column {
        x: 36; y: 40; spacing: 12
        Label { text: "A little company. Room to work."; font.pixelSize: 30; color: "#e9eeea" }
        Label { text: "Hover over your pet for care. Click to open its menu."; color: "#a0b4ba" }
        Label { text: "Fixture preview · no saved care, running commands or real desktop input masking"; color: "#7f989f"; font.pixelSize: 12 }
        Row {
            spacing: 12
            ComboBox {
                model: host.fixtureNames
                onActivated: { host.fixture = currentText; host.request("state.get", {}, function () {}) }
            }
            Button { text: pet.visible ? "Hide pet" : "Show pet"; onClicked: pet.visible = !pet.visible }
            CheckBox { text: "Reduced motion"; checked: root.reducedMotion; onToggled: root.reducedMotion = checked }
        }
    }
    DesktopPet {
        id: pet
        objectName: "overlayScene"
        anchors.fill: parent
        host: host
        reducedMotion: root.reducedMotion
        onReducedMotionRequested: enabled => root.reducedMotion = enabled
        surfaceActive: root.visible && root.visibility !== Window.Minimized && visible
        onHideRequested: visible = false
        onTaskPanelRequested: taskWindow.show()
    }
    // v0.2 DesktopPet menu: "Choose look…" opens the Companion view (ignored by older pets).
    Connections {
        target: pet
        ignoreUnknownSignals: true
        function onChooserRequested() { root.openView("companion") }
    }
    Connections {
        target: host
        function onCompanionRequested() { root.openView("companion") }
        function onLifeRequested() { root.openView("life") }
    }
    function openView(name) {
        panel.view = name
        taskWindow.show()
    }
    ApplicationWindow {
        id: taskWindow
        objectName: "overlayTaskWindow"
        width: 680; height: 820
        minimumWidth: 480; minimumHeight: 440
        visible: false
        title: "CodePet · tasks and review (simulation)"
        palette: root.palette
        color: "#202c33"
        onVisibleChanged: if (visible && !host.busy) panel.refresh()
        ScrollView {
            anchors.fill: parent
            contentWidth: availableWidth
            clip: true
            Panel {
                id: panel
                x: 20; width: parent.width - 40
                host: host
                showPetCare: true
            }
        }
    }
    Component.onCompleted: host.request("state.get", {}, function () {})
}
