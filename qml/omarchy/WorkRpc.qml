pragma ComponentBehavior: Bound
import QtQuick
import Quickshell.Io

// A short request process never owns the worker. Closing this window leaves the
// supervised job available to the next window, without restarting it.
Item {
    id: rpc
    visible: false
    property string binary: "codepet"
    property bool busy: false
    property var current: null
    property int responseTimeoutMs: 30000
    readonly property int maxFrameBytes: 1048576
    function bytes(text) {
        var n = 0
        for (var i = 0; i < text.length; i++) {
            var c = text.charCodeAt(i)
            if (c < 128) n++
            else if (c < 2048) n += 2
            else if (c >= 0xd800 && c <= 0xdbff && i + 1 < text.length && text.charCodeAt(i + 1) >= 0xdc00 && text.charCodeAt(i + 1) <= 0xdfff) { n += 4; i++ }
            else n += 3
        }
        return n
    }
    function fail(message) { return { ok: false, error: { code: "TRANSPORT", message: message } } }
    function request(method, params, callback) {
        if (rpc.busy) return false
        if (["status", "start", "get", "cancel", "accept", "discard", "repos", "scout", "scout-settings", "fix"].indexOf(method) < 0) return false
        var frame = JSON.stringify({ method: method, params: params || {} }) + "\n"
        if (bytes(frame) > 65536) { callback(fail("That request is too long.")); return false }
        var job = process.createObject(rpc, { frame: frame, callback: callback })
        if (!job) { callback(fail("Could not open CodePet.")); return false }
        rpc.current = job
        rpc.busy = true
        watchdog.restart()
        job.running = true
        return true
    }
    Timer {
        id: watchdog
        interval: rpc.responseTimeoutMs
        onTriggered: {
            if (!rpc.current) return
            if (rpc.current.answered) rpc.current.signal(9)
            else {
                rpc.current.answer(rpc.fail("CodePet did not answer. Reopen Work to check this job."))
                rpc.current.signal(15)
                watchdog.interval = 2000
                watchdog.restart()
            }
        }
    }
    Component {
        id: process
        Process {
            id: child
            required property string frame
            required property var callback
            property bool answered: false
            property bool settled: false
            command: [rpc.binary, "work-rpc"]
            stdinEnabled: true
            stdout: SplitParser {
                splitMarker: "\n"
                onRead: line => {
                    if (child.answered) return
                    var result
                    if (rpc.bytes(line) <= rpc.maxFrameBytes) {
                        try { result = JSON.parse(line) } catch (e) { result = null }
                    }
                    if (!result || typeof result.ok !== "boolean") {
                        child.answer(rpc.fail("CodePet returned an unreadable answer."))
                        child.signal(15)
                    } else child.answer(result)
                }
            }
            onStarted: { write(frame); stdinEnabled = false }
            onRunningChanged: if (!running) Qt.callLater(child.settle)
            function answer(response) {
                if (answered) return
                answered = true
                // Keep the slot until process exit, even after a malformed answer.
                try { callback(response) } catch (e) { console.warn("CodePet work response handler failed") }
            }
            function settle() {
                if (running || settled) return
                settled = true
                if (!answered) answer(rpc.fail("Could not read this job. Check that CodePet is installed."))
                watchdog.stop()
                watchdog.interval = rpc.responseTimeoutMs
                rpc.busy = false
                rpc.current = null
                child.destroy()
            }
        }
    }
}
