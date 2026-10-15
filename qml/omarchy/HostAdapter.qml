pragma ComponentBehavior: Bound
import QtQuick
import Quickshell.Io
import "BarState.js" as BarState

// Quickshell host for the shared CodePet panel. Each request starts one short
// `codepet rpc` process, writes one JSON request line to its stdin, and reads one
// response line from its stdout. No timers poll; requests happen only when a
// component asks (explicit actions, or the panel opening).
//
// Quickshell 0.3.1 API used (verified in /usr/lib/qt6/qml/Quickshell/Io/quickshell-io.qmltypes):
//   Process: command (list<string>), running, stdinEnabled, stdout/stderr (DataStreamParser),
//            write(string), signal(int), started(), exited(exitCode, exitStatus), runningChanged()
//   SplitParser: splitMarker, read(data)
// Closing stdin by setting stdinEnabled=false after write() follows existing local usage;
// `codepet rpc` reads one newline-terminated frame and does not need EOF.
//
// Host contract additions for v0.2 (SPEC §12): `clientTag` prefixes request ids,
// `presentationLocks` counts running presentation sequences (PetCare routines),
// `openCompanion()` / `openLife()` ask the host to show those views. The passive
// `bar-state.json` subset is merged by `applyBarState()`, which never issues RPC.
QtObject {
    id: root
    property var snapshot: null
    property bool busy: false
    property string lastError: ""
    // Request-id prefix so the overlay's ids are distinguishable from the bar's.
    property string clientTag: "qs"
    // Shared components increment this while a presentation sequence runs.
    property int presentationLocks: 0
    // Last newer-than-ever bar-state.json subset (see BarStateWatcher.qml).
    property var lastBarState: null

    // View requests from shared components; the host entry decides what to open.
    signal companionRequested()
    signal lifeRequested()
    // Emitted after an authoritative state.get replaced the snapshot.
    signal snapshotReplaced()
    readonly property bool supervisorActive: liveSupervisors > 0
    property int liveSupervisors: 0

    // argv for the RPC command; never run through a shell.
    property var command: ["codepet", "rpc"]
    // Bound on how long a request may wait for its response line.
    property int responseTimeoutMs: 15000
    readonly property int maxFrameBytes: 65536
    property int seq: 0

    function utf8Bytes(s) {
        var n = 0
        for (var i = 0; i < s.length; i++) {
            var c = s.charCodeAt(i)
            if (c < 0x80) n += 1
            else if (c < 0x800) n += 2
            else if (c >= 0xd800 && c <= 0xdbff) { n += 4; i++ }
            else n += 3
        }
        return n
    }
    function newId() {
        root.seq += 1
        return root.clientTag + "-" + Date.now().toString(36) + "-" + root.seq
    }
    function clientError(id, code, message) {
        return { "v": 1, "id": id, "ok": false, "error": { "code": code, "message": message, "retryable": false } }
    }
    function openCompanion() {
        root.companionRequested()
    }
    function openLife() {
        root.lifeRequested()
    }
    // Merges a parsed, strictly newer bar-state.json into the presentation snapshot.
    // Pure data: never requests, so a file event can never launch Rust. A subset that
    // arrives before the first state.get is kept and applied once a snapshot exists.
    function applyBarState(bar) {
        if (!bar || typeof bar.state_version !== "number")
            return false
        if (root.lastBarState && bar.state_version <= root.lastBarState.state_version)
            return false
        root.lastBarState = bar
        if (root.snapshot === null || !BarState.isNewer(root.snapshot, bar))
            return false
        root.snapshot = BarState.merge(root.snapshot, bar)
        return true
    }
    // Completes the in-flight request exactly once.
    function finish(method, resp, callback) {
        root.busy = false
        if (resp.ok === true && method === "state.get") {
            var snap = resp.result
            // A newer passive subset (for example bar care committed while this
            // state.get was in flight) stays visible over the older full snapshot.
            if (snap && root.lastBarState && BarState.isNewer(snap, root.lastBarState))
                snap = BarState.merge(snap, root.lastBarState)
            root.snapshot = snap
            root.snapshotReplaced()
        }
        if (resp.ok !== true) {
            var e = resp.error || {}
            root.lastError = String(e.code || "ERROR") + ": " + String(e.message || "")
        }
        if (callback)
            callback(resp)
    }
    function request(method, params, callback, requestId) {
        if (root.busy) {
            if (callback)
                callback(root.clientError(null, "CLIENT_BUSY", "Another request is still running."))
            return
        }
        var id = requestId || root.newId()
        var frame = JSON.stringify({ "v": 1, "id": id, "method": method, "params": params || {} })
        if (method !== "state.get")
            root.lastError = ""
        if (root.utf8Bytes(frame) + 1 > root.maxFrameBytes) {
            root.finish(method, root.clientError(id, "OUTPUT_LIMIT", "Request is larger than 64 KiB."), callback)
            return
        }
        root.busy = true
        var job = root.requestComponent.createObject(root, {
            "host": root, "requestId": id, "method": method, "frame": frame + "\n", "callback": callback
        })
        var proc = job as RequestProcess
        if (!proc) {
            root.finish(method, root.clientError(id, "CLIENT_ERROR", "Could not create the request process."), callback)
            return
        }
        proc.begin()
    }

    property Component requestComponent: Component {
        RequestProcess {}
    }

    // One `codepet rpc` invocation. Destroyed only after the process has exited, so a
    // task.start process (the per-job mock supervisor) is never killed by cleanup.
    component RequestProcess: Process {
        id: proc
        // Inline components cannot see the adapter's ids, so it is passed in.
        required property var host
        property string requestId
        property string method
        property string frame
        property var callback
        property bool answered: false
        property bool settled: false
        property bool counted: false
        property int extraLines: 0
        property string stderrTail: ""

        command: host.command
        stdinEnabled: true
        stdout: SplitParser {
            splitMarker: "\n"
            onRead: data => proc.handleLine(data)
        }
        stderr: SplitParser {
            splitMarker: "\n"
            onRead: data => {
                if (proc.stderrTail.length < 2048)
                    proc.stderrTail += data.slice(0, 512) + "\n"
            }
        }

        property Timer watchdog: Timer {
            interval: proc.host.responseTimeoutMs
            repeat: false
            onTriggered: proc.timedOut()
        }

        function begin() {
            watchdog.start()
            running = true
        }
        function answer(resp) {
            if (answered)
                return
            answered = true
            watchdog.stop()
            // task.start keeps this process alive as the per-job mock supervisor.
            if (method === "task.start" && resp.ok === true && running) {
                counted = true
                host.liveSupervisors += 1
            }
            host.finish(method, resp, callback)
        }
        function handleLine(line) {
            if (answered) {
                extraLines += 1
                return
            }
            if (host.utf8Bytes(line) > host.maxFrameBytes) {
                answer(host.clientError(requestId, "OUTPUT_LIMIT", "Response frame exceeded 64 KiB."))
                return
            }
            var resp
            try {
                resp = JSON.parse(line)
            } catch (e) {
                answer(host.clientError(requestId, "PROTOCOL_MISMATCH", "Response was not JSON."))
                return
            }
            if (!resp || typeof resp !== "object" || resp.v !== 1 || typeof resp.ok !== "boolean"
                    || (resp.id !== requestId && !(resp.ok === false && resp.id === null))) {
                answer(host.clientError(requestId, "PROTOCOL_MISMATCH", "Unexpected response envelope."))
                return
            }
            answer(resp)
        }
        function timedOut() {
            if (answered)
                return
            answer(host.clientError(requestId, "CLIENT_TIMEOUT", "codepet rpc did not answer in time."))
            if (proc.running)
                proc.signal(15)
            else
                Qt.callLater(proc.settle)
        }
        function settle() {
            if (settled || running)
                return
            settled = true
            if (!answered) {
                var tail = stderrTail.trim()
                answer(host.clientError(requestId, "TRANSPORT",
                    "codepet rpc exited without a response" + (tail ? ": " + tail.slice(0, 300) : ".")))
            }
            if (counted)
                host.liveSupervisors -= 1
            proc.destroy()
        }

        onStarted: {
            write(frame)
            stdinEnabled = false
        }
        // Fires on exit and on failure to start. Deferred so stdout lines delivered at
        // exit are handled first. (`exited` is not used: its QProcess::ExitStatus
        // parameter type is not exposed to qmllint.)
        onRunningChanged: if (!running) Qt.callLater(proc.settle)
    }
}
