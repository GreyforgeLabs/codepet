pragma ComponentBehavior: Bound
import QtQuick
import Quickshell.Io
import "UpdateModel.js" as Updates

// The detached helper owns installation. This view only runs bounded commands and
// reconnects through durable status after the plugin reloads.
QtObject {
    id: updates
    property string helper: "codepet-update"
    property var childEnv: ({})
    property bool opened: false
    property bool supported: true
    property var snapshot: null
    property bool busy: false
    property string operation: ""
    property var current: null
    property int responseTimeoutMs: 45000
    property bool initialStatus: false
    function failed(message) {
        snapshot = Object.assign({schema: "codepet.update.v1", current: "", latest: "", checked_at: 0}, snapshot || {}, {state: "error", message: message})
    }
    function open() {
        opened = true
        if (busy) return
        initialStatus = true
        request("status")
    }
    function check() { if (!busy) request("check") }
    function install() { if (Updates.canInstall(snapshot, busy)) request("install") }
    function request(verb) {
        if (!supported) {
            failed("Updates are unavailable with a test state or custom backend. Open the normal installed CodePet to update.")
            return false
        }
        if (busy || ["status", "check", "install"].indexOf(verb) < 0) return false
        var job = process.createObject(updates, { verb: verb })
        if (!job) { failed("Could not open the update helper. Reinstall CodePet 0.4 or later."); return false }
        current = job
        operation = verb
        busy = true
        watchdog.restart()
        job.running = true
        return true
    }
    function settled(job) {
        if (job.running || job.finished) return
        job.finished = true
        watchdog.stop()
        if (job.answer) snapshot = job.answer
        else failed(job.failure || "Could not open the update helper. Reinstall CodePet 0.4 or later.")
        var shouldCheck = initialStatus && job.verb === "status" && !!job.answer
            && ["idle", "current", "available"].indexOf(snapshot.state) >= 0
        initialStatus = false
        current = null
        busy = false
        operation = ""
        job.destroy()
        if (shouldCheck && opened) Qt.callLater(check)
    }
    property Component process: Component {
        Process {
            id: child
            required property string verb
            property var answer: null
            property string failure: ""
            property bool finished: false
            command: [updates.helper, verb]
            environment: updates.childEnv
            stdout: SplitParser {
                splitMarker: "\n"
                onRead: line => {
                    if (child.answer || child.failure) return
                    child.answer = Updates.parse(line)
                    if (!child.answer) {
                        child.failure = "The update helper returned an unreadable answer. Try checking again."
                        child.signal(15)
                    }
                }
            }
            onRunningChanged: if (!running) Qt.callLater(() => updates.settled(child))
        }
    }
    property Timer watchdog: Timer {
        objectName: "codepetUpdate.watchdog"
        repeat: false
        interval: updates.responseTimeoutMs
        onTriggered: {
            if (!updates.current) return
            updates.current.failure = "The update helper did not answer. Reopen Updates to check its progress."
            updates.current.signal(9)
        }
    }
    property Timer poll: Timer {
        objectName: "codepetUpdate.poll"
        interval: 1200
        repeat: true
        running: updates.opened && Updates.active(updates.snapshot)
        onTriggered: if (!updates.busy) updates.request("status")
    }
    Component.onDestruction: {
        if (current) { current.signal(15); current.destroy() }
    }
}
