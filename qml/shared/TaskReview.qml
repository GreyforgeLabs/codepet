import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Review of one review_ready task, loaded from Rust with task.review. Shows the
// provider-authored artifact as plain text and the verification result plainly.
// Accept/Discard are explicit clicks; the reward shown afterwards is the receipt
// returned by Rust, never computed here. A recovery_required task cannot be accepted;
// it can only be abandoned (task.discard, no reward) after an acknowledgement.
ColumnLayout {
    id: root
    required property var host
    signal closed

    property var task: null
    property var receipt: null
    property bool duplicate: false
    property string outcome: ""
    // Idempotency key for acceptance: reused only for an identical retry.
    property string acceptKey: ""
    property string acceptKeyParams: ""
    property int keyCounter: 0

    readonly property bool hostBusy: !!(host && host.busy)
    readonly property var review: task && task.review ? task.review : null
    readonly property string verification: review ? String(review.verification) : ""
    readonly property bool needsOverride: verification !== "passed"
    readonly property bool canDecide: task !== null && review !== null && task.state === "review_ready" && receipt === null
    readonly property bool canAbandon: task !== null && review !== null && task.state === "recovery_required"
    readonly property int maxReasonBytes: 512
    readonly property int reasonBytes: utf8Bytes(reasonField.text.trim())
    readonly property bool reasonOk: reasonBytes <= maxReasonBytes
        && (!needsOverride || reasonField.text.trim().length > 0)
    readonly property var categoryNames: ({
        "hunger": "Bugfix",
        "thirst": "Quality & Performance",
        "mental_health": "Learning & Specification",
        "hygiene": "Cleanup"
    })
    spacing: 10

    function categoryName(key) {
        return root.categoryNames[key] || String(key)
    }
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
    function verificationText() {
        if (root.verification === "passed")
            return "Verification: passed"
        if (root.verification === "failed")
            return "Verification: FAILED. Checks reported a failure" + (root.review && root.review.simulated ? " (SIMULATED)." : ".")
        if (root.verification === "not_verified")
            return "Verification: NOT VERIFIED. No checks ran."
        return "Verification: unknown (" + root.verification + "). Treat as NOT VERIFIED."
    }
    function load(taskId) {
        root.task = null
        root.receipt = null
        root.duplicate = false
        root.outcome = ""
        root.acceptKey = ""
        root.acceptKeyParams = ""
        ackBox.checked = false
        abandonAck.checked = false
        reasonField.text = ""
        root.host.request("task.review", { "task_id": taskId }, function (resp) {
            if (resp.ok)
                root.task = resp.result
            root.host.request("state.get", {}, function () {})
        })
    }
    function acceptClicked() {
        if (!root.canDecide || !ackBox.checked || root.hostBusy)
            return
        var reason = reasonField.text.trim()
        if (!root.reasonOk)
            return
        var params = { "task_id": root.task.task_id, "review_digest": root.review.digest, "acknowledged": true }
        if (reason.length > 0)
            params.override_reason = reason
        var key = JSON.stringify(params)
        if (key !== root.acceptKeyParams || root.acceptKey === "") {
            root.keyCounter += 1
            root.acceptKey = "accept-" + Date.now().toString(36) + "-" + root.keyCounter
            root.acceptKeyParams = key
        }
        root.host.request("task.accept_artifact", params, function (resp) {
            if (resp.ok && resp.result) {
                root.receipt = resp.result.receipt
                root.duplicate = resp.result.duplicate === true
            }
            root.host.request("state.get", {}, function () {})
        }, root.acceptKey)
    }
    function discardClicked() {
        if (!root.canDecide || root.hostBusy)
            return
        var params = { "task_id": root.task.task_id, "review_digest": root.review.digest }
        root.host.request("task.discard", params, function (resp) {
            if (resp.ok) {
                root.task = resp.result
                root.outcome = "Discarded. No reward was granted and no project files were changed."
            }
            root.host.request("state.get", {}, function () {})
        })
    }

    // Explicit no-reward abandonment of a recovery_required task, after acknowledgement.
    function abandonClicked() {
        if (!root.canAbandon || !abandonAck.checked || root.hostBusy)
            return
        var params = { "task_id": root.task.task_id, "review_digest": root.review.digest }
        abandonAck.checked = false
        root.host.request("task.discard", params, function (resp) {
            if (resp.ok) {
                root.task = resp.result
                root.outcome = "Abandoned. No reward was granted. CodePet did not restore, change or delete any files."
            }
            root.host.request("state.get", {}, function () {})
        })
    }

    Label { text: "Review result"; font.bold: true; font.pixelSize: 18 }

    Label {
        visible: root.task === null
        text: root.hostBusy ? "Loading review…" : "No review loaded."
        textFormat: Text.PlainText
    }

    ColumnLayout {
        Layout.fillWidth: true
        visible: root.task !== null
        spacing: 8

        StatusBadge { taskState: root.task ? root.task.state : "" }
        Label {
            Layout.fillWidth: true
            text: root.task
                ? "Category: " + root.categoryName(root.task.category) + " · Provider: " + root.task.provider
                    + "\nObjective: " + root.task.instructions
                : ""
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }
        Label {
            Layout.fillWidth: true
            text: root.review && root.review.simulated
                ? "PROVIDER-AUTHORED · SIMULATED: produced by the mock provider. No command ran and no project files were read or written."
                : "PROVIDER-AUTHORED: written by the provider, not independent proof."
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            font.bold: true
        }
        ScrollView {
            Layout.fillWidth: true
            Layout.preferredHeight: 160
            TextArea {
                readOnly: true
                textFormat: TextEdit.PlainText
                wrapMode: TextEdit.Wrap
                text: root.review ? String(root.review.artifact) : ""
                Accessible.name: "Provider-authored artifact"
            }
        }
        Label {
            Layout.fillWidth: true
            text: root.verificationText()
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            font.bold: true
            Accessible.name: text
        }
        Label {
            Layout.fillWidth: true
            text: root.review ? "Review digest: " + root.review.digest : ""
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }

        ColumnLayout {
            Layout.fillWidth: true
            visible: root.canDecide
            CheckBox {
                id: ackBox
                objectName: "taskReview.ack"
                text: root.needsOverride
                    ? "I reviewed this artifact and understand it is " + (root.verification === "failed" ? "FAILED" : "NOT VERIFIED")
                    : "I reviewed this artifact"
                Accessible.name: text
            }
            Label {
                visible: root.needsOverride
                text: "Why accept it anyway? (required)"
            }
            TextField {
                id: reasonField
                objectName: "taskReview.reason"
                Layout.fillWidth: true
                visible: root.needsOverride
                placeholderText: "Short reason for accepting without passing checks"
                Accessible.name: "Override reason"
            }
            Label {
                visible: root.needsOverride
                text: root.reasonBytes + " / " + root.maxReasonBytes + " bytes"
                    + (root.reasonBytes > root.maxReasonBytes ? " — too long" : "")
                color: root.reasonBytes > root.maxReasonBytes ? "#9b1c1c" : palette.windowText
            }
            Label {
                Layout.fillWidth: true
                text: "Acceptance never refills care. Eligible simulated acceptance records demo XP once; failed checks grant no XP. Real-work XP remains separate and requires supported real evidence. Discarding grants nothing."
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }
            RowLayout {
                Button {
                    objectName: "taskReview.accept"
                    text: "Accept artifact"
                    enabled: root.canDecide && ackBox.checked && !root.hostBusy && root.reasonOk
                    onClicked: root.acceptClicked()
                }
                Button {
                    objectName: "taskReview.discard"
                    text: "Discard (no reward)"
                    enabled: root.canDecide && !root.hostBusy
                    onClicked: root.discardClicked()
                }
            }
        }
    }

    ColumnLayout {
        Layout.fillWidth: true
        visible: root.canAbandon
        Label {
            Layout.fillWidth: true
            text: "Recovery required: this saved review no longer matches what was sealed, so it cannot be accepted. "
                + "Abandoning ends the task with no reward and frees the task slot. It does not restore, change or delete any files."
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            font.bold: true
        }
        CheckBox {
            id: abandonAck
            objectName: "taskReview.abandonAck"
            text: "I understand nothing is rewarded or restored"
            Accessible.name: text
        }
        Button {
            objectName: "taskReview.abandon"
            text: "Abandon (no reward)"
            enabled: root.canAbandon && abandonAck.checked && !root.hostBusy
            onClicked: root.abandonClicked()
        }
    }

    Label {
        Layout.fillWidth: true
        visible: root.receipt !== null
        text: root.receipt
            ? (root.duplicate ? "Already accepted earlier. " : "Accepted. ")
                + "Saved receipt: +" + (root.receipt.demo_xp || 0) + " demo XP · +" + (root.receipt.real_xp || 0) + " real-work XP"
                + " · track: " + (root.receipt.progression_track || "none")
                + (root.receipt.amount_hundredths > 0 ? " · historical care reward: +" + (root.receipt.amount_hundredths / 100).toFixed(1) : "") + " · verification: " + root.receipt.verification
                + (root.receipt.override_reason ? " · override reason: " + root.receipt.override_reason : "")
            : ""
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        font.bold: true
    }
    Label {
        Layout.fillWidth: true
        visible: root.outcome !== ""
        text: root.outcome
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
    }

    Button {
        text: "Back"
        enabled: !root.hostBusy
        onClicked: root.closed()
    }
}
