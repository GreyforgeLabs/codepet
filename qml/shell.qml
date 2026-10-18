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
// never polled). Clicking opens a compact work card; Care opens the bar popup. The process exits by itself when
// CodePet is Off or "Pet on my desktop" is off, and follows Calm mode live; both switches
// are the bar entry's settings in ~/.config/omarchy/shell.json (omarchy/OverlayFlow.js).
ShellRoot {
    id: shell
    property string preferredScreenName: Quickshell.env("CODEPET_SCREEN")
    property bool explicitlyHidden: false
    // The last valid bar-state.json, and "Pet on my desktop" (null = unknown).
    property var barState: null
    property var workState: null
    readonly property bool cardOpened: pet.cardOpen
    property var desktopPet: null
    // This process has seen "Pet on my desktop" on (then an off exits at once).
    property bool sawDesktopPetOn: false
    // Calm mode: the bar entry's `calm` (null = unknown), and a `setReducedMotion` call
    // (null = none), which holds until the setting changes.
    property var calmSetting: null
    property var reducedMotionOverride: null
    readonly property bool reducedMotion: OverlayFlow.reducedMotion(shell.reducedMotionOverride, shell.calmSetting,
                                                                    Quickshell.env("CODEPET_REDUCED_MOTION"))
    property bool startupReadDone: false
    property bool leaving: false
    // Home requests sent to the Omarchy shell (coalesced: at most one per second).
    property int homeRequests: 0
    property real lastHomeMs: 0
    property var placement: null
    property string pendingPlacement: ""
    property var careRetries: ({})
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

    onExitReasonChanged: shell.checkExit()
    onDesktopPetChanged: if (shell.desktopPet === true) shell.sawDesktopPetOn = true
    onCalmSettingChanged: shell.reducedMotionOverride = null

    // Explicit reads only: the startup read when bar-state.json is missing or unreadable,
    // the `refresh` verb, and explicit wheel care. A file event never launches codepet.
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
    FileView {
        path: BarState.statePath(Quickshell.env("XDG_STATE_HOME"), Quickshell.env("HOME")).replace(/bar-state\.json$/, "work-state.json")
        watchChanges: true
        printErrors: false
        onLoaded: {
            try { shell.workState = JSON.parse(text()) } catch (e) { shell.workState = null }
        }
        onLoadFailed: shell.workState = null
        onFileChanged: reload()
    }

    WorkController {
        id: work
        binary: Quickshell.env("CODEPET_BIN") || "codepet"
        summary: shell.workState
        pet: shell.barState ? shell.barState.life : null
        calm: shell.reducedMotion
        opened: shell.cardOpened && shell.surfaceActive
        automatic: shell.surfaceActive && !!shell.barState && shell.barState.enabled === true && !!shell.barState.life && shell.barState.life.armed === true
    }
    function openCard() { if (shell.surfaceActive) pet.cardOpen = true }
    function openReview() {
        var binary = Quickshell.env("CODEPET_BIN")
        var launcher = Quickshell.env("CODEPET_WORK_LAUNCHER") || (binary.charAt(0) === "/" ? binary.replace(/[^/]+$/, "codepet-work") : "codepet-work")
        Quickshell.execDetached([launcher, "build"])
        pet.cardOpen = false
    }

    // "Pet on my desktop" (overlayOnLoad) and Calm mode (calm): the bar entry's settings. Placement is written only after an explicit drag or pin.
    FileView {
        id: settingsFile
        path: OverlayFlow.settingsPath(Quickshell.env("HOME"))
        watchChanges: true
        printErrors: false
        onLoaded: shell.readSettings(text())
        onLoadFailed: shell.readSettings("")
        onFileChanged: reload()
    }
    function readSettings(text) {
        shell.desktopPet = OverlayFlow.desktopPetSetting(text)
        shell.calmSetting = OverlayFlow.calmSetting(text)
        if (OverlayFlow.parseSettings(text) !== null) {
            var next = OverlayFlow.placementSetting(text)
            if (JSON.stringify(next) !== JSON.stringify(shell.placement)) {
                // A pin switch in More keeps the pet at its current position, including
                // distance it wandered since the last drag. Persist that position too.
                if (shell.placement && next.pinned !== shell.placement.pinned
                        && next.x === shell.placement.x && next.y === shell.placement.y
                        && next.screen === shell.placement.screen) {
                    pet.pinned = next.pinned
                    pet.savePlacement()
                    return
                }
                shell.placement = next
                if (next.screen !== "") shell.preferredScreenName = next.screen
                pet.restorePlacement(next)
            }
        }
    }

    function savePlacement(x, y, pinned) {
        var next = {x: x, y: y, pinned: pinned,
                    screen: shell.selectedScreen ? shell.selectedScreen.name : ""}
        shell.placement = next
        shell.pendingPlacement = JSON.stringify(next)
        flushPlacement()
    }
    function flushPlacement() {
        if (placementWriter.running || shell.pendingPlacement === "") return
        var next = JSON.parse(shell.pendingPlacement)
        shell.pendingPlacement = ""
        placementWriter.command = OverlayFlow.placementArgv(Quickshell.env("OMARCHY_PATH"), next)
        placementWriter.running = true
    }
    Process {
        id: placementWriter
        stdout: StdioCollector {
            onStreamFinished: if (text.trim() !== "ok") console.warn("codepet-overlay: placement save failed: " + text.trim())
        }
        onExited: (code, status) => {
            if (code !== 0) console.warn("codepet-overlay: placement save exited " + code)
            Qt.callLater(shell.flushPlacement)
        }
    }

    // A "Pet on my desktop" off read before this process saw it on must hold this long:
    // the launch after Let's go! can race the write that turns the switch on.
    Timer {
        id: settleTimer
        objectName: "overlay.settleTimer"
        interval: OverlayFlow.SETTLE_MS
        repeat: false
        onTriggered: if (shell.exitReason !== "") shell.leave()
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
    function careAction(action) {
        if (!shell.surfaceActive || ["food", "water", "play", "wash", "sleep", "wake"].indexOf(action) < 0) return
        if (hostAdapter.busy) { pet.careAnswered(action, { ok: false }); return }
        var life = shell.barState ? shell.barState.life : null
        var method = action === "sleep" ? "pet.sleep" : action === "wake" ? "pet.wake" : "pet.care"
        var params = method === "pet.care" ? { action: action } : {}
        if (action === "wake" && life && life.paused) { method = "pet.pause"; params = { paused: false } }
        var signature = String(life ? life.life_id : "") + ":" + action
        var retry = shell.careRetries[signature]
        var key = retry && retry.until > Date.now() ? retry.id : hostAdapter.newId()
        shell.careRetries[signature] = { id: key, until: Date.now() + 120000 }
        hostAdapter.request(method, params, function (response) {
            if (response.ok) delete shell.careRetries[signature]
            pet.careAnswered(action, response)
        }, key)
    }
    // Opens the bar popup at Home (the bar plugin's `open()` through Omarchy's IPC). A burst
    // of clicks sends one call: requests within a second of the last one are dropped (the
    // pet still hops and shows its heart for every click).
    function openHome() {
        if (shell.exitReason !== "")
            return
        var now = Date.now()
        if (!OverlayFlow.homeDue(shell.lastHomeMs, now))
            return
        shell.lastHomeMs = now
        shell.homeRequests += 1
        Quickshell.execDetached(OverlayFlow.openHomeArgv(Quickshell.env("OMARCHY_PATH")))
    }
    function hidePet() {
        shell.explicitlyHidden = true
    }
    // Off, or "Pet on my desktop" switched off after it was on: exit now. An off read before
    // the switch was ever on here exits once it has held for the settle time.
    function checkExit() {
        if (shell.exitReason === "") {
            settleTimer.stop()
        } else if (OverlayFlow.exitsAtOnce(shell.exitReason, shell.sawDesktopPetOn)) {
            shell.leave()
        } else if (!settleTimer.running) {
            settleTimer.restart()
        }
    }
    // Off or "Pet on my desktop" off: the process exits (SPEC-v0.3 §3.8, §7.3).
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
        // no simulation task (kept for the launcher's `quit` check). `exit_reason` is
        // non-empty while the process is about to exit ("off", "desktop_pet_off"), so a
        // launcher can say the desktop pet is off instead of reporting a failure.
        function status(): string {
            var s = shell.barState
            var life = s && s.life ? s.life : null
            return JSON.stringify({
                "pet": shell.explicitlyHidden ? "hidden" : (shell.surfaceActive ? "shown" : "suppressed"),
                "busy": hostAdapter.busy,
                "supervisor": false,
                "enabled": s ? s.enabled !== false : null,
                "desktop_pet": shell.desktopPet,
                "exit_reason": shell.exitReason,
                "calm": shell.reducedMotion,
                "face": pet.face.state,
                "overlay": pet.face.overlay,
                "drawn": pet.active,
                "home_requests": shell.homeRequests,
                "card_open": pet.cardOpen,
                "card_hit": pet.cardHit,
                "work_busy": work.busy,
                "x": pet.petX,
                "y": pet.petY < 0 ? pet.floorY : pet.petY,
                "pinned": pet.pinned,
                "dragging": pet.dragging,
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
        // Opens the bar popup at Home. The stub `panel` verb and the v0.2 window verbs
        // (`tasks`, `care`, `companion`, `life`) went with the v0.2 bar menu (SPEC-v0.3 §10
        // step 7; notes.md §8 item 21).
        function openHome(): void { shell.openHome() }
        function openCard(): void { shell.openCard() }
        function refresh(): void { shell.refresh() }
        function setReducedMotion(value: bool): void { shell.reducedMotionOverride = value }
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
        WlrLayershell.keyboardFocus: shell.surfaceActive && pet.interactiveOpen ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None

        readonly property var petInput: OverlayPolicy.clipHit(pet.petHit, width, height, shell.surfaceActive)

        readonly property var cardInput: OverlayPolicy.clipHit(pet.cardHit, width, height, shell.surfaceActive)
        readonly property var wheelInput: OverlayPolicy.clipHit(pet.wheelHit, width, height, shell.surfaceActive)

        // Pet, visible card and wheel are separate input islands; the thought bubble is decorative and
        // stays click-through. Never use the full scene Item as a mask, or a null mask
        // while hidden (an empty region is click-through everywhere).
        mask: Region {
            x: overlay.petInput.x; y: overlay.petInput.y
            width: overlay.petInput.width; height: overlay.petInput.height
            Region {
                intersection: Intersection.Combine
                x: overlay.cardInput.x; y: overlay.cardInput.y
                width: overlay.cardInput.width; height: overlay.cardInput.height
                radius: 16
            }
            Region {
                intersection: Intersection.Combine
                x: overlay.wheelInput.x; y: overlay.wheelInput.y
                width: overlay.wheelInput.width; height: overlay.wheelInput.height
                radius: 24
            }
        }

        DesktopPet {
            id: pet
            anchors.fill: parent
            barState: shell.barState
            workState: shell.workState
            workController: work
            onReviewRequested: shell.openReview()
            surfaceActive: shell.surfaceActive && overlay.visible
            reducedMotion: shell.reducedMotion
            onHomeRequested: shell.openHome()
            onUpdatesRequested: Quickshell.execDetached(OverlayFlow.openUpdatesArgv(Quickshell.env("OMARCHY_PATH")))
            onPlacementRequested: (x, y, pinned) => shell.savePlacement(x, y, pinned)
            onCareRequested: action => shell.careAction(action)
            onWidthChanged: if (shell.placement) Qt.callLater(() => pet.restorePlacement(shell.placement))
            onHeightChanged: if (shell.placement) Qt.callLater(() => pet.restorePlacement(shell.placement))
        }
    }

    // Startup: read the file (no RPC when it is there). CODEPET_OPEN is ignored: the
    // launcher sends its verb (for example `panel`) right after starting the overlay.
    Component.onCompleted: barWatcher.reload()
}
