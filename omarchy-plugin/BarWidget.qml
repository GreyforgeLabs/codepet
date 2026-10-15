import QtQuick
import Quickshell
import Quickshell.Io
import qs.Ui
import qs.Commons
import "CodePetBar.js" as Model

// CodePet on the Omarchy bar (plugin greyforge.codepet).
//
// Read-mostly: the cell renders the bar-safe <stateDir>/bar-state.json that the
// `codepet` CLI writes after every settling command, watched with a FileView. A file
// event never launches Rust. RPC happens only on: the leader's one startup read, the
// popup opening, the Refresh button and explicit care/sleep/pause clicks.
//
// Nothing is ever written inside this plugin folder (the shell hot-reloads on any
// change there). The leader writes <stateDir>/presence.json; settings are written only
// through bar.shell.updateEntryInline().
BarWidget {
    id: root
    moduleName: "greyforge.codepet"

    // ------------------------------------------------------------- settings
    // Injected settings can predate the last inline write after a hot reload, so this
    // widget's own entry in shell.json is watched and preferred.
    property var fileSettings: null
    readonly property var effectiveSettings: fileSettings || settings || ({})
    readonly property var cfg: Model.config(effectiveSettings)
    readonly property var env: ({
        HOME: Quickshell.env("HOME") || "",
        XDG_STATE_HOME: Quickshell.env("XDG_STATE_HOME") || "",
        XDG_DATA_HOME: Quickshell.env("XDG_DATA_HOME") || "",
        XDG_CONFIG_HOME: Quickshell.env("XDG_CONFIG_HOME") || ""
    })
    readonly property var paths: Model.paths(cfg, env)

    FileView {
        id: shellConfigFile
        path: root.paths.shellConfig
        watchChanges: true
        printErrors: false
        onLoaded: root.fileSettings = Model.ownSettings(text(), root.moduleName, root.settings)
        onLoadFailed: root.fileSettings = null
        onFileChanged: reload()
    }

    function setLabelMode(mode) {
        if (["name", "status", "none"].indexOf(mode) < 0 || !bar || !bar.shell
                || typeof bar.shell.updateEntryInline !== "function") return false
        var next = {}
        var current = root.effectiveSettings || {}
        for (var k in current) if (k !== "id") next[k] = current[k]
        next.label = mode
        if (bar.shell.updateEntryInline(root.moduleName, next)) root.fileSettings = next
        return true
    }

    // ------------------------------------------------------------- state input
    property var barState: null
    property bool stateMissing: false
    property bool stateDirReady: false
    property var registry: null

    FileView {
        id: barStateFile
        path: root.paths.barState
        watchChanges: true
        printErrors: false
        onLoaded: {
            root.barState = Model.parse(text())
            root.stateMissing = false
            root.stateDirReady = true
        }
        onLoadFailed: {
            root.barState = null
            root.stateMissing = true
        }
        // Only re-read the file: a file event never launches Rust.
        onFileChanged: reload()
    }

    // Species names, stage names and care labels, read once. Missing or unparsable
    // gives generic text ("Companion", "stage N", "Food"), never an error.
    FileView {
        id: registryFile
        path: root.paths.registry
        printErrors: false
        onLoaded: root.registry = Model.parseRegistry(text())
        onLoadFailed: root.registry = null
    }

    // ------------------------------------------------------------- leader + presence
    // One bar exists per monitor; the first instance across all screens leads.
    readonly property bool leader: {
        if (!bar || typeof bar.moduleWidgets !== "function") return false
        var items = bar.moduleWidgets(root.moduleName)
        return items.length > 0 && items[0] === root
    }

    Loader {
        id: recorderLoader
        active: root.leader
        sourceComponent: PresenceRecorder {
            presenceSetting: root.cfg.presence
            barState: root.barState
            stateDir: root.paths.stateDir
            stateDirReady: root.stateDirReady
        }
    }

    readonly property int attendedSinceMs: {
        var r = recorderLoader.item
        if (!root.leader || !r || !root.barState) return 0
        return r.attendedSince(root.barState.generated_at_ms)
    }
    readonly property var summary: Model.summarize(root.barState, root.cfg,
        { registry: root.registry, attendedSinceMs: root.attendedSinceMs })

    // ------------------------------------------------------------- transport
    BarRpc {
        id: rpc
        objectName: "codepetBar.rpc"
        binary: root.paths.binary
        childEnv: root.paths.childEnv
        // A response may have re-exported bar-state.json: every instance re-reads the
        // file now, in case a watcher missed the atomic rename or the file's creation.
        onResponded: (method, response) => root.broadcast("reloadState")
    }
    // Called on every instance (all screens) after a response; a file read only.
    function reloadState() {
        barStateFile.reload()
        if (!root.registry) registryFile.reload()
    }
    readonly property bool rpcBusy: rpc.busy
    readonly property bool rpcNotInstalled: rpc.notInstalled
    property bool startupDone: false

    // The leader's one startup read: settles presence, exports bar-state.json and
    // creates the boot cursor.
    function startup() {
        if (root.startupDone || !root.leader) return
        root.startupDone = true
        rpc.request("state.get", {}, null)
        if (root.cfg.overlayOnLoad) root.launch("show")
    }
    Component.onCompleted: Qt.callLater(root.startup)

    readonly property var launcherVerbs: ["show", "hide", "toggle", "tasks", "care", "companion", "life", "refresh"]
    // Desktop overlay verbs go through the codepet-overlay launcher as a detached argv,
    // never through a shell string and never as a child of this shell. A test state home
    // is passed with env(1) as more argv words: in Quickshell 0.3.1 an execDetached
    // context object written as a JS literal launches nothing (verified offscreen).
    function launch(verb) {
        if (root.launcherVerbs.indexOf(verb) < 0) return false
        var argv = [root.paths.launcher, verb]
        if (root.paths.testState) {
            if (root.paths.launcher.indexOf("=") >= 0) return false
            argv = ["/usr/bin/env", "XDG_STATE_HOME=" + root.paths.childEnv.XDG_STATE_HOME].concat(argv)
        }
        Quickshell.execDetached(argv)
        return true
    }

    // ------------------------------------------------------------- popup
    property bool menu: false
    property string grewText: ""
    property bool eventsAcked: false
    property string feedback: ""

    function close() {
        if (popupLoader.item) popupLoader.item.open = false
        root.menu = false
    }

    // Opening the popup is an explicit interaction: one state.get (it settles presence
    // and may record an ending), then an unseen growth is shown once and acknowledged.
    function openPopup() {
        root.menu = true
        root.grewText = ""
        root.eventsAcked = false
        root.feedback = ""
        root.hideTip()
        rpc.request("state.get", {}, function (resp) {
            if (resp.ok) root.noteUnseen(resp.result)
            else root.feedback = "Could not refresh: " + String((resp.error || {}).message || "unknown error")
        })
    }

    function refresh() {
        rpc.request("state.get", {}, function (resp) {
            root.feedback = resp.ok ? "Refreshed." : "Could not refresh: " + String((resp.error || {}).message || "unknown error")
            if (resp.ok) root.noteUnseen(resp.result)
        })
    }

    function noteUnseen(snapshot) {
        var life = snapshot && snapshot.life ? snapshot.life : (root.barState ? root.barState.life : null)
        var events = snapshot && Array.isArray(snapshot.unseen_events) ? snapshot.unseen_events
            : (root.barState ? root.barState.unseen_events : [])
        var seq = Model.maxUnseenSeq(events)
        if (!life || seq < 0 || root.eventsAcked) return
        var stage = 0
        for (var i = 0; i < events.length; i++) if (events[i] && events[i].seq === seq) stage = events[i].stage
        root.grewText = Model.grewText(life.name, root.registry, life.appearance, stage || life.stage)
        var lifeId = life.life_id
        // Acknowledge after the line has been shown.
        Qt.callLater(function () { root.ackSeen(lifeId, seq) })
    }

    function ackSeen(lifeId, seq) {
        if (!root.menu || root.eventsAcked || typeof lifeId !== "string") return
        root.eventsAcked = true
        rpc.request("pet.events_seen", { life_id: lifeId, through_seq: seq }, function (resp) {
            if (!resp.ok) root.eventsAcked = false
        })
    }

    property var careRetry: null
    property int careSeq: 0
    // Care, sleep, wake and pause: one key per press, reused when that same press is
    // retried after an unconfirmed response. Success is shown only after the response.
    function perform(method, params, doneText) {
        if (rpc.busy) return
        var signature = method + JSON.stringify(params)
        root.careSeq += 1
        root.careRetry = Model.retryKey(root.careRetry, signature, Date.now(), root.careSeq)
        root.feedback = "Waiting for CodePet…"
        rpc.request(method, params, function (resp) {
            if (resp.ok) {
                root.careRetry = null
                root.feedback = resp.result && resp.result.changed === false ? "Already done; nothing changed." : doneText
            } else {
                root.feedback = "Not confirmed: " + String((resp.error || {}).message || "refresh to check saved state.")
            }
        }, root.careRetry.key)
    }
    function care(action) {
        if (!Model.allowedCareAction(action)) return
        root.perform("pet.care", { action: action }, "Care confirmed.")
    }
    function sleepOrWake() {
        var sleeping = root.barState && root.barState.care && root.barState.care.sleeping === true
        if (sleeping) root.perform("pet.wake", {}, "Awake.")
        else root.perform("pet.sleep", {}, "Tucked into bed.")
    }
    function togglePause() {
        var paused = root.barState && root.barState.life && root.barState.life.paused === true
        root.perform("pet.pause", { paused: !paused }, paused ? "Time counts again." : "Paused: time isn't counting.")
    }

    Loader {
        id: popupLoader
        active: root.menu
        onLoaded: item.open = true
        sourceComponent: BarPopup {
            widget: root
            anchorItem: root
            bar: root.bar
            owner: root
        }
    }

    // ------------------------------------------------------------- tooltip
    property bool hovered: false
    function hideTip() {
        hoverGrace.stop()
        if (bar && typeof bar.hideTooltip === "function") bar.hideTooltip(root)
    }
    // Mapping the popup window sends spurious exit/enter pairs; act on the settled
    // hover state 250 ms later. The handler never sends a request.
    Timer {
        id: hoverGrace
        objectName: "codepetBar.hoverGrace"
        interval: 250
        repeat: false
        onTriggered: {
            if (!root.bar) return
            if (root.hovered && !root.menu) root.bar.showTooltip(root, Model.tooltip(root.barState, root.cfg,
                { registry: root.registry, attendedSinceMs: root.attendedSinceMs }))
            else if (typeof root.bar.hideTooltip === "function") root.bar.hideTooltip(root)
        }
    }

    // ------------------------------------------------------------- cell
    readonly property color fg: bar ? bar.barForeground : Color.foreground
    readonly property string fam: bar ? bar.fontFamily : Style.font.family
    readonly property string thumb: root.barState && root.barState.life
        ? Model.thumbPath(root.barState.life.appearance, root.barState.life.stage) : ""
    readonly property bool showThumb: thumb !== "" && thumbImage.status !== Image.Error

    implicitWidth: vertical ? barSize : cell.implicitWidth + Style.spaceReal(12)
    implicitHeight: vertical ? cell.implicitHeight + Style.spaceReal(8) : barSize

    Accessible.role: Accessible.Button
    Accessible.name: root.summary.accessibleName

    component Label: Text {
        color: root.fg
        font.family: root.fam
        font.pixelSize: Style.font.body
        renderType: Text.NativeRendering
    }

    Grid {
        id: cell
        anchors.centerIn: parent
        columns: root.vertical ? 1 : 3
        columnSpacing: Style.space(4)
        rowSpacing: Style.space(2)
        horizontalItemAlignment: Grid.AlignHCenter
        verticalItemAlignment: Grid.AlignVCenter

        Item {
            implicitWidth: Math.round(root.barSize * 0.72)
            implicitHeight: Math.round(root.barSize * 0.72)
            Image {
                id: thumbImage
                anchors.fill: parent
                visible: root.showThumb
                source: root.thumb !== "" ? "file://" + root.paths.runtime + root.thumb : ""
                sourceSize.width: root.barSize * 2
                sourceSize.height: root.barSize * 2
                fillMode: Image.PreserveAspectFit
                asynchronous: true
                cache: true
                smooth: true
            }
            Label {
                anchors.centerIn: parent
                visible: !root.showThumb
                text: "CP"
                font.bold: true
            }
        }
        Label {
            id: badge
            visible: text !== ""
            text: root.summary.glyph
            font.bold: true
        }
        Label {
            visible: !root.vertical && text !== ""
            text: root.summary.label
        }
    }

    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
        cursorShape: Qt.PointingHandCursor
        onContainsMouseChanged: {
            root.hovered = containsMouse
            hoverGrace.restart()
        }
        onClicked: mouse => {
            if (mouse.button === Qt.RightButton) root.launch("toggle")
            else if (mouse.button === Qt.MiddleButton) root.launch("tasks")
            else if (root.menu) root.close()
            else root.openPopup()
        }
    }
}
