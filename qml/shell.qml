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
import "omarchy/BarState.js" as BarState

// Quickshell 0.3.1 host entry. Quickshell only loads QML inside the config folder
// (this file's directory), so the entry lives at qml/ where shared/, omarchy/ and
// assets/ are all inside it. Launch explicitly; this file installs no autostart.
// Only this host and omarchy/ import compositor APIs. See omarchy/README.md for
// supported behavior, integration inputs and the live-Wayland validation gate.
// packaging/omarchy/codepet-overlay is the verb launcher (show/hide/toggle/tasks/...).
ShellRoot {
    id: shell
    property string preferredScreenName: Quickshell.env("CODEPET_SCREEN")
    property bool reducedMotion: Quickshell.env("CODEPET_REDUCED_MOTION") === "1"
    property bool explicitlyHidden: false
    property bool taskWindowOpen: false
    // Window tabs, in order. "tasks" is the default view of the focusable window.
    readonly property var views: ["care", "tasks", "companion", "life"]
    property string panelView: "tasks"
    // Clicking a TabButton breaks a currentIndex binding, so the tabs follow here.
    onPanelViewChanged: viewTabs.currentIndex = Math.max(0, shell.views.indexOf(shell.panelView))

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
    HostAdapter {
        id: hostAdapter
        clientTag: "overlay"
        command: [Quickshell.env("CODEPET_BIN") || "codepet", "rpc"]
        onCompanionRequested: shell.openView("companion")
        onLifeRequested: shell.openView("life")
        // Re-read (never RPC) after an authoritative refresh, so a bar-state.json
        // created after startup is picked up and watched.
        onSnapshotReplaced: barWatcher.reload()
    }

    // Passive input: care done from the Omarchy bar shows on the pet. The watcher
    // only parses the file; the adapter merges the newer subset without RPC.
    BarStateWatcher {
        id: barWatcher
        path: BarState.statePath(Quickshell.env("XDG_STATE_HOME"), Quickshell.env("HOME"))
        onUpdated: state => hostAdapter.applyBarState(state)
    }

    // Shows a window view. A v0.2 Panel exposes the five views and openLife(); an
    // older Panel only has its home view, which holds care and tasks together. A
    // draft form or review in progress is kept when its own Tasks tab is reopened.
    function applyPanelView() {
        var inTaskDetail = taskPanel.view === "form" || taskPanel.view === "review"
        if (typeof taskPanel.openLife === "function") {
            if (!(shell.panelView === "tasks" && inTaskDetail))
                taskPanel.view = shell.panelView
        } else if (!inTaskDetail) {
            taskPanel.view = "home"
        }
    }
    function openView(name) {
        if (shell.views.indexOf(name) < 0 || shell.sessionBlocked || shell.selectedScreen === null)
            return
        shell.panelView = name
        shell.applyPanelView()
        shell.taskWindowOpen = true
        taskWindow.minimized = false
        if (!hostAdapter.busy)
            taskPanel.refresh()
    }
    function openTasks() {
        shell.openView("tasks")
    }
    function hidePet() {
        shell.explicitlyHidden = true
    }
    // One explicit state.get; skipped while another request is in flight.
    function refresh() {
        if (!hostAdapter.busy)
            hostAdapter.request("state.get", {}, function () {})
    }

    // Explicit local UI control. These methods never start or retry work.
    IpcHandler {
        target: "codepet"
        // Read-only one-line JSON status for the launcher's `status` verb and tests.
        // Holds no task text, folders or provider output.
        function status(): string {
            var snap = hostAdapter.snapshot
            return JSON.stringify({
                "pet": shell.explicitlyHidden ? "hidden" : (shell.surfaceActive ? "shown" : "suppressed"),
                "window": taskWindow.visible,
                "view": shell.panelView,
                "busy": hostAdapter.busy,
                "state_version": snap && snap.state_version !== undefined ? snap.state_version : null,
                "appearance": snap && snap.life ? snap.life.appearance : null,
                "stage": snap && snap.life ? snap.life.stage : null
            })
        }
        // Liveness probe for packaging/omarchy/codepet-overlay; always this prefix.
        function ping(): string {
            return "codepet-overlay " + (shell.explicitlyHidden ? "hidden" : "shown")
        }
        function show(): void { shell.explicitlyHidden = false }
        function hide(): void { shell.hidePet() }
        function toggle(): void { shell.explicitlyHidden = !shell.explicitlyHidden }
        function tasks(): void { shell.openTasks() }
        function care(): void { shell.openView("care") }
        function companion(): void { shell.openView("companion") }
        function life(): void { shell.openView("life") }
        function refresh(): void { shell.refresh() }
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
        // v0.2 DesktopPet menu: "Choose look…" opens the Companion tab.
        Connections {
            target: pet
            ignoreUnknownSignals: true
            function onChooserRequested() { shell.openView("companion") }
        }
    }

    // A normal focusable toplevel is opened only by an explicit action. It is
    // independent of the layer-shell surface (no parentWindow relationship).
    FloatingWindow {
        id: taskWindow
        title: "CodePet"
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
                    Accessible.name: "Close the window; keep CodePet and its task running"
                    onClicked: shell.taskWindowOpen = false
                }
            }
            // Care · Tasks · Companion · Life. Text input for adoption and retirement
            // lives in this focusable window, never on the click-through overlay.
            TabBar {
                id: viewTabs
                objectName: "shell.viewTabs"
                Layout.fillWidth: true
                currentIndex: Math.max(0, shell.views.indexOf(shell.panelView))
                Repeater {
                    model: ["Care", "Tasks", "Companion", "Life"]
                    TabButton {
                        required property string modelData
                        required property int index
                        text: modelData
                        Accessible.name: modelData + " tab"
                        onClicked: {
                            shell.panelView = shell.views[index]
                            shell.applyPanelView()
                        }
                    }
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

    // Keeps the tabs in step when the panel changes its own view (for example
    // "See Life" inside the care card). Older panels without these views are ignored.
    Connections {
        target: taskPanel
        function onViewChanged() {
            if (shell.views.indexOf(taskPanel.view) >= 0)
                shell.panelView = taskPanel.view
        }
    }

    // One startup read, never polling or provider execution. CODEPET_OPEN (set by
    // the launcher) opens one view; the read is already in flight, so opening the
    // window does not issue a second request.
    Component.onCompleted: {
        hostAdapter.request("state.get", {}, function () {})
        var open = String(Quickshell.env("CODEPET_OPEN") || "")
        if (shell.views.indexOf(open) >= 0)
            shell.openView(open)
    }
}
