import QtQml

// Pure-QML preview host. It is NOT connected to Rust: it serves fixed fixture
// snapshots and logs only the request ID and method (never user payloads). It never changes
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
    readonly property var fixtureNames: ["idle", "ready", "bathroom_due", "sleep_due", "sleeping", "demo_level_2", "running", "review_not_verified", "review_check_failed", "recovery_required"]

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
        var ready = name === "ready" || name === "demo_level_2"
        var sleeping = name === "sleeping"
        var due = name === "sleep_due" || sleeping
        var bathroom = name === "bathroom_due"
        var meter = ready || due || bathroom ? 10000 : 7000
        var reasons = []
        if (!ready && meter < 10000) reasons.push("Needs food, water, play and a wash")
        if (bathroom) reasons.push("Needs a bathroom break at the tree")
        if (due) reasons.push(sleeping ? "Resting. Wake or refresh after the nap" : "Needs a two-minute nap")
        if (active) reasons.push("Finish or resolve the active task first")
        return {
            "schema_version": 2, "protocol_version": 1, "last_seq": 3,
            "meters": { "hunger": meter, "thirst": meter, "mental_health": meter, "hygiene": meter },
            "care": { "revision": 1, "bathroom_due": bathroom, "starts_since_bathroom": bathroom ? 2 : 0,
                "sleeping": sleeping, "sleep_due": due, "sleep_remaining_ms": sleeping ? 92000 : 120000,
                "sleep_started_at_ms": sleeping ? Date.now() - 28000 : null,
                "work_sessions": due ? 3 : 0, "execution_ms": 0,
                "ready_to_work": ready && !active, "readiness_reasons": reasons, "welcome_care_available": name === "idle" },
            "progression": { "real": { "lifetime_xp": 0, "level": 1, "next_level_xp": 80,
                "mastery": { "bugfix": 0, "quality_performance": 0, "learning_specification": 0, "cleanup": 0 } },
                "demo": { "lifetime_xp": name === "demo_level_2" ? 80 : 0, "level": name === "demo_level_2" ? 2 : 1,
                    "next_level_xp": name === "demo_level_2" ? 200 : 80,
                    "mastery": { "bugfix": 0, "quality_performance": 0, "learning_specification": name === "demo_level_2" ? 80 : 0, "cleanup": 0 } } },
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
        var id = requestId || ("preview-" + root.seq)
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
