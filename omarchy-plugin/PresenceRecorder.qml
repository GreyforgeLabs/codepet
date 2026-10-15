import QtQuick
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Hyprland
import "PresenceModel.js" as Presence

// Attended-presence recorder (spec §5.1). BarWidget creates it only on the bar leader
// (the first instance across all screens), so one recorder runs per shell.
//
// It counts a minute only while all of these hold: the `presence` setting is on, Rust
// says presence is wanted for this exact life (alive, armed, not paused, no slot task,
// no pending ending), the user has not been idle for 180 s, and the focused workspace
// has no fullscreen window. Sleep is deliberately not checked here: Rust discards the
// nap portion itself. Locking leads to idle (the lock screen takes the input), and the
// idle look-back drops the minutes before idle was noticed.
//
// The only file it writes is <stateDir>/presence.json, after a commit, at most once per
// tick, and only once the bar-state.json load proved the state directory exists. A
// failed write freezes time; it never retries from a timer.
Item {
    id: recorder
    visible: false

    property bool presenceSetting: true
    property var barState: null
    property string stateDir: ""
    property bool stateDirReady: false
    // Bumped on every commit so bindings (the predicted badge) re-read attendedSince().
    property int revision: 0
    property var model: null
    property string writeError: ""

    readonly property var presence: barState && barState.presence && typeof barState.presence === "object" ? barState.presence : null
    readonly property string lifeId: presence && Presence.validLifeId(presence.life_id) ? presence.life_id : ""
    readonly property bool wanted: presence !== null && presence.wanted === true && lifeId !== ""
    readonly property bool idle: idleMonitor.isIdle
    readonly property bool fullscreen: Hyprland.focusedWorkspace ? Hyprland.focusedWorkspace.hasFullscreen === true : false
    readonly property bool eligible: presenceSetting && wanted && stateDirReady && stateDir !== "" && !idle && !fullscreen

    function attendedSince(wallMs) {
        void recorder.revision
        return Presence.attendedSince(recorder.model, wallMs)
    }

    // A session never spans two lives: a new life (adoption, an ending) or a new
    // recorder load starts a fresh session, and the old unconsumed tail is dropped.
    function ensureSession(now) {
        if (recorder.lifeId === "") return false
        if (recorder.model && recorder.model.life_id === recorder.lifeId) return true
        var session = Presence.newSession(now, Math.random)
        recorder.model = Presence.create(session, recorder.lifeId, now)
        recorder.revision += 1
        return recorder.model !== null
    }

    function tick() {
        var now = Date.now()
        if (!recorder.ensureSession(now)) return
        var committed = Presence.onTick(recorder.model, now, recorder.eligible, recorder.idle)
        if (committed > 0) {
            recorder.revision += 1
            if (Presence.valid(recorder.model, now)) presenceFile.setText(Presence.payload(recorder.model))
        }
    }

    onLifeIdChanged: {
        if (recorder.model && recorder.model.life_id !== recorder.lifeId) {
            recorder.model = null
            recorder.revision += 1
        }
    }
    onEligibleChanged: {
        if (!recorder.model) recorder.ensureSession(Date.now())
        if (!recorder.eligible) Presence.stop(recorder.model)
    }
    onIdleChanged: if (recorder.idle) Presence.stop(recorder.model)

    IdleMonitor {
        id: idleMonitor
        enabled: true
        timeout: 180
        // Video players' inhibitors do not count as being at the computer.
        respectInhibitors: false
    }

    Timer {
        id: presenceTick
        objectName: "codepet.presenceTick"
        interval: 60000
        repeat: true
        running: recorder.eligible
        onRunningChanged: if (running && recorder.ensureSession(Date.now())) Presence.start(recorder.model, Date.now())
        onTriggered: recorder.tick()
    }

    FileView {
        id: presenceFile
        path: recorder.stateDir !== "" ? recorder.stateDir + "/presence.json" : ""
        preload: false
        atomicWrites: true
        printErrors: false
        onSaveFailed: error => recorder.writeError = "presence.json could not be written; time is frozen."
        onSaved: recorder.writeError = ""
    }
}
