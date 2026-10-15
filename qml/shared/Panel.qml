pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Shared CodePet panel. Renders the Rust-owned snapshot only; it never changes
// meters or rewards itself. Every action goes through `host.request` and is
// followed by an explicit `state.get` refresh.
//
// Host contract (see qml/README.md):
//   property var snapshot          last `state.get` result, set by the host
//   property bool busy             a request is awaiting its response
//   property string lastError      "CODE: message" of the last failed request, else ""
//   function request(method, params, callback[, requestId])
//                                  callback receives the parsed response envelope
// Optional: property bool supervisorActive (a started job's process is still alive).
ColumnLayout {
    id: root
    required property var host
    property string view: "home"
    property bool showPetCare: true
    property string notice: ""
    readonly property var snap: host && host.snapshot ? host.snapshot : null
    readonly property var activeTask: snap && snap.active_task ? snap.active_task : null
    readonly property var meters: snap && snap.meters ? snap.meters : null
    readonly property bool paused: !!(snap && snap.decay && snap.decay.paused)
    readonly property bool decayEnabled: !!(snap && snap.decay && snap.decay.enabled)
    readonly property bool hostBusy: !!(host && host.busy)
    readonly property var cancellable: ["preparing", "running", "waiting_for_user", "verifying"]
    readonly property var categoryNames: ({
        "hunger": "Bugfix",
        "thirst": "Quality & Performance",
        "mental_health": "Learning & Specification",
        "hygiene": "Cleanup"
    })
    spacing: 14

    function refresh() {
        root.host.request("state.get", {}, function () {})
    }
    // Run one action, then refresh from Rust whatever the outcome.
    function act(method, params, done) {
        root.notice = ""
        root.host.request(method, params, function (resp) {
            if (done)
                done(resp)
            root.refresh()
        })
    }
    function meterValue(key) {
        return root.meters && typeof root.meters[key] === "number" ? root.meters[key] : 0
    }
    function categoryName(key) {
        return root.categoryNames[key] || String(key)
    }
    function openNewTask() {
        taskForm.reset()
        root.view = "form"
    }
    function openReview(taskId) {
        root.view = "review"
        taskReview.load(taskId)
    }

    Label {
        Layout.fillWidth: true
        visible: root.snap === null
        text: root.hostBusy ? "Loading CodePet state…" : "No state loaded yet. Use Refresh."
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
    }

    Label {
        Layout.fillWidth: true
        visible: root.host && root.host.lastError !== ""
        text: "Error: " + (root.host ? root.host.lastError : "")
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        font.bold: true
        color: "#9b1c1c"
        Accessible.name: text
    }
    Label {
        Layout.fillWidth: true
        visible: root.notice !== ""
        text: root.notice
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
    }

    ColumnLayout {
        Layout.fillWidth: true
        visible: root.view === "home"
        spacing: 14

        Label {
            Layout.fillWidth: true
            text: "SIMULATION ONLY · No real provider, commands or project changes"
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            font.bold: true
        }
        PetCare {
            objectName: "panel.petCare"
            visible: root.showPetCare
            Layout.fillWidth: true
            host: root.host
        }
        GroupBox {
            title: "Care · all four bars must reach 100 before Start"
            Layout.fillWidth: true
            ColumnLayout {
                anchors.fill: parent
                spacing: 10
                Label {
                    Layout.fillWidth: true
                    text: "All four meters are well-being from 0 to 100: higher is better. Values come from CodePet's saved state."
                    textFormat: Text.PlainText
                    wrapMode: Text.WordWrap
                }
                WellbeingMeter { Layout.fillWidth: true; label: "Nourishment"; hundredths: root.meterValue("hunger") }
                WellbeingMeter { Layout.fillWidth: true; label: "Hydration"; hundredths: root.meterValue("thirst") }
                WellbeingMeter { Layout.fillWidth: true; label: "Mood"; hundredths: root.meterValue("mental_health") }
                WellbeingMeter { Layout.fillWidth: true; label: "Hygiene"; hundredths: root.meterValue("hygiene") }
                Label {
                    Layout.fillWidth: true
                    text: !root.decayEnabled ? "Decay: off (default)" : (root.paused ? "Decay: paused" : "Decay: gentle decay while open")
                    textFormat: Text.PlainText
                }
            }
        }

        GroupBox {
            title: "Task"
            Layout.fillWidth: true
            ColumnLayout {
                anchors.fill: parent
                spacing: 8
                Label {
                    visible: root.activeTask === null
                    text: "[–] No active task"
                    textFormat: Text.PlainText
                    font.bold: true
                }
                StatusBadge {
                    visible: root.activeTask !== null
                    taskState: root.activeTask ? root.activeTask.state : ""
                }
                Label {
                    Layout.fillWidth: true
                    visible: root.activeTask !== null
                    text: root.activeTask
                        ? "Category: " + root.categoryName(root.activeTask.category) + " · Provider: " + root.activeTask.provider
                            + (root.activeTask.provider === "mock" ? " (SIMULATED)" : "")
                        : ""
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }
                Label {
                    Layout.fillWidth: true
                    visible: root.activeTask !== null && root.cancellable.indexOf(root.activeTask.state) !== -1
                    text: root.host && root.host.supervisorActive === true
                        ? "The job process is still running. Refresh to see its latest saved state."
                        : "Nothing updates on its own. Refresh to see the latest saved state."
                    textFormat: Text.PlainText
                    wrapMode: Text.WordWrap
                }
                Flow {
                    Layout.fillWidth: true
                    spacing: 8
                    Button {
                        text: "New task"
                        enabled: !root.hostBusy && root.snap !== null && root.activeTask === null
                        onClicked: root.openNewTask()
                    }
                    Button {
                        text: "Review result"
                        visible: root.activeTask !== null && root.activeTask.state === "review_ready"
                        enabled: !root.hostBusy
                        onClicked: root.openReview(root.activeTask.task_id)
                    }
                    Button {
                        objectName: "panel.resolveRecovery"
                        text: "Resolve recovery"
                        visible: root.activeTask !== null && root.activeTask.state === "recovery_required"
                        enabled: !root.hostBusy
                        onClicked: root.openReview(root.activeTask.task_id)
                    }
                    Button {
                        text: "Cancel task"
                        visible: root.activeTask !== null && root.cancellable.indexOf(root.activeTask.state) !== -1
                        enabled: !root.hostBusy
                        onClicked: root.act("task.cancel", { "task_id": root.activeTask.task_id })
                    }
                    Button {
                        text: root.paused ? "Resume decay" : "Pause decay"
                        enabled: !root.hostBusy && root.snap !== null
                        onClicked: root.act("pet.pause", { "paused": !root.paused })
                    }
                    Button {
                        text: "Refresh"
                        enabled: !root.hostBusy
                        onClicked: root.refresh()
                    }
                    Button {
                        text: "Check interrupted tasks"
                        enabled: !root.hostBusy && root.snap !== null
                        onClicked: root.act("task.recover", {}, function (resp) {
                            if (resp.ok && resp.result && resp.result.changed)
                                root.notice = resp.result.changed.length === 0
                                    ? "No interrupted tasks found."
                                    : resp.result.changed.length + " task(s) marked after recovery. Nothing was resumed."
                        })
                    }
                }
            }
        }

        ProgressionPanel {
            Layout.fillWidth: true
            progression: root.snap ? root.snap.progression : null
        }
        TaskTimeline {
            objectName: "panel.timeline"
            Layout.fillWidth: true
            host: root.host
        }
        GroupBox {
            title: "Recent results"
            Layout.fillWidth: true
            ColumnLayout {
                anchors.fill: parent
                spacing: 6
                Label {
                    visible: !root.snap || !root.snap.recent || root.snap.recent.length === 0
                    text: "No tasks yet."
                    textFormat: Text.PlainText
                }
                Repeater {
                    model: root.snap && root.snap.recent ? root.snap.recent : []
                    delegate: ColumnLayout {
                        id: recentRow
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: 2
                        StatusBadge { taskState: recentRow.modelData.state }
                        Label {
                            Layout.fillWidth: true
                            text: root.categoryName(recentRow.modelData.category) + " · " + recentRow.modelData.provider
                                + " · " + String(recentRow.modelData.instructions_excerpt || "").split("\n")[0]
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                        }
                        Label {
                            Layout.fillWidth: true
                            visible: !!recentRow.modelData.receipt
                            text: recentRow.modelData.receipt
                                ? "Saved receipt: " + (recentRow.modelData.receipt.demo_xp || 0) + " demo XP · "
                                    + (recentRow.modelData.receipt.real_xp || 0) + " real-work XP"
                                    + (recentRow.modelData.receipt.amount_hundredths > 0 ? " · historical care reward: +" + (recentRow.modelData.receipt.amount_hundredths / 100).toFixed(1) : "")
                                    + " (verification: " + recentRow.modelData.receipt.verification + ")"
                                : ""
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                        }
                        Button {
                            text: "Continue draft"
                            visible: recentRow.modelData.state === "draft"
                            enabled: !root.hostBusy && root.activeTask === null
                            onClicked: {
                                var id = recentRow.modelData.task_id
                                taskForm.loadDraftById(id, function (ok) {
                                    if (ok)
                                        root.view = "form"
                                })
                            }
                        }
                    }
                }
            }
        }
    }

    TaskForm {
        id: taskForm
        objectName: "panel.taskForm"
        Layout.fillWidth: true
        visible: root.view === "form"
        host: root.host
        onClosed: {
            root.view = "home"
            root.refresh()
        }
    }

    TaskReview {
        id: taskReview
        objectName: "panel.taskReview"
        Layout.fillWidth: true
        visible: root.view === "review"
        host: root.host
        onClosed: {
            root.view = "home"
            root.refresh()
        }
    }
}
