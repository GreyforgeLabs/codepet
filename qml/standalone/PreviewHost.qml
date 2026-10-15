import QtQml

// Pure-QML preview host. It is NOT connected to Rust: it serves fixed fixture
// snapshots and logs the request envelope it would have sent. It never changes
// meters, never stores or simulates rewards, and refuses every state-changing
// method with PREVIEW_ONLY. Draft and preflight replies are canned so the form
// can be inspected; the preflight token is not valid anywhere.
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

    readonly property var providers: [
        { "id": "mock", "label": "Mock", "available": true, "note": "Simulation only; runs no command" },
        { "id": "codex", "label": "Codex", "available": false, "note": "Not yet supported on this version" },
        { "id": "opencode", "label": "OpenCode", "available": false, "note": "Not yet supported on this version" },
        { "id": "hermes", "label": "Hermes", "available": false, "note": "Not yet supported on this version" },
        { "id": "claude_code", "label": "Claude Code", "available": false, "note": "Not yet supported on this version" }
    ]
    readonly property var fixtureNames: ["idle", "running", "review_not_verified", "review_check_failed", "recovery_required"]

    function summary(id, state, outcome, verification) {
        var s = {
            "task_id": id, "revision": 2, "state": state,
            "category": "mental_health", "provider": "mock",
            "folder": "/preview/example-project", "scope": ["docs"],
            "instructions": "Preview fixture: write a short plan for the parser refactor.",
            "mock": { "outcome": outcome, "step_ms": 0 },
            "created_at_ms": 1760400000000, "updated_at_ms": 1760400060000,
            "review": null, "receipt": null
        }
        if (verification !== "")
            s.review = {
                "digest": "preview0000000000000000000000000000000000000000000000000000000000",
                "verification": verification,
                "artifact": "PREVIEW FIXTURE ARTIFACT\n<b>Markup stays literal</b>\nNo command ran; this text is static.",
                "simulated": true
            }
        return s
    }
    // state.get lists briefs (docs/milestone-1.md): no folder, scope or artifact text.
    function brief(s) {
        if (!s)
            return null
        return {
            "task_id": s.task_id, "revision": s.revision, "state": s.state,
            "category": s.category, "provider": s.provider, "mock": s.mock,
            "instructions_excerpt": s.instructions, "instructions_bytes": s.instructions.length,
            "instructions_sha256": "(preview: not computed)",
            "created_at_ms": s.created_at_ms, "updated_at_ms": s.updated_at_ms,
            "review": s.review ? { "digest": s.review.digest, "verification": s.review.verification,
                                   "simulated": s.review.simulated, "artifact_bytes": s.review.artifact.length } : null,
            "receipt": s.receipt
        }
    }
    function fullTasks(name) {
        var list = [summary("preview-failed", "failed", "provider_failure", ""),
                    summary("preview-draft", "draft", "artifact", "")]
        var active = activeFor(name)
        if (active)
            list.push(active)
        return list
    }
    function activeFor(name) {
        if (name === "running")
            return summary("preview-running", "running", "artifact", "")
        if (name === "review_not_verified")
            return summary("preview-review", "review_ready", "artifact", "not_verified")
        if (name === "review_check_failed")
            return summary("preview-review", "review_ready", "check_failed", "failed")
        if (name === "recovery_required")
            return summary("preview-recovery", "recovery_required", "artifact", "not_verified")
        return null
    }
    function buildSnapshot(name) {
        var active = root.brief(root.activeFor(name))
        var recent = root.fullTasks(name).map(root.brief).reverse()
        return {
            "schema_version": 1, "protocol_version": 1, "last_seq": 0,
            "meters": { "hunger": 7000, "thirst": 7000, "mental_health": 7000, "hygiene": 7000 },
            "decay": { "enabled": false, "paused": false },
            "settings": { "revision": 1, "decay_enabled": false, "decay_per_hour": [100, 100, 50, 50], "timeout_seconds": 1800 },
            "active_task": active,
            "recent": recent,
            "providers": root.providers
        }
    }
    function respond(id, method, params) {
        if (method === "state.get")
            return { "v": 1, "id": id, "ok": true, "result": root.buildSnapshot(root.fixture) }
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
        var id = requestId || ("preview-" + root.seq)
        var envelope = { "v": 1, "id": id, "method": method, "params": params || {} }
        console.log("[codepet preview] would send:", JSON.stringify(envelope).slice(0, 2000))
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
