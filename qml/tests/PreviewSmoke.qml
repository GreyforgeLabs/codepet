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
                top.check(!top.find(panel, "panel.recoveryMetadataHint").visible, "no missing-metadata hint when the review is present")
                // A non-null but malformed digest (left by an older build) still needs the hint:
                // task.discard would reject it with INVALID_PARAMS.
                var hint = top.find(panel, "panel.recoveryMetadataHint")
                var saved = host.snapshot
                var damaged = JSON.parse(JSON.stringify(saved))
                damaged.active_task.review.digest = ""
                host.snapshot = damaged
                top.check(hint.visible && hint.text.indexOf("Check interrupted tasks") !== -1, "empty digest shows the recovery hint")
                damaged = JSON.parse(JSON.stringify(saved))
                damaged.active_task.review.digest = "f".repeat(200)
                host.snapshot = damaged
                top.check(hint.visible, "oversized digest shows the recovery hint")
                damaged = JSON.parse(JSON.stringify(saved))
                damaged.active_task.review = null
                host.snapshot = damaged
                top.check(hint.visible, "missing review shows the recovery hint")
                host.snapshot = saved
                top.check(!hint.visible, "hint clears once the saved review is usable again")
                var digestTask = function (d) { return { "review": { "digest": d } } }
                top.check(!panel.reviewDigestUnusable(digestTask("a".repeat(128))), "128-byte digest is within the protocol limit")
                top.check(panel.reviewDigestUnusable(digestTask("a".repeat(129))), "129-byte digest exceeds the protocol limit")
                top.check(panel.reviewDigestUnusable(digestTask("\u4e2d".repeat(43))), "limit counts UTF-8 bytes (43 CJK = 129 bytes)")
                top.check(!panel.reviewDigestUnusable(digestTask("\u4e2d".repeat(42))), "42 CJK = 126 bytes is usable")
                top.check(panel.reviewDigestUnusable(digestTask(null)) && panel.reviewDigestUnusable({ "review": null }),
                          "non-string digest or null review is unusable")
                top.check(panel.recoverNotice({ "changed": [], "repaired": [] }) === "No interrupted tasks found.", "empty recovery result")
                top.check(panel.recoverNotice({ "changed": [], "repaired": ["preview-recovery"] }).indexOf("Recovery metadata restored") === 0,
                          "a metadata-only repair is reported, not 'no interrupted tasks found'")
                top.check(top.review.missingReviewNotice("preview-recovery", { "ok": false, "error": { "code": "INVALID_STATE" } })
                          .indexOf("Check interrupted tasks") !== -1, "task.review without metadata points to recovery")
                top.check(top.review.missingReviewNotice("preview-recovery", { "ok": false, "error": { "code": "NOT_FOUND" } }) === "",
                          "other review errors get no recovery hint")
                panel.openReview("preview-recovery")
                break
            case 18:
                top.check(top.review.canAbandon && !top.review.canDecide, "recovery_required: abandon only, no accept")
                top.check(top.review.verificationText().indexOf("unavailable") !== -1,
                          "damaged review never claims trustworthy verification evidence")
                var ab = top.find(top.review, "taskReview.abandon")
                top.check(!ab.enabled, "abandon disabled before acknowledgement")
                top.find(top.review, "taskReview.abandonAck").checked = true
                top.check(ab.enabled, "abandon enabled after acknowledgement")
                ab.clicked()
                break
            case 20:
                top.check(host.lastError.indexOf("PREVIEW_ONLY") === 0, "preview abandon refused (would send task.discard)")
                top.check(top.meters() === top.initialMeters, "meters unchanged after abandon")
                var shown = top.review.task
                top.review.task = { "task_id": "preview-repaired", "state": "discarded",
                    "review": { "digest": "0".repeat(64), "verification": "failed", "metadata_repaired": true,
                                "artifact": "", "simulated": false }, "receipt": null }
                top.check(top.review.verificationText().indexOf("not a sealed review") !== -1,
                          "repaired review metadata is never shown as a sealed result, even after discard")
                // Malformed non-null digests would be refused with INVALID_PARAMS: no button
                // sends them, and a hint points to recovery instead.
                var abandonBtn = top.find(top.review, "taskReview.abandon")
                var digestHint = top.find(top.review, "taskReview.digestNotice")
                var badDigests = ["", "f".repeat(129), "中".repeat(43), 42]
                for (var bd = 0; bd < badDigests.length; bd++) {
                    top.review.task = { "task_id": "preview-bad-digest", "state": "recovery_required",
                        "review": { "digest": badDigests[bd], "verification": "not_verified", "artifact": "", "simulated": true },
                        "receipt": null }
                    top.find(top.review, "taskReview.abandonAck").checked = true
                    top.check(top.review.canAbandon && !top.review.digestUsable && !abandonBtn.enabled,
                              "malformed digest #" + bd + " keeps Abandon disabled")
                    top.check(digestHint.visible && digestHint.text.indexOf("Check interrupted tasks") !== -1,
                              "malformed digest #" + bd + " shows the recovery hint")
                }
                top.review.task = { "task_id": "preview-bad-digest", "state": "review_ready",
                    "review": { "digest": "f".repeat(129), "verification": "passed", "artifact": "", "simulated": true },
                    "receipt": null }
                top.find(top.review, "taskReview.ack").checked = true
                top.check(!top.find(top.review, "taskReview.accept").enabled && !top.find(top.review, "taskReview.discard").enabled,
                          "malformed digest keeps Accept and Discard disabled")
                top.review.task = { "task_id": "preview-ok-digest", "state": "recovery_required",
                    "review": { "digest": "a".repeat(128), "verification": "not_verified", "artifact": "", "simulated": true },
                    "receipt": null }
                top.check(top.review.digestUsable && !digestHint.visible, "a 128-byte digest is usable and shows no hint")
                top.find(top.review, "taskReview.abandonAck").checked = false
                top.check(top.review.missingReviewNotice("preview-recovery", { "ok": false, "error": { "code": "INVALID_PARAMS" } })
                          .indexOf("Check interrupted tasks") !== -1, "an INVALID_PARAMS refusal points to recovery")
                top.review.task = shown
                panel.view = "home"
                top.form.loadDraftById("preview-draft", function (ok) {
                    top.check(ok, "task.get loaded the draft")
                })
                break
            case 22:
                top.check(top.form.taskId === "preview-draft" && top.form.draftParams().folder === "/preview/example-project"
                    && top.form.draftParams().scope.length === 1, "Continue draft uses the full task.get summary")
                // v0.2 life fixtures: lifecycle mutations are refused like every other mutation.
                host.fixture = "unarmed"
                panel.view = "life"
                panel.refresh()
                break
            case 24:
                top.check(panel.life !== null && panel.life.armed === false, "unarmed fixture loaded")
                top.check(top.find(panel, "life.rulesCard").visible, "unarmed life shows the rules card in the preview")
                top.find(panel, "rulesCard.ack").checked = true
                top.find(panel, "rulesCard.start").clicked()
                break
            case 26:
                top.check(host.lastError.indexOf("PREVIEW_ONLY") === 0, "preview pet.rules refused: " + host.lastError)
                top.check(host.snapshot.life.armed === false, "preview never arms a life")
                host.fixture = "no_life_memorial"
                panel.view = "home"
                panel.refresh()
                break
            case 29:
                var adoption = top.find(panel, "panel.adoption")
                top.check(adoption.visible && adoption.rulesLoaded, "preview serves lifecycle.rules to the adoption form")
                top.find(panel, "adoption.ack").checked = true
                top.find(panel, "adoption.adopt").clicked()
                break
            case 31:
                top.check(host.lastError.indexOf("PREVIEW_ONLY") === 0 && host.snapshot.life === null, "preview pet.adopt refused; still no companion")
                panel.view = "life"
                top.find(panel, "memorial.show").clicked()
                break
            case 33:
                top.check(top.find(panel, "life.memorial").entries.length === 3, "preview serves pet.memorial pages")
                // Every fixture renders in every view without a request.
                var before = host.seq
                var views = ["home", "care", "tasks", "companion", "life"]
                for (var i = 0; i < host.fixtureNames.length; i++) {
                    host.fixture = host.fixtureNames[i]
                    host.snapshot = host.buildSnapshot(host.fixtureNames[i])
                    for (var v = 0; v < views.length; v++) {
                        panel.view = views[v]
                        top.check(panel.inView, host.fixtureNames[i] + " renders in " + views[v])
                    }
                    var snap = host.snapshot
                    top.check(snap.lifecycle && typeof snap.lifecycle.frozen_reason === "string"
                              && ("life" in snap) && Array.isArray(snap.unseen_events) && snap.memorial,
                              host.fixtureNames[i] + " has the v0.2 snapshot blocks")
                }
                top.check(host.seq === before, "switching fixtures and views sends no request")
                top.check(host.fixtureNames.length === 20, "20 preview fixtures")
                var wanted = ["unarmed", "warning_thirst", "critical_hunger", "gentle_fainted", "death_pending_review",
                              "no_life_memorial", "evolved_unseen", "paused", "aquatic_goldfish", "robot_working"]
                top.check(wanted.every(function (n) { return host.fixtureNames.indexOf(n) >= 0 }), "all spec §12 life fixtures exist")
                host.snapshot = host.buildSnapshot("aquatic_goldfish")
                top.check(host.snapshot.life.appearance === "goldfish" && host.snapshot.life.stage === 3, "aquatic fixture is a mature goldfish")
                host.snapshot = host.buildSnapshot("robot_working")
                top.check(host.snapshot.life.appearance === "robot" && host.snapshot.life.species === "cat"
                          && host.snapshot.active_task.state === "running", "robot look on a cat life, working")
                console.log("DONE failures=" + top.failures)
                Qt.exit(top.failures === 0 ? 0 : 3)
                break
            }
        }
    }
}
