// Headless smoke test for the preview host and shared components. See qml/README.md.
import QtQuick
import "../shared"
import "../standalone"

Item {
    id: top
    width: 700
    height: 900
    property int step: 0
    property int failures: 0
    property var form: null
    property var review: null

    PreviewHost { id: host; fixture: "ready" }
    Panel { id: panel; width: 680; host: host }

    function find(obj, name) {
        if (!obj)
            return null
        if (obj.objectName === name)
            return obj
        var kids = obj.children || []
        for (var i = 0; i < kids.length; i++) {
            var r = find(kids[i], name)
            if (r)
                return r
        }
        if (obj.contentItem && kids.indexOf(obj.contentItem) === -1) {
            var c = find(obj.contentItem, name)
            if (c)
                return c
        }
        return null
    }
    function check(cond, label) {
        console.log((cond ? "PASS " : "FAIL ") + label)
        if (!cond)
            failures += 1
    }
    function meters() { return JSON.stringify(host.snapshot.meters) }
    property string initialMeters: ""

    Timer {
        interval: 120
        repeat: true
        running: true
        onTriggered: {
            top.step += 1
            var f = top.form, r = top.review
            switch (top.step) {
            case 1:
                panel.refresh()
                break
            case 2:
                top.check(host.snapshot !== null && host.snapshot.meters.hunger === 10000, "ready state.get fixture loaded")
                top.initialMeters = top.meters()
                top.form = top.find(panel, "panel.taskForm")
                top.review = top.find(panel, "panel.taskReview")
                top.check(top.form !== null && top.review !== null, "form/review found")
                panel.openNewTask()
                top.check(top.form.providerId === "mock", "default provider is first available (mock)")
                top.form.setFields("mental_health", "codex", "/tmp/x", ["docs"], "Plan it", "artifact", 0)
                top.check(!top.form.draftValid, "unavailable provider blocks preflight")
                top.form.setFields("mental_health", "mock", "/tmp/x", ["docs"], "Plan it", "artifact", 0)
                top.check(top.form.draftValid, "valid mock draft")
                top.check(!top.find(top.form, "taskForm.start").enabled, "Start disabled before preflight")
                top.find(top.form, "taskForm.preflight").clicked()
                break
            case 4:
                top.check(top.form.preflightFresh, "preflight fresh after save+preflight")
                top.check(!top.find(top.form, "taskForm.start").enabled, "Start disabled without acknowledgement")
                top.find(top.form, "taskForm.ack").checked = true
                top.check(top.find(top.form, "taskForm.start").enabled, "Start enabled with fresh preflight + ack")
                top.form.edited()
                top.check(!top.form.preflightFresh && !top.find(top.form, "taskForm.ack").checked, "edit clears preflight and ack")
                top.check(!top.find(top.form, "taskForm.start").enabled, "Start disabled after edit")
                top.find(top.form, "taskForm.preflight").clicked()
                break
            case 6:
                top.check(top.form.preflightFresh && top.form.revision === 2, "re-preflight on revision 2")
                top.find(top.form, "taskForm.ack").checked = true
                top.find(top.form, "taskForm.start").clicked()
                break
            case 8:
                top.check(host.lastError.indexOf("PREVIEW_ONLY") === 0, "preview start refused: " + host.lastError)
                top.check(!top.form.preflightFresh, "token cleared after Start click")
                top.check(top.meters() === top.initialMeters, "meters unchanged after start")
                host.fixture = "review_not_verified"
                panel.view = "home"
                panel.refresh()
                break
            case 9:
                top.initialMeters = top.meters()
                panel.openReview("preview-review")
                break
            case 11:
                top.check(top.review.task !== null && top.review.verification === "not_verified", "review loaded (not_verified)")
                var accept = top.find(top.review, "taskReview.accept")
                top.check(!accept.enabled, "accept disabled before ack")
                top.find(top.review, "taskReview.ack").checked = true
                top.check(!accept.enabled, "accept disabled without override reason")
                top.find(top.review, "taskReview.reason").text = "useful plan"
                top.check(accept.enabled, "accept enabled with ack + reason")
                accept.clicked()
                break
            case 13:
                top.check(host.lastError.indexOf("PREVIEW_ONLY") === 0, "preview accept refused")
                top.check(top.review.receipt === null, "no receipt in preview")
                top.check(top.meters() === top.initialMeters, "meters unchanged after accept")
                host.fixture = "review_check_failed"
                panel.openReview("preview-review")
                break
            case 15:
                top.check(top.review.verification === "failed" && top.review.verificationText().indexOf("FAILED") !== -1, "failed check shown as FAILED")
                var acc = top.find(top.review, "taskReview.accept")
                top.find(top.review, "taskReview.ack").checked = true
                // 200 CJK characters are 600 UTF-8 bytes: over the 512-byte limit Rust enforces.
                top.find(top.review, "taskReview.reason").text = "\u4e2d".repeat(200)
                top.check(!acc.enabled && top.review.reasonBytes === 600, "600-byte reason disables accept")
                top.find(top.review, "taskReview.reason").text = "\u4e2d".repeat(170)
                top.check(acc.enabled, "510-byte reason enables accept")
                host.fixture = "recovery_required"
                panel.view = "home"
                panel.refresh()
                break
            case 16:
                top.check(panel.activeTask !== null && panel.activeTask.state === "recovery_required", "recovery_required fixture loaded")
                top.check(top.find(panel, "panel.resolveRecovery").visible, "Resolve recovery button shown")
                panel.openReview("preview-recovery")
                break
            case 18:
                top.check(top.review.canAbandon && !top.review.canDecide, "recovery_required: abandon only, no accept")
                var ab = top.find(top.review, "taskReview.abandon")
                top.check(!ab.enabled, "abandon disabled before acknowledgement")
                top.find(top.review, "taskReview.abandonAck").checked = true
                top.check(ab.enabled, "abandon enabled after acknowledgement")
                ab.clicked()
                break
            case 20:
                top.check(host.lastError.indexOf("PREVIEW_ONLY") === 0, "preview abandon refused (would send task.discard)")
                top.check(top.meters() === top.initialMeters, "meters unchanged after abandon")
                panel.view = "home"
                top.form.loadDraftById("preview-draft", function (ok) {
                    top.check(ok, "task.get loaded the draft")
                })
                break
            case 22:
                top.check(top.form.taskId === "preview-draft" && top.form.draftParams().folder === "/preview/example-project"
                    && top.form.draftParams().scope.length === 1, "Continue draft uses the full task.get summary")
                console.log("DONE failures=" + top.failures)
                Qt.exit(top.failures === 0 ? 0 : 3)
                break
            }
        }
    }
}
