import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window

Window {
    id: root
    width: 760
    height: 880
    minimumWidth: 560
    minimumHeight: 480
    visible: true
    title: "CodePet — UI mock"
    color: "#f5f5f2"

    property string phase: "Idle"
    property var meters: [70, 70, 70, 70]
    readonly property var meterNames: ["Hunger satisfied", "Hydration", "Mental health", "Cleanliness"]
    property var approvedTask: ({})
    property bool rewardGranted: false
    property string acceptedOverrideReason: ""
    readonly property bool drafting: phase === "Idle"
    readonly property bool active: ["Preparing", "Running", "Waiting for user", "Verifying"].indexOf(phase) !== -1
    readonly property bool terminal: ["Accepted", "Discarded", "Cancelled", "Failed"].indexOf(phase) !== -1

    function invalidateDraft() {
        if (scopeAcknowledged) scopeAcknowledged.checked = false
    }
    function startMock() {
        if (!drafting || !startButton.enabled) return
        approvedTask = {
            category: category.currentIndex,
            task: task.text.trim(),
            folder: folder.text.trim(),
            scope: scope.text.trim()
        }
        rewardGranted = false
        acceptedOverrideReason = ""
        overrideReason.text = ""
        reviewAcknowledged.checked = false
        phase = "Preparing"
    }
    function acceptMock() {
        if (phase !== "Review ready" || !reviewAcknowledged.checked
                || overrideReason.text.trim().length === 0 || rewardGranted) return
        acceptedOverrideReason = overrideReason.text.trim()
        var next = meters.slice()
        next[approvedTask.category] = Math.min(100, next[approvedTask.category] + 20)
        meters = next
        rewardGranted = true
        phase = "Accepted"
    }

    ScrollView {
        anchors.fill: parent
        anchors.margins: 20
        contentWidth: availableWidth
        clip: true
        ColumnLayout {
            width: parent.width
            spacing: 16
            Label { text: "CodePet"; font.pixelSize: 28; font.bold: true }
            Label {
                Layout.fillWidth: true
                text: "UI mock · Rust disconnected · All changes are in memory"
                wrapMode: Text.WordWrap
                font.bold: true
            }
            Label {
                Layout.fillWidth: true
                text: "No providers, commands, network requests, or project files are accessed. Folder and scope are display-only text. Closing resets everything."
                wrapMode: Text.WordWrap
            }
            GroupBox {
                title: "Well-being · higher is better · decay disabled"
                Layout.fillWidth: true
                ColumnLayout {
                    anchors.fill: parent
                    spacing: 12
                    Repeater {
                        model: 4
                        WellbeingMeter {
                            required property int index
                            Layout.fillWidth: true
                            label: root.meterNames[index]
                            value: root.meters[index]
                        }
                    }
                }
            }
            Label { text: "Status: " + root.phase; font.bold: true; font.pixelSize: 18 }
            GroupBox {
                title: "1. Describe a bounded task"
                Layout.fillWidth: true
                visible: root.drafting
                ColumnLayout {
                    anchors.fill: parent
                    Label { text: "Care category" }
                    ComboBox {
                        id: category
                        Layout.fillWidth: true
                        model: ["Hunger · fix a specific bug", "Hydration · improve speed or quality", "Mental health · learn or write a plan", "Cleanliness · selected obsolete project files"]
                        Accessible.name: "Care category"
                        onCurrentIndexChanged: root.invalidateDraft()
                    }
                    Label { text: "Task outcome" }
                    TextField {
                        id: task
                        Layout.fillWidth: true
                        placeholderText: "Describe one specific outcome"
                        maximumLength: 4096
                        Accessible.name: "Task outcome"
                        onTextChanged: root.invalidateDraft()
                    }
                    Label { text: "Folder (display-only; not validated)" }
                    TextField {
                        id: folder
                        Layout.fillWidth: true
                        placeholderText: "/example/disposable-project"
                        maximumLength: 4096
                        Accessible.name: "Folder, display only"
                        onTextChanged: root.invalidateDraft()
                    }
                    Label { text: "Relative writable scope (display-only)" }
                    TextField {
                        id: scope
                        Layout.fillWidth: true
                        placeholderText: "src/example.rs"
                        maximumLength: 4096
                        Accessible.name: "Relative writable scope, display only"
                        onTextChanged: root.invalidateDraft()
                    }
                    Label { text: "Provider: mock only (no executable)"; font.bold: true }
                    Label {
                        Layout.fillWidth: true
                        text: "2. Review before Start\nTask: " + task.text + "\nFolder: " + folder.text + "\nScope: " + scope.text + "\nNetwork: none · File operations: none\nLimits: one mock task; no process or resource limits are enforced because nothing runs. Verification: no checks run."
                        textFormat: Text.PlainText
                        wrapMode: Text.WrapAnywhere
                    }
                    CheckBox {
                        id: scopeAcknowledged
                        text: "I reviewed this display-only task"
                        Accessible.name: text
                    }
                    Button {
                        id: startButton
                        text: "Start mock task"
                        enabled: root.drafting && task.text.trim().length > 0
                            && folder.text.trim().length > 0 && scope.text.trim().length > 0
                            && scopeAcknowledged.checked
                        onClicked: root.startMock()
                    }
                }
            }
            GroupBox {
                title: "Mock task progress"
                Layout.fillWidth: true
                visible: root.active || root.phase === "Cancelling"
                ColumnLayout {
                    anchors.fill: parent
                    Label {
                        Layout.fillWidth: true
                        text: "Nothing advances automatically. These buttons only change the displayed state."
                        wrapMode: Text.WordWrap
                    }
                    Button {
                        text: "Advance: running"
                        visible: root.phase === "Preparing"
                        onClicked: root.phase = "Running"
                    }
                    Button {
                        text: "Simulate waiting for user"
                        visible: root.phase === "Running"
                        onClicked: root.phase = "Waiting for user"
                    }
                    Button {
                        text: "Continue mock task"
                        visible: root.phase === "Waiting for user"
                        onClicked: root.phase = "Running"
                    }
                    Button {
                        text: "Advance: verifying"
                        visible: root.phase === "Running"
                        onClicked: root.phase = "Verifying"
                    }
                    Button {
                        text: "Advance: review ready (not verified)"
                        visible: root.phase === "Verifying"
                        onClicked: root.phase = "Review ready"
                    }
                    RowLayout {
                        Button {
                            text: "Cancel mock task"
                            visible: root.active
                            onClicked: root.phase = "Cancelling"
                        }
                        Button {
                            text: "Simulate failure"
                            visible: root.active
                            onClicked: root.phase = "Failed"
                        }
                    }
                    Button {
                        text: "Advance: cancelled"
                        visible: root.phase === "Cancelling"
                        onClicked: root.phase = "Cancelled"
                    }
                }
            }
            GroupBox {
                title: "3. Review mock result"
                Layout.fillWidth: true
                visible: root.phase === "Review ready"
                ColumnLayout {
                    anchors.fill: parent
                    Label {
                        Layout.fillWidth: true
                        text: "Objective: " + (root.approvedTask.task || "")
                            + "\nProvider: mock only\nFiles added/modified/deleted: none\nVerification: NOT VERIFIED; no commands ran\nMock artifact: this placeholder is the entire simulated result.\nNo real result exists to apply. Accept demonstrates a +20 category reward, capped at 100."
                        textFormat: Text.PlainText
                        wrapMode: Text.WrapAnywhere
                    }
                    CheckBox {
                        id: reviewAcknowledged
                        text: "I reviewed this mock; no verification ran"
                        Accessible.name: text
                    }
                    Label { text: "Why accept this unverified mock? (required)" }
                    TextField {
                        id: overrideReason
                        Layout.fillWidth: true
                        maximumLength: 512
                        placeholderText: "Give a short reason for this mock acceptance"
                        Accessible.name: "Reason for accepting an unverified mock result"
                    }
                    RowLayout {
                        Button {
                            text: "Accept mock (+20)"
                            enabled: root.phase === "Review ready" && reviewAcknowledged.checked
                                && overrideReason.text.trim().length > 0 && !root.rewardGranted
                            onClicked: root.acceptMock()
                        }
                        Button {
                            text: "Discard (no reward)"
                            onClicked: root.phase = "Discarded"
                        }
                    }
                }
            }
            Label {
                Layout.fillWidth: true
                visible: root.terminal
                text: root.phase === "Accepted"
                    ? "Mock accepted once. The selected meter increased by up to 20; no files changed."
                    : "Mock " + root.phase.toLowerCase() + ". No reward granted; no files changed."
                wrapMode: Text.WordWrap
            }
            Button {
                visible: root.terminal
                text: "New mock task"
                onClicked: {
                    scopeAcknowledged.checked = false
                    reviewAcknowledged.checked = false
                    overrideReason.text = ""
                    root.acceptedOverrideReason = ""
                    root.phase = "Idle"
                }
            }
            Label {
                Layout.fillWidth: true
                text: "Prototype only. No Omarchy/Quickshell integration, Rust IPC, persistence, sandbox, staging, apply, or provider adapter is implemented here."
                wrapMode: Text.WordWrap
            }
        }
    }
}
