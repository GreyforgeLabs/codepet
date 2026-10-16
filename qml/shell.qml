pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import "shared"
import "omarchy"
import "omarchy/OverlayPolicy.js" as OverlayPolicy
import "omarchy/BarState.js" as BarState
import "omarchy/OverlayFlow.js" as OverlayFlow

// Quickshell 0.3.1 host entry of the CodePet desktop pet (SPEC-v0.3 §3.8, §8). Quickshell
// only loads QML inside the config folder (this file's directory), so the entry lives at
// qml/ where shared/, omarchy/ and assets/ are all inside it. Launch explicitly; this file
// installs no autostart. packaging/omarchy/codepet-overlay is the verb launcher.
//
// The overlay is one click-through layer-shell pet. It draws bar-state.json (watched,
// never polled) and has no window of its own: everything you do with CodePet happens in
// the bar popup, which a click on the pet opens at Home. The process exits by itself when
// CodePet is Off or "Pet on my desktop" is off (omarchy/OverlayFlow.js).
ShellRoot {
    id: shell
    property string preferredScreenName: Quickshell.env("CODEPET_SCREEN")
    property bool reducedMotion: Quickshell.env("CODEPET_REDUCED_MOTION") === "1"
    property bool explicitlyHidden: false
    // The last valid bar-state.json, and "Pet on my desktop" (null = unknown).
    property var barState: null
    property var desktopPet: null
    property bool startupReadDone: false
    property bool leaving: false
    property int homeRequests: 0
    readonly property string exitReason: OverlayFlow.exitReason(shell.barState, shell.desktopPet)

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
                                         && exitReason === ""

    onExitReasonChanged: if (shell.exitReason !== "") shell.leave()

    // Explicit reads only: the startup read when bar-state.json is missing or unreadable,
    // and the `refresh` verb. A file event never launches codepet.
    HostAdapter {
        id: hostAdapter
        clientTag: "overlay"
        command: [Quickshell.env("CODEPET_BIN") || "codepet", "rpc"]
        // The answer may have (re)created bar-state.json: re-read it (a read, not RPC).
        onAnswered: barWatcher.reload()
    }

    BarStateWatcher {
        id: barWatcher
        path: BarState.statePath(Quickshell.env("XDG_STATE_HOME"), Quickshell.env("HOME"))
        onUpdated: state => shell.barState = state
        onUnavailable: shell.startupRead()
    }

    // "Pet on my desktop": the bar plugin's overlayOnLoad setting, read-only.
    FileView {
        id: settingsFile
        path: OverlayFlow.settingsPath(Quickshell.env("XDG_CONFIG_HOME"), Quickshell.env("HOME"))
        watchChanges: true
        printErrors: false
        onLoaded: shell.desktopPet = OverlayFlow.desktopPetSetting(text())
        onLoadFailed: shell.desktopPet = null
        onFileChanged: reload()
    }

    // At most one startup state.get, and only when there is no readable file to draw.
    function startupRead() {
        if (shell.startupReadDone || shell.barState !== null)
            return
        shell.startupReadDone = true
        hostAdapter.request("state.get", {}, null)
    }
    // One explicit state.get; skipped while another request is in flight.
    function refresh() {
        barWatcher.reload()
        if (!hostAdapter.busy)
            hostAdapter.request("state.get", {}, null)
    }
    // Opens the bar popup at Home (the bar plugin's `open()` through Omarchy's IPC).
    function openHome() {
        if (shell.exitReason !== "")
            return
        shell.homeRequests += 1
        Quickshell.execDetached(OverlayFlow.openHomeArgv(Quickshell.env("OMARCHY_PATH")))
    }
    function hidePet() {
        shell.explicitlyHidden = true
    }
    // Off or "Pet on my desktop" off: the process exits (SPEC-v0.3 §7.3).
    function leave() {
        if (shell.leaving)
            return
        shell.leaving = true
        console.log("codepet-overlay: exiting (" + shell.exitReason + ")")
        Qt.quit()
    }

    // Explicit local control. These methods never start, stop or answer a coding app.
    IpcHandler {
        target: "codepet"
        // Read-only one-line JSON status for the launcher's `status` verb and tests. No
        // timers, no names of coding apps' content. `supervisor` is always false: v0.3 has
        // no simulation task (kept for the launcher's `quit` check).
        function status(): string {
            var s = shell.barState
            var life = s && s.life ? s.life : null
            return JSON.stringify({
                "pet": shell.explicitlyHidden ? "hidden" : (shell.surfaceActive ? "shown" : "suppressed"),
                "busy": hostAdapter.busy,
                "supervisor": false,
                "enabled": s ? s.enabled !== false : null,
                "desktop_pet": shell.desktopPet,
                "face": pet.face.state,
                "overlay": pet.face.overlay,
                "drawn": pet.active,
                "home_requests": shell.homeRequests,
                "state_version": s ? s.state_version : null,
                "appearance": life ? life.appearance : null,
                "stage": life ? life.stage : null
            })
        }
        // Liveness probe for packaging/omarchy/codepet-overlay; always this prefix.
        function ping(): string {
            return "codepet-overlay " + (shell.explicitlyHidden ? "hidden" : "shown")
        }
        function show(): void { shell.explicitlyHidden = false }
        function hide(): void { shell.hidePet() }
        function toggle(): void { shell.explicitlyHidden = !shell.explicitlyHidden }
        function openHome(): void { shell.openHome() }
        // `panel` opens the bar popup; the v0.2 window verbs are kept as aliases for the
        // v0.2 bar menu and launcher until the v0.3 bar plugin replaces them.
        function panel(): void { shell.openHome() }
        function tasks(): void { shell.openHome() }
        function care(): void { shell.openHome() }
        function companion(): void { shell.openHome() }
        function life(): void { shell.openHome() }
        function refresh(): void { shell.refresh() }
        function setReducedMotion(value: bool): void { shell.reducedMotion = value }
        function selectScreen(name: string): void { shell.preferredScreenName = name }
        function setSessionState(locked: bool, suspended: bool): void {
            shell.sessionLocked = locked
            shell.sessionSuspended = suspended
        }
        function setFullscreen(value: bool): void { shell.selectedScreenFullscreen = value }
    }

    // One persistent scene: moving/removing screens must not destroy the scene. Null
    // screen is accepted by Quickshell; the surface is hidden until a connected screen is
    // selected. Opposite anchors resize the scene in logical pixels; DesktopPet clamps it.
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

        // The pet's hit box is the only input region; the thought bubble is decorative and
        // stays click-through. Never use the full scene Item as a mask, or a null mask
        // while hidden (an empty region is click-through everywhere).
        mask: Region {
            x: overlay.petInput.x; y: overlay.petInput.y
            width: overlay.petInput.width; height: overlay.petInput.height
        }

        DesktopPet {
            id: pet
            anchors.fill: parent
            barState: shell.barState
            surfaceActive: shell.surfaceActive && overlay.visible
            reducedMotion: shell.reducedMotion
            onHomeRequested: shell.openHome()
        }
    }

    // Startup: read the file (no RPC when it is there). CODEPET_OPEN is ignored: the
    // launcher sends its verb (for example `panel`) right after starting the overlay.
    Component.onCompleted: barWatcher.reload()
}
