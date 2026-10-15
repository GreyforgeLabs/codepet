import QtQuick
import Quickshell.Io
import "CodePetBar.js" as Model

// Minimal `codepet rpc` transport for the bar: one argv-only Process per request (never
// a shell), one request in flight, a 15 s watchdog, and frames bounded to 64 KiB.
// Only the allowlisted read/care methods may be sent (CodePetBar.ALLOWED_METHODS);
// every task.* method is refused here, so no mock supervisor can ever become a child
// of the Omarchy shell. A hot reload that kills a request mid-flight only rolls the
// backend transaction back.
//
// Quickshell 0.3.1 Process has no error signal: a binary that cannot start is seen as
// `running` returning to false without `started` having fired.
Item {
    id: rpc
    visible: false

    property string binary: ""
    // Extra environment for the child (XDG_STATE_HOME for a test state home); empty = inherit.
    property var childEnv: ({})
    property int responseTimeoutMs: 15000
    readonly property int maxFrameBytes: 65536
    property bool busy: false
    property bool notInstalled: false
    property string lastError: ""
    property int seq: 0
    property var current: null

    signal responded(string method, var response)

    function clientError(id, code, message) {
        return { "v": 1, "id": id, "ok": false, "error": { "code": code, "message": message, "retryable": false } }
    }

    // callback(response) runs exactly once. requestId lets care retries reuse one key.
    function request(method, params, callback, requestId) {
        if (!Model.allowedMethod(method)) {
            if (callback) callback(rpc.clientError(null, "CLIENT_REFUSED", "The bar does not send " + String(method) + "."))
            return false
        }
        if (rpc.busy) {
            if (callback) callback(rpc.clientError(null, "CLIENT_BUSY", "Another request is still running."))
            return false
        }
        rpc.seq += 1
        var id = requestId || Model.newRequestId(Date.now(), rpc.seq)
        var frame = JSON.stringify({ "v": 1, "id": id, "method": method, "params": params || {} })
        if (Model.utf8Bytes(frame) + 1 > rpc.maxFrameBytes) {
            if (callback) callback(rpc.clientError(id, "OUTPUT_LIMIT", "Request is larger than 64 KiB."))
            return false
        }
        var job = jobComponent.createObject(rpc, {
            "requestId": id, "method": method, "frame": frame + "\n", "callback": callback
        })
        if (!job) {
            if (callback) callback(rpc.clientError(id, "CLIENT_ERROR", "Could not create the request process."))
            return false
        }
        rpc.busy = true
        rpc.current = job
        rpcWatchdog.restart()
        job.running = true
        return true
    }

    function finish(job, resp) {
        if (rpc.current === job) {
            rpc.current = null
            rpc.busy = false
            rpcWatchdog.stop()
        }
        rpc.lastError = resp.ok === true ? "" : String((resp.error || {}).code || "ERROR") + ": " + String((resp.error || {}).message || "")
        if (job.callback) job.callback(resp)
        rpc.responded(job.method, resp)
    }

    // Runs only while a request is in flight. Its handler only stops that process and
    // reports the timeout; it never sends a request.
    Timer {
        id: rpcWatchdog
        objectName: "codepetBar.rpcWatchdog"
        interval: rpc.responseTimeoutMs
        repeat: false
        onTriggered: {
            var job = rpc.current
            if (!job) return
            job.answer(rpc.clientError(job.requestId, "CLIENT_TIMEOUT", "codepet rpc did not answer in time."))
            if (job.running) job.signal(15)
            else Qt.callLater(job.settle)
        }
    }

    Component {
        id: jobComponent
        Process {
            id: proc
            property string requestId: ""
            property string method: ""
            property string frame: ""
            property var callback: null
            property bool answered: false
            property bool didStart: false
            property bool settled: false
            property string stderrTail: ""

            command: [rpc.binary, "rpc"]
            environment: rpc.childEnv
            stdinEnabled: true
            stdout: SplitParser {
                splitMarker: "\n"
                onRead: data => proc.handleLine(data)
            }
            stderr: SplitParser {
                splitMarker: "\n"
                onRead: data => {
                    if (proc.stderrTail.length < 1024) proc.stderrTail += String(data).slice(0, 256) + "\n"
                }
            }
            onStarted: {
                didStart = true
                write(frame)
                stdinEnabled = false
            }
            onRunningChanged: if (!running) Qt.callLater(proc.settle)

            function answer(resp) {
                if (answered) return
                answered = true
                rpc.finish(proc, resp)
            }
            function handleLine(line) {
                if (answered) return
                if (Model.utf8Bytes(line) > rpc.maxFrameBytes) {
                    answer(rpc.clientError(requestId, "OUTPUT_LIMIT", "Response frame exceeded 64 KiB."))
                    return
                }
                var resp
                try { resp = JSON.parse(line) } catch (e) {
                    answer(rpc.clientError(requestId, "PROTOCOL_MISMATCH", "Response was not JSON."))
                    return
                }
                if (!resp || typeof resp !== "object" || resp.v !== 1 || typeof resp.ok !== "boolean"
                        || (resp.id !== requestId && !(resp.ok === false && resp.id === null))) {
                    answer(rpc.clientError(requestId, "PROTOCOL_MISMATCH", "Unexpected response envelope."))
                    return
                }
                answer(resp)
            }
            function settle() {
                if (settled || running) return
                settled = true
                if (!answered) {
                    if (!didStart) {
                        rpc.notInstalled = true
                        answer(rpc.clientError(requestId, "NOT_INSTALLED", "CodePet is not installed: run packaging/omarchy/install.sh."))
                    } else {
                        var tail = stderrTail.trim()
                        answer(rpc.clientError(requestId, "TRANSPORT",
                            "codepet rpc exited without a response" + (tail ? ": " + tail.slice(0, 200) : ".")))
                    }
                } else if (didStart) {
                    rpc.notInstalled = false
                }
                proc.destroy()
            }
        }
    }
}
