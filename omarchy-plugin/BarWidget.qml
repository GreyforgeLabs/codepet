import QtQuick
import Quickshell
import Quickshell.Io
import qs.Ui
import qs.Commons
import "CodePetBar.js" as Model
import "DesktopPetFlow.js" as DesktopFlow
import "petstate.js" as PetState
import "Strings.js" as Strings
import "BuildInfo.js" as BuildInfo

// CodePet on the Omarchy bar (plugin greyforge.codepet), v0.3 (SPEC-v0.3 §3.1, §8).
//
// The cell renders the bar-safe <stateDir>/bar-state.json that `codepet` writes after every
// change, watched with a FileView: a file event never launches Rust. RPC happens only on the
// leader's one startup read, when the popup opens, and on explicit taps.
//
// The cell shows the pet's thumbnail (or its static sleep/working frame) and one bar badge
// chosen by the shared priority (petstate.js). It runs no timer except one one-shot that
// re-evaluates the face when the export says something will change (agent.stale_at_ms, a
// celebration window, the error frame); none runs while CodePet is Off (K9). Only the
// "needs you" hand pulses, never in Calm mode.
//
// Clicks: left opens Home (when Off it turns CodePet on and opens Home, one click); right
// opens the small menu (CodePet on/off, Pet on my desktop, Helpers, More). There are no
// tooltips. The leader exposes the IPC target `greyforge.codepet` with openHome(), which
// the desktop pet calls when it is clicked.
//
// Nothing is ever written inside this plugin folder (the shell hot-reloads on any change
// there). The leader writes <stateDir>/presence.json; settings are written only through
// bar.shell.updateEntryInline().
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
    readonly property string runtimeBase: "file://" + paths.runtime
    readonly property bool testState: paths.testState
    readonly property string stateHome: paths.stateHome

    FileView {
        id: shellConfigFile
        path: root.paths.shellConfig
        watchChanges: true
        printErrors: false
        onLoaded: root.fileSettings = Model.ownSettings(text(), root.moduleName, root.settings)
        onLoadFailed: root.fileSettings = null
        onFileChanged: reload()
    }

    // One plugin setting (label, presence, overlayOnLoad, calm), written through the shell.
    function setSetting(key, value) {
        if (["label", "presence", "overlayOnLoad", "calm", "desktopPlacement"].indexOf(key) < 0 || !bar || !bar.shell
                || typeof bar.shell.updateEntryInline !== "function") return false
        var next = {}
        var current = root.effectiveSettings || {}
        for (var k in current) if (k !== "id") next[k] = current[k]
        next[key] = value
        if (bar.shell.updateEntryInline(root.moduleName, next)) root.fileSettings = next
        return true
    }

    // ------------------------------------------------------------- state input
    property var barState: null
    property bool stateMissing: false
    property bool stateDirReady: false
    property var registry: null
    // The last state.get (or snapshot-shaped) result: Help's helpers switch, Memories.
    property var snapshot: null
    property var workState: null
    FileView {
        path: root.paths.workState
        watchChanges: true
        printErrors: false
        onLoaded: root.workState = Model.parseWork(text())
        onLoadFailed: root.workState = null
        onFileChanged: reload()
    }
    UpdateController {
        id: updater
        helper: root.paths.updateHelper
        childEnv: root.paths.childEnv
        supported: !root.paths.testState && root.cfg.binary === "" && Quickshell.env("CODEPET_TEST_STATE") !== "1"
    }
    readonly property bool updateSupported: updater.supported
    readonly property var updateState: updater.snapshot
    readonly property bool updateBusy: updater.busy
    readonly property string updateOperation: updater.operation
    function openUpdates() { updater.open() }
    function closeUpdates() { updater.opened = false }
    function checkUpdates() { updater.check() }
    function installUpdate() { updater.install() }
    function restartBar() { updater.restartBar() }

    // The time the face is evaluated at: set when the file is read, by the one-shot below
    // and when the popup opens.
    property real now: Date.now()
    // Celebrations already shown in the popup (agent_vectors.json priority.celebrate.seen).
    property var seen: ({ finished_seq: 0, hello_seq: 0, test_at_ms: 0 })
    readonly property var face: root.barState ? PetState.currentPriority(root.barState, root.now, root.seen) : null
    readonly property bool isOn: !root.barState || root.barState.enabled !== false
    readonly property var cell: Model.barCell(root.barState, root.face, root.registry, root.cfg, root.cfg.calm)

    FileView {
        id: barStateFile
        path: root.paths.barState
        watchChanges: true
        printErrors: false
        onLoaded: {
            root.barState = Model.parse(text())
            root.now = Date.now()
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

    // Species, frames and kid words, read once. Missing or unparsable gives the painted
    // egg and plain verbs, never an error.
    FileView {
        id: registryFile
        path: root.paths.registry
        printErrors: false
        onLoaded: root.registry = Model.parseRegistry(text())
        onLoadFailed: root.registry = null
    }

    // The face's next change, as one one-shot (SPEC-v0.3 §8 rule 4). Its handler only
    // re-reads the clock; it never sends a request. Never armed while Off (wake_at_ms null).
    readonly property real wakeAt: root.face && root.face.wake_at_ms !== null && root.face.wake_at_ms !== undefined ? root.face.wake_at_ms : 0
    onWakeAtChanged: root.armWake()
    function armWake() {
        if (root.wakeAt > 0 && root.isOn) {
            faceWake.interval = Math.max(50, Math.min(2147483000, root.wakeAt - Date.now() + 20))
            faceWake.restart()
        } else {
            faceWake.stop()
        }
    }
    Timer {
        id: faceWake
        objectName: "codepetBar.faceWake"
        repeat: false
        onTriggered: root.now = Date.now()
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
            onCommitted: lifeId => root.settlePresence(lifeId)
        }
    }

    // A direct IPC target for Home (SPEC-v0.3 §3.8), only on the leader; Omarchy's
    // `shell summon greyforge.codepet` reaches open() below as well.
    IpcHandler {
        target: "greyforge.codepet"
        enabled: root.leader
        function openHome(): void { root.open() }
        function openUpdates(): void { root.openPage("updates") }
        function buildIdentity(): string { return BuildInfo.identity }
        function close(): void { root.close() }
        function toggle(): void { if (root.menu) root.close(); else root.open() }
    }

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
    property bool presenceSettleBusy: false
    function settlePresence(lifeId) {
        var life = root.barState ? root.barState.life : null
        if (root.presenceSettleBusy || !root.leader || !root.isOn || !life
                || life.life_id !== lifeId || life.armed !== true) return
        root.presenceSettleBusy = true
        rpc.request("state.get", {}, function (resp) {
            root.presenceSettleBusy = false
            if (resp.ok) root.snapshot = resp.result
        })
    }

    // Every popup request goes through here: the allow-list, the queue, and a snapshot
    // kept when the answer is one. Each care or treat press gets its own request id; a press
    // repeated soon after an unconfirmed answer reuses that press's id, so the backend
    // applies it at most once (CodePetBar.retryClaim). The widget's own follow-ups
    // (responseEffects: the desktop pet after Let's go! or Turn on) run here, so they happen
    // even when the popup closed while the request was in flight.
    property var careRetry: ({})
    property int careSeq: 0
    function request(method, params, callback) {
        var key = undefined
        if (Model.RETRY_METHODS.indexOf(method) >= 0) {
            root.careSeq += 1
            var claim = Model.retryClaim(root.careRetry, method + JSON.stringify(params || {}), Date.now(), root.careSeq)
            root.careRetry = claim.book
            key = claim.key
        }
        return rpc.request(method, params, function (resp) {
            if (key !== undefined) root.careRetry = Model.retryRelease(root.careRetry, key, resp, Date.now())
            if (resp && resp.ok && resp.result && typeof resp.result === "object" && resp.result.count_helpers !== undefined)
                root.snapshot = resp.result
            root.applyEffects(DesktopFlow.responseEffects(method, params, resp, root.cfg))
            if (callback) callback(resp)
        }, key)
    }
    function applyEffects(effects) {
        for (var i = 0; i < effects.length; i++) {
            if (effects[i].setting !== undefined) root.setSetting(effects[i].setting, effects[i].value)
            if (effects[i].launch !== undefined) root.launch(effects[i].launch)
        }
    }

    // The leader's one startup read: settles presence, exports bar-state.json and creates
    // the boot cursor; then the desktop pet comes back if it was on.
    function startup() {
        if (root.startupDone || !root.leader) return
        root.startupDone = true
        rpc.request("state.get", {}, function (resp) {
            if (resp.ok) root.snapshot = resp.result
            // The desktop pet comes back only when its switch is on, CodePet is On and a pet
            // was started (an unarmed or missing life has nothing to show yet).
            var verb = resp.ok ? DesktopFlow.launchOnLoad(root.cfg, resp.result) : null
            if (verb) root.launch(verb)
        })
    }
    Component.onCompleted: {
        Qt.callLater(root.startup)
        if (bar && typeof bar.registerClickTarget === "function") bar.registerClickTarget(root)
    }
    Component.onDestruction: if (bar && typeof bar.unregisterClickTarget === "function") bar.unregisterClickTarget(root)

    readonly property var launcherVerbs: ["show", "hide", "toggle", "card"]
    // Desktop overlay verbs go through the codepet-overlay launcher as a detached argv,
    // never through a shell string and never as a child of this shell. The configured
    // binary (CODEPET_BIN), a test state home and Calm mode are passed with env(1) as more
    // argv words: in Quickshell 0.3.1 an execDetached context object written as a JS
    // literal launches nothing (verified offscreen).
    function launch(verb) {
        if (root.launcherVerbs.indexOf(verb) < 0) return false
        var argv = [root.paths.launcher, verb]
        if (root.paths.launcherEnv.length > 0) {
            if (root.paths.launcher.indexOf("=") >= 0) return false
            argv = ["/usr/bin/env"].concat(root.paths.launcherEnv, argv)
        }
        Quickshell.execDetached(argv)
        return true
    }

    // Care sounds ship with the plugin and are copied into the installed runtime
    // (current/qml/assets/sounds), next to the pet art. Playback uses that absolute
    // path. The plugin must not resolve its own folder.
    function playCue(rels) {
        if (root.cfg.calm === true || !rels || !rels.length) return
        var base = root.paths.runtime + "qml/"
        for (var i = 0; i < rels.length && i < 2; i++) {
            var rel = rels[i]
            if (typeof rel !== "string" || !/^assets\/sounds\/action\/[a-z]+\.wav$/.test(rel)) continue
            var argv = ["/usr/bin/pw-play", "--volume", "0.4", base + rel]
            Quickshell.execDetached(argv)
        }
    }

    function openWork(kind) {
        if (kind === "compact") { var opened = root.launch("card"); if (opened) root.close(); return opened }
        if (["bug", "research", "build"].indexOf(kind) < 0) return false
        var argv = [root.paths.workLauncher, kind]
        if (root.paths.launcherEnv.length > 0) argv = ["/usr/bin/env"].concat(root.paths.launcherEnv, argv)
        Quickshell.execDetached(argv)
        root.close()
        return true
    }

    // ------------------------------------------------------------- popup
    property bool menu: false
    // "popup" (Home and its pages) or "menu" (the right-click menu).
    property string popupMode: "popup"
    property string startPage: ""
    // The popup is up (PopupApp runs its animation clock only then).
    readonly property bool popupVisible: root.menu
    // Omarchy's bar-widget panel contract: `omarchy-shell shell summon|hide|toggle
    // greyforge.codepet` calls open() / close() and reads `opened` on the focused bar's
    // widget. The desktop pet opens Home this way (notes.md §9 item 1). A summon never
    // switches CodePet on: while Off it opens the Off look, where Turn on is one tap (only a
    // real click on the cell turns it on and opens Home, triggerPress; notes.md §8 item 2).
    readonly property bool opened: root.menu
    function open() {
        if (root.menu && root.popupMode === "popup") return
        root.openHome()
    }

    function close() {
        if (!root.menu) return
        root.noteShown()
        if (popupLoader.item) popupLoader.item.open = false
        root.menu = false
    }
    function closePopup() { root.close() }

    // Opening the popup is an explicit interaction: one state.get (it drains the inbox and
    // settles presence, SPEC-v0.3 §6.3). A popup that is open switches to the asked page.
    function openPopup(mode, page) {
        root.now = Date.now()
        if (root.menu && root.popupMode === mode && popupLoader.item) {
            popupLoader.item.show(page || "")
            return
        }
        if (root.menu) root.close()
        root.popupMode = mode
        root.startPage = page || ""
        root.menu = true
        if (mode === "popup") rpc.request("state.get", {}, function (resp) { if (resp.ok) root.snapshot = resp.result })
    }
    function openHome() { root.openPopup("popup", "") }
    function openPage(page) { root.openPopup("popup", page) }

    // A left click on the Off cell turns CodePet on and opens Home: one click (SPEC-v0.3 §3.1).
    function turnOnAndOpen() {
        root.openPopup("popup", "")
        root.request("app.set_enabled", { enabled: true }, null)
    }

    // When the popup closes, what it showed counts as seen: a celebration ends, and a
    // growth is acknowledged once (pet.events_seen) so it is not shown again.
    function noteShown() {
        var app = popupLoader.item ? popupLoader.item.app : null
        if (root.popupMode !== "popup" || !app || !app.presentedDoc || !app.presentedFace) return
        var doc = app.presentedDoc, face = app.presentedFace
        root.seen = Model.seenAfter(root.seen, doc, face)
        var life = doc.life
        var seq = Model.maxUnseenSeq(doc.unseen_events)
        if (life && seq >= 0 && face.line === "grew")
            rpc.request("pet.events_seen", { life_id: life.life_id, through_seq: seq }, null)
    }

    // Omarchy forwards a click on the bar strip to registered targets while a panel is open.
    function triggerPress(button) {
        if (button === Qt.RightButton) {
            if (root.menu && root.popupMode === "menu") root.close()
            else root.openPopup("menu", "")
        } else if (button === Qt.LeftButton) {
            if (root.menu) root.close()
            else if (!root.isOn) root.turnOnAndOpen()
            else root.openHome()
        }
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
            mode: root.popupMode
            startPage: root.startPage
        }
    }

    // ------------------------------------------------------------- cell
    readonly property color fg: bar ? bar.barForeground : Color.foreground
    readonly property string fam: bar ? bar.fontFamily : Style.font.family
    readonly property real thumbSize: Math.round(root.barSize * 0.76)
    readonly property real badgeSize: Math.max(16, Math.min(20, Math.round(root.barSize * 0.66)))
    readonly property string labelText: root.cell.hi ? Model.t(Strings.STRINGS, "bar.hi", "floor", {}) : root.cell.label

    implicitWidth: vertical ? barSize : cellRow.implicitWidth + Style.spaceReal(12)
    implicitHeight: vertical ? cellRow.implicitHeight + Style.spaceReal(8) : barSize

    Accessible.role: Accessible.Button
    Accessible.name: {
        var line = Model.faceLine(Strings.STRINGS, root.registry, root.barState, root.face)
        var name = root.barState && root.barState.life ? root.barState.life.name : "CodePet"
        return line !== "" ? line : name
    }

    // The label can be the user-chosen pet name: always plain text, never rich text.
    component Label: Text {
        color: root.fg
        font.family: root.fam
        font.pixelSize: Style.font.body
        renderType: Text.NativeRendering
        textFormat: Text.PlainText
    }

    Grid {
        id: cellRow
        anchors.centerIn: parent
        columns: root.vertical ? 1 : 3
        columnSpacing: Style.space(4)
        rowSpacing: Style.space(2)
        horizontalItemAlignment: Grid.AlignHCenter
        verticalItemAlignment: Grid.AlignVCenter

        Item {
            id: thumbBox
            implicitWidth: root.thumbSize
            implicitHeight: root.thumbSize
            opacity: root.cell.opacity
            Image {
                id: thumbImage
                objectName: "codepetBar.thumb"
                anchors.fill: parent
                visible: !root.cell.thumbIsIcon && root.cell.thumb !== "" && status !== Image.Error
                source: !root.cell.thumbIsIcon && root.cell.thumb !== "" ? root.runtimeBase + root.cell.thumb : ""
                sourceSize.width: root.barSize * 2
                sourceSize.height: root.barSize * 2
                fillMode: Image.PreserveAspectFit
                asynchronous: true
                cache: true
                smooth: true
            }
            Icon {
                anchors.centerIn: parent
                icon: root.cell.thumbIsIcon || thumbImage.status === Image.Error || root.cell.thumb === "" ? "ui/egg" : ""
                size: root.thumbSize
            }
        }
        Icon {
            id: badge
            objectName: "codepetBar.badge"
            visible: root.cell.badge !== ""
            icon: root.cell.badge
            size: root.badgeSize
            SequentialAnimation on scale {
                running: root.cell.pulse && root.isOn
                loops: Animation.Infinite
                alwaysRunToEnd: true
                NumberAnimation { to: 1.25; duration: 520; easing.type: Easing.InOutSine }
                NumberAnimation { to: 1; duration: 520; easing.type: Easing.InOutSine }
            }
        }
        Label {
            visible: !root.vertical && text !== ""
            text: root.labelText
            opacity: root.isOn ? 1 : 0.55
        }
    }

    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        cursorShape: Qt.PointingHandCursor
        onClicked: mouse => root.triggerPress(mouse.button)
    }
}
