import QtQuick
import Quickshell.Io
import "CodePetBar.js" as Model

// Minimal `codepet rpc` transport for the bar: one argv-only Process per request (never
// a shell), one request in flight (later taps wait in a short queue of at most 8, so a
// care tap is never dropped, SPEC-v0.3 K5), a 15 s watchdog, and request frames bounded
// to 64 KiB.
// A response line is checked against 64 KiB once it is complete; the watchdog bounds a
// line that never ends. A child whose answer was refused here (oversize or not the
// protocol) or that timed out is sent SIGTERM, then SIGKILL if it is still running when
// the watchdog fires again, and no new request starts until it has exited.
// Only the allowlisted v0.3 methods may be sent (CodePetBar.ALLOWED_METHODS); every
// task.* method is refused here, so no mock supervisor can ever become a child of the
// Omarchy shell. clients.connect/disconnect/test return a job id at once (a detached
// `codepet clients-job` does the work), so they never hold this slot for long. A hot reload that kills a request mid-flight only rolls the
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
    // Requests waiting for the slot, oldest first.
    property var pending: []
    readonly property int maxPending: 8

    // Starts the oldest waiting request once the slot is free. Called only after a child
    // has answered or exited, never from a timer.
    function startNext() {
        if (rpc.busy || rpc.pending.length === 0) return
        var next = rpc.pending[0]
        rpc.pending = rpc.pending.slice(1)
        rpc.request(next.method, next.params, next.callback, next.requestId)
    }

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
            if (rpc.pending.length >= rpc.maxPending) {
                if (callback) callback(rpc.clientError(null, "CLIENT_BUSY", "Another request is still running."))
                return false
            }
            rpc.pending = rpc.pending.concat([{ method: method, params: params, callback: callback, requestId: requestId }])
            return true
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

    // keepBusy: the child is still running after a refused answer or a timeout, so the
    // slot stays taken (and the watchdog armed) until it exits.
    function finish(job, resp, keepBusy) {
        var freed = rpc.current === job && !keepBusy
        if (freed) {
            rpc.current = null
            rpc.busy = false
            rpcWatchdog.stop()
        }
        rpc.lastError = resp.ok === true ? "" : String((resp.error || {}).code || "ERROR") + ": " + String((resp.error || {}).message || "")
        if (job.callback) job.callback(resp)
        rpc.responded(job.method, resp)
        if (freed) Qt.callLater(rpc.startNext)
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
            if (job.answered) {
                // Already answered (refused or timed out) and sent SIGTERM; still running.
                if (job.running) job.signal(9)
                else Qt.callLater(job.settle)
                return
            }
            job.answer(rpc.clientError(job.requestId, "CLIENT_TIMEOUT", "codepet rpc did not answer in time."), true)
            if (job.running) {
                job.signal(15)
                rpcWatchdog.restart()
            } else Qt.callLater(job.settle)
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

            function answer(resp, keepBusy) {
                if (answered) return
                answered = true
                rpc.finish(proc, resp, keepBusy === true)
            }
            // A refused answer: report it now, end the child (the real binary has already
            // written its one frame) and keep the slot until it has exited.
            function reject(code, message) {
                answer(rpc.clientError(requestId, code, message), true)
                if (running) signal(15)
            }
            function handleLine(line) {
                if (answered) return
                if (Model.utf8Bytes(line) > rpc.maxFrameBytes) {
                    reject("OUTPUT_LIMIT", "Response frame exceeded 64 KiB.")
                    return
                }
                var resp
                try { resp = JSON.parse(line) } catch (e) {
                    reject("PROTOCOL_MISMATCH", "Response was not JSON.")
                    return
                }
                if (!resp || typeof resp !== "object" || resp.v !== 1 || typeof resp.ok !== "boolean"
                        || (resp.id !== requestId && !(resp.ok === false && resp.id === null))) {
                    reject("PROTOCOL_MISMATCH", "Unexpected response envelope.")
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
                if (rpc.current === proc) {
                    // A refused or timed-out child has now exited: free the slot.
                    rpc.current = null
                    rpc.busy = false
                    rpcWatchdog.stop()
                    Qt.callLater(rpc.startNext)
                }
                proc.destroy()
            }
        }
    }
}
