import QtQml
import "Fixtures.js" as Fixtures

// Pure-QML preview host. It is NOT connected to Rust: it serves fixed fixture
// snapshots and logs only the request ID and method (never user payloads). It never changes
// meters, never stores or simulates rewards, and refuses every state-changing
// method with PREVIEW_ONLY (including every v0.2 lifecycle mutation: pet.select, pet.adopt,
// pet.rules, pet.configure_life, pet.retire and pet.events_seen). Draft and preflight replies
// are canned so the form can be inspected; the preflight token is not valid anywhere. The
// read-only `lifecycle.rules` and `pet.memorial` answer from Fixtures.js.
QtObject {
    id: root
    property var snapshot: null
    property bool busy: false
    property string lastError: ""
    readonly property bool supervisorActive: false
    readonly property bool preview: true
    property string fixture: "idle"
    property int seq: 0
    property var lastDraft: null
    // v0.2 host contract (qml/README.md): request-id prefix and presentation locks.
    property string clientTag: "preview"
    property int presentationLocks: 0
    // Every method requested, in order (method names only; no payloads), for tests.
    property var methodLog: []
    signal companionRequested()
    signal lifeRequested()
    function openCompanion() { root.companionRequested() }
    function openLife() { root.lifeRequested() }

    readonly property var providers: Fixtures.PROVIDERS
    readonly property var fixtureNames: Fixtures.NAMES

    function summary(id, state, outcome, verification) { return Fixtures.summary(id, state, outcome, verification) }
    function brief(s) { return Fixtures.brief(s) }
    function fullTasks(name) { return Fixtures.fullTasks(name) }
    function activeFor(name) { return Fixtures.activeFor(name) }
    function buildSnapshot(name) { return Fixtures.snapshot(name, Date.now()) }
    function respond(id, method, params) {
        if (method === "state.get")
            return { "v": 1, "id": id, "ok": true, "result": root.buildSnapshot(root.fixture) }
        // Read-only lifecycle methods answer from fixtures; every mutation is refused below.
        if (method === "lifecycle.rules")
            return { "v": 1, "id": id, "ok": true, "result": Fixtures.rulesResult() }
        if (method === "pet.memorial")
            return { "v": 1, "id": id, "ok": true, "result": Fixtures.memorialPage(root.fixture, params) }
        if (method === "events.subscribe") {
            var events = ["draft", "preparing", "running"].map(function (state, index) {
                return {v: 1, event: "task.state", seq: index + 1, task_id: "preview-running",
                    data: {state: state, from: index === 0 ? null : index === 1 ? "draft" : "preparing",
                        revision: 1, at_ms: 1760400000000 + index * 1000}}
            }).filter(function (event) { return event.seq > params.after_seq })
            return {v: 1, id: id, ok: true, result: {events: events, last_seq: 3}}
        }
        if (method === "task.review" || method === "task.get") {
            var tasks = root.fullTasks(root.fixture)
            for (var i = 0; i < tasks.length; i++)
                if (tasks[i].task_id === params.task_id && (method === "task.get" || tasks[i].review))
                    return { "v": 1, "id": id, "ok": true, "result": tasks[i] }
            return { "v": 1, "id": id, "ok": false, "error": { "code": "NOT_FOUND", "message": "Preview fixture has no such task or review.", "retryable": false } }
        }
        if (method === "task.create" || method === "task.update") {
            var d = {
                "task_id": "preview-draft",
                "revision": method === "task.create" ? 1 : params.expected_revision + 1,
                "state": "draft", "category": params.category, "provider": params.provider,
                "folder": params.folder, "scope": params.scope, "instructions": params.instructions,
                "mock": params.mock || null, "created_at_ms": 0, "updated_at_ms": 0,
                "review": null, "receipt": null
            }
            root.lastDraft = d
            return { "v": 1, "id": id, "ok": true, "result": d }
        }
        if (method === "task.preflight" && root.lastDraft && params.task_id === root.lastDraft.task_id
                && params.revision === root.lastDraft.revision) {
            var t = root.lastDraft
            return { "v": 1, "id": id, "ok": true, "result": {
                "task_id": t.task_id, "revision": t.revision,
                "preflight_token": "preview-token-not-valid",
                "expires_at_ms": Date.now() + 300000,
                "summary": {
                    "category": t.category, "provider": t.provider, "folder": t.folder, "scope": t.scope,
                    "instructions_sha256": "(preview: not computed)",
                    "network": "none (mock)",
                    "writable": "none (mock: no project files are read or written)",
                    "timeout_seconds": 1800
                }
            } }
        }
        return { "v": 1, "id": id, "ok": false, "error": {
            "code": "PREVIEW_ONLY",
            "message": "Preview host is not connected to Rust. Nothing was sent, started or stored.",
            "retryable": false
        } }
    }
    function request(method, params, callback, requestId) {
        if (root.busy) {
            if (callback)
                callback({ "v": 1, "id": null, "ok": false, "error": { "code": "CLIENT_BUSY", "message": "Another request is still running.", "retryable": true } })
            return
        }
        root.seq += 1
        root.methodLog = root.methodLog.concat([method])
        var id = requestId || (root.clientTag + "-" + root.seq)
        console.log("[codepet preview] request:", id, method)
        if (method !== "state.get")
            root.lastError = ""
        root.busy = true
        Qt.callLater(function () {
            var resp = root.respond(id, method, params || {})
            root.busy = false
            if (resp.ok && method === "state.get")
                root.snapshot = resp.result
            if (!resp.ok)
                root.lastError = resp.error.code + ": " + resp.error.message
            if (callback)
                callback(resp)
        })
    }
}
