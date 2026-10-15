pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "shared"
import "omarchy"
import "omarchy/OverlayPolicy.js" as OverlayPolicy

// Quickshell 0.3.1 host entry. Quickshell only loads QML inside the config folder
// (this file's directory), so the entry lives at qml/ where shared/, omarchy/ and
// assets/ are all inside it. Launch explicitly; this file installs no autostart.
// Only this host and omarchy/ import compositor APIs. See omarchy/README.md for
// supported behavior, integration inputs and the live-Wayland validation gate.
ShellRoot {
    id: shell
    property string preferredScreenName: Quickshell.env("CODEPET_SCREEN")
    property bool reducedMotion: Quickshell.env("CODEPET_REDUCED_MOTION") === "1"
    property bool explicitlyHidden: false
    property bool taskWindowOpen: false

    // Integration inputs, NOT detectors. Bind these to trustworthy host events
    // before claiming automatic lock/sleep/fullscreen suppression. In particular,
    // WlSessionLock creates a lock; it does not observe another locker's state.
    property bool sessionLocked: false
    property bool sessionSuspended: false
    property bool selectedScreenFullscreen: false
    readonly property bool sessionBlocked: sessionLocked || sessionSuspended
    readonly property var selectedScreen: OverlayPolicy.selectScreen(Quickshell.screens, preferredScreenName)
    readonly property bool surfaceActive: !explicitlyHidden && !sessionBlocked
                                         && !selectedScreenFullscreen && selectedScreen !== null

    // The transport and all per-job processes outlive either window. Hiding or
    // closing a window never cancels a task and never destroys the adapter.
    HostAdapter { id: hostAdapter }

    function openTasks() {
        if (shell.sessionBlocked || shell.selectedScreen === null)
            return
        shell.taskWindowOpen = true
        taskWindow.minimized = false
        if (!hostAdapter.busy)
            taskPanel.refresh()
    }
    function hidePet() {
        shell.explicitlyHidden = true
    }

    // Explicit local UI control. These methods never start or retry work.
    IpcHandler {
        target: "codepet"
        function show(): void { shell.explicitlyHidden = false }
        function hide(): void { shell.hidePet() }
        function tasks(): void { shell.openTasks() }
        function closeTasks(): void { shell.taskWindowOpen = false }
        function setReducedMotion(value: bool): void { shell.reducedMotion = value }
        function selectScreen(name: string): void { shell.preferredScreenName = name }
        function setSessionState(locked: bool, suspended: bool): void {
            shell.sessionLocked = locked
            shell.sessionSuspended = suspended
        }
        function setFullscreen(value: bool): void { shell.selectedScreenFullscreen = value }
    }

    // One persistent scene: moving/removing screens must not destroy a care
    // controller while a request callback is in flight. Null screen is accepted
    // by Quickshell; the surface is hidden until a connected screen is selected.
    // Opposite anchors resize the scene in logical pixels; DesktopPet clamps it.
    PanelWindow {
        id: overlay
        screen: shell.selectedScreen
        visible: shell.surfaceActive
        color: "transparent"
        anchors { left: true; right: true; top: true; bottom: true }
        exclusiveZone: 0
        WlrLayershell.namespace: "codepet-overlay"
        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

        readonly property var petInput: OverlayPolicy.clipHit(pet.petHit, width, height, shell.surfaceActive)
        readonly property var menuInput: OverlayPolicy.clipHit(pet.menuHit, width, height, shell.surfaceActive)

        // Empty parent + two Combine regions = union of the interactive hit boxes.
        // Care props are decorative (no handler), so they stay click-through: the bed
        // can remain visible for a whole nap and must not become a dead click zone.
        // Never use the full scene Item as a mask, or a null mask while hidden.
        mask: Region {
            intersection: Intersection.Combine
            Region {
                x: overlay.petInput.x; y: overlay.petInput.y
                width: overlay.petInput.width; height: overlay.petInput.height
                intersection: Intersection.Combine
            }
            Region {
                x: overlay.menuInput.x; y: overlay.menuInput.y
                width: overlay.menuInput.width; height: overlay.menuInput.height
                intersection: Intersection.Combine
            }
        }

        DesktopPet {
            id: pet
            anchors.fill: parent
            host: hostAdapter
            surfaceActive: shell.surfaceActive && overlay.visible
            reducedMotion: shell.reducedMotion
            onReducedMotionRequested: enabled => shell.reducedMotion = enabled
            onTaskPanelRequested: shell.openTasks()
            onHideRequested: shell.hidePet()
        }
    }

    // A normal focusable toplevel is opened only by an explicit action. It is
    // independent of the layer-shell surface (no parentWindow relationship).
    FloatingWindow {
        id: taskWindow
        title: "CodePet · Tasks and review"
        screen: shell.selectedScreen
        visible: shell.taskWindowOpen && !shell.sessionBlocked && shell.selectedScreen !== null
        implicitWidth: 760
        implicitHeight: 820
        minimumSize: Qt.size(420, 360)
        color: "#f7f8fa"
        onClosed: shell.taskWindowOpen = false

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: 16
            spacing: 12
            RowLayout {
                Layout.fillWidth: true
                Label {
                    Layout.fillWidth: true
                    text: "CodePet · Simulation only"
                    textFormat: Text.PlainText
                    font.bold: true
                }
                Button {
                    text: "Close"
                    Accessible.name: "Close tasks; keep CodePet and its task running"
                    onClicked: shell.taskWindowOpen = false
                }
            }
            ScrollView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                contentWidth: availableWidth
                clip: true
                Panel {
                    id: taskPanel
                    showPetCare: true
                    width: parent.width
                    host: hostAdapter
                }
            }
        }
    }

    // One startup read, never polling or provider execution.
    Component.onCompleted: hostAdapter.request("state.get", {}, function () {})
}
