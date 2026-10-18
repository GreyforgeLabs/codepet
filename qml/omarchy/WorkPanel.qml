pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import "../shared"
import "WorkModel.js" as Work

Rectangle {
    id: panel
    required property var controller
    readonly property var snapshot: controller.state
    readonly property var task: snapshot ? snapshot.task : null
    readonly property bool busy: controller.busy
    property string selectedKind: "build"
    property alias project: projectInput.text
    property alias prompt: promptInput.text
    property bool verificationAcknowledged: false
    readonly property bool reviewing: Work.review(task)
    readonly property color ink: "#edf0e9"
    readonly property color muted: "#aeb9b1"
    readonly property color accent: "#a8dda0"
    readonly property string digest: task ? String(task.review_digest || "") : ""
    readonly property var pet: controller.pet || null
    property int petFrame: 0
    Timer { interval: 600; repeat: true; running: panel.visible && !!panel.pet && !panel.controller.calm; onTriggered: panel.petFrame++ }
    onDigestChanged: verificationAcknowledged = false
    color: "#171e1b"
    implicitWidth: 780
    implicitHeight: 760

    function openKind(value) { if (!panel.task) panel.selectedKind = Work.kind(value) }
    function start() {
        if (Work.canStart(panel.snapshot, panel.busy, panel.project, panel.prompt))
            controller.start(panel.selectedKind, panel.project.trim(), panel.prompt.trim())
    }
    function accept() {
        if (Work.canAccept(panel.task, panel.busy, panel.verificationAcknowledged))
            controller.action("accept", { task_id: panel.task.id, review_digest: panel.digest, acknowledge_unverified: true })
    }

    component Words: Label {
        color: panel.ink
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        font.pixelSize: 15
    }
    component Action: Button {
        id: action
        font.pixelSize: 15
        leftPadding: 16
        rightPadding: 16
        topPadding: 10
        bottomPadding: 10
        implicitHeight: 44
        palette.button: highlighted ? panel.accent : "#303d35"
        palette.buttonText: highlighted ? "#142219" : panel.ink
        palette.highlight: panel.accent
        palette.highlightedText: "#142219"
        background: Rectangle {
            radius: 8
            color: action.highlighted ? panel.accent : action.down ? "#43574a" : action.hovered ? "#364a3d" : "#303d35"
            opacity: action.enabled ? 1 : 0.45
            border.color: action.activeFocus ? panel.accent : "#506155"
            border.width: action.activeFocus ? 2 : 1
        }
        contentItem: Text {
            text: action.text
            textFormat: Text.PlainText
            font: action.font
            color: action.highlighted ? "#142219" : panel.ink
            opacity: action.enabled ? 1 : 0.5
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
    }
    component Editor: TextArea {
        color: panel.ink
        selectionColor: "#5d855a"
        selectedTextColor: panel.ink
        placeholderTextColor: panel.muted
        font.pixelSize: 15
        padding: 12
        background: Rectangle { color: "#202b24"; radius: 8; border.color: "#506155" }
    }
    FolderDialog {
        id: folder
        title: "Choose a project for CodePet"
        onAccepted: panel.project = Work.filePath(selectedFolder)
    }
    ScrollView {
        objectName: "work.scroll"
        anchors.fill: parent
        anchors.margins: 26
        contentWidth: availableWidth
        clip: true
        ColumnLayout {
            width: parent.width
            spacing: 16
            RowLayout {
                Layout.fillWidth: true
                PetSprite {
                    Layout.preferredWidth: 70
                    Layout.preferredHeight: 70
                    visible: !!panel.pet
                    petId: panel.pet ? panel.pet.appearance : ""
                    stage: panel.pet ? panel.pet.stage : 1
                    semanticState: panel.snapshot && panel.snapshot.readiness && panel.snapshot.readiness.sleeping ? "sleep" : panel.task && panel.task.status === "running" ? "working" : panel.reviewing ? "waiting" : "idle"
                    frame: panel.petFrame
                    reducedMotion: panel.controller.calm === true
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 4
                    Words { text: panel.pet ? panel.pet.name + " / CODEPET" : "CODEPET / WORK"; color: panel.accent; font.pixelSize: 12; font.letterSpacing: 2 }
                    Words { objectName: "work.heading"; text: Work.stateTitle(panel.task); font.pixelSize: 28; font.bold: true; Layout.fillWidth: true }
                }
                Action { text: "Refresh"; enabled: !panel.busy; onClicked: panel.controller.refresh() }
            }
            RowLayout {
                visible: Work.finished(panel.task)
                Layout.fillWidth: true
                Words { objectName: "work.previousTime"; Layout.fillWidth: true; color: panel.muted; font.pixelSize: 12; text: Work.startedDate(panel.task) }
                    Action {
                        objectName: "work.new"
                        visible: Work.finished(panel.task)
                        enabled: !panel.busy
                        text: "Dismiss result"
                        highlighted: false
                        onClicked: { panel.verificationAcknowledged = false; panel.controller.newJob() }
                    }
            }
            ClientSelector { Layout.fillWidth: true; controller: panel.controller }
            Rectangle {
                Layout.fillWidth: true
                implicitHeight: readinessColumn.implicitHeight + 28
                radius: 12
                color: "#24312a"
                ColumnLayout {
                    id: readinessColumn
                    anchors.fill: parent
                    anchors.margins: 14
                    spacing: 8
                    RowLayout {
                        Layout.fillWidth: true
                        Words {
                            Layout.fillWidth: true
                            text: panel.snapshot && panel.snapshot.readiness ? panel.snapshot.readiness.reason : "Checking on your pet…"
                        }
                        Words { objectName: "work.energy"; color: panel.accent; text: Work.energy(panel.snapshot) === null ? "" : "Energy " + Work.energy(panel.snapshot) + "%" }
                    }
                    ProgressBar {
                        id: energyBar
                        Layout.fillWidth: true
                        implicitHeight: 8
                        value: (Work.energy(panel.snapshot) || 0) / 100
                        background: Rectangle { radius: 4; color: "#435048" }
                        contentItem: Item { Rectangle { width: energyBar.visualPosition * parent.width; height: parent.height; radius: 4; color: panel.accent } }
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        Action { text: "Play"; enabled: !panel.busy; onClicked: panel.controller.care("play") }
                        Action { text: "Nap"; enabled: !panel.busy && !(panel.snapshot && panel.snapshot.readiness && panel.snapshot.readiness.sleeping); onClicked: panel.controller.care("sleep") }
                        Item { Layout.fillWidth: true }
                        Action { text: "Small snack"; enabled: !panel.busy; onClicked: panel.controller.care("food") }
                        Action { text: "Drink"; enabled: !panel.busy; onClicked: panel.controller.care("water") }
                    }
                    Words { color: panel.muted; font.pixelSize: 12; text: "Care is free. Accepted work earns care and XP." }
                }
            }
            Words {
                objectName: "work.error"
                Layout.fillWidth: true
                visible: text !== "" && !panel.reviewing
                color: "#ffc2a8"
                text: panel.controller.error
                Accessible.role: Accessible.AlertMessage
            }
            ColumnLayout {
                visible: !panel.task
                Layout.fillWidth: true
                spacing: 14
                RowLayout {
                    Layout.fillWidth: true
                    Repeater {
                        model: Work.KINDS
                        Action {
                            required property string modelData
                            objectName: "work.kind." + modelData
                            Layout.fillWidth: true
                            text: Work.title(modelData)
                            highlighted: panel.selectedKind === modelData
                            onClicked: panel.selectedKind = modelData
                        }
                    }
                }
                Words { text: Work.reward(panel.selectedKind); color: panel.accent; Layout.fillWidth: true }
                Words { text: "Project folder"; font.bold: true }
                RowLayout {
                    Layout.fillWidth: true
                    TextField {
                        id: projectInput
                        objectName: "work.project"
                        Layout.fillWidth: true
                        placeholderText: "/home/you/Projects/my-project"
                        selectByMouse: true
                        font.pixelSize: 15
                        color: panel.ink
                        placeholderTextColor: panel.muted
                        padding: 12
                        background: Rectangle { radius: 8; color: "#202b24"; border.color: projectInput.activeFocus ? panel.accent : "#506155" }
                        Accessible.name: "Project folder"
                    }
                    Action { text: "Choose…"; onClicked: folder.open() }
                }
                Words { text: "What should your pet do?"; font.bold: true }
                ScrollView {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 116
                    Editor {
                        id: promptInput
                        objectName: "work.prompt"
                        textFormat: Text.PlainText
                        wrapMode: TextEdit.Wrap
                        selectByMouse: true
                        placeholderText: panel.selectedKind === "bug" ? "Describe the bug and how to reproduce it…" : panel.selectedKind === "research" ? "What would you like to find out?" : "Describe a small, useful change…"
                        Accessible.name: "Task description"
                    }
                }
                Words {
                    objectName: "work.scope"
                    Layout.fillWidth: true
                    color: panel.muted
                    font.pixelSize: 13
                    text: Work.clientName(panel.snapshot) + " works on a private copy of this folder with network access. Your project changes only after you review and apply the result."
                }
                Words {
                    Layout.fillWidth: true
                    visible: !!panel.snapshot && !!panel.snapshot.provider && !panel.snapshot.provider.available
                    color: "#ffc2a8"
                    text: panel.snapshot && panel.snapshot.provider ? String(panel.snapshot.provider.reason || "This client is not ready.") : ""
                }
                Action {
                    objectName: "work.start"
                    Layout.fillWidth: true
                    text: panel.busy ? "Checking…" : "Start this job"
                    highlighted: true
                    enabled: Work.canStart(panel.snapshot, panel.busy, panel.project, panel.prompt)
                    onClicked: panel.start()
                }
            }
            ColumnLayout {
                visible: !!panel.task
                Layout.fillWidth: true
                spacing: 12
                Words { Layout.fillWidth: true; text: panel.task ? Work.title(panel.task.kind) + " · " + panel.task.project : ""; color: panel.accent; font.bold: true }
                Words { Layout.fillWidth: true; text: panel.task ? panel.task.prompt || "" : "" }
                Words { objectName: "work.progress"; Layout.fillWidth: true; text: panel.task ? String(panel.task.progress || "") : ""; color: panel.muted }
                Words { objectName: "work.clientDetail"; Layout.fillWidth: true; visible: text !== ""; text: panel.task ? String(panel.task.client_detail || "") : ""; color: panel.muted; font.pixelSize: 12 }
                BusyIndicator { running: !!panel.task && ["preparing", "running"].indexOf(panel.task.status) >= 0; visible: running; Layout.alignment: Qt.AlignHCenter }
                Words { Layout.fillWidth: true; text: panel.task ? String(panel.task.error || "") : ""; color: "#ffc2a8"; visible: text !== "" }
                Words { text: panel.reviewing ? "Result and reported checks" : "Activity"; font.bold: true }
                ScrollView {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 200
                    Editor {
                        objectName: "work.output"
                        readOnly: true
                        selectByMouse: true
                        textFormat: Text.PlainText
                        wrapMode: TextEdit.WrapAnywhere
                        text: panel.task ? String(panel.task.output || "Waiting for the first update…") : ""
                    }
                }
                Words { visible: panel.reviewing; text: "Review every changed file"; font.bold: true }
                Repeater {
                    model: panel.reviewing && panel.task && panel.task.changes ? panel.task.changes : []
                    ColumnLayout {
                        id: changeItem
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: 6
                        Words { Layout.fillWidth: true; text: changeItem.modelData.path; color: panel.accent; font.bold: true }
                        RowLayout {
                            Layout.fillWidth: true
                            Repeater {
                                model: ["before", "after"]
                                ColumnLayout {
                                    id: versionColumn
                                    required property string modelData
                                    Layout.fillWidth: true
                                    Words { text: versionColumn.modelData === "before" ? "Before" : "After"; color: panel.muted }
                                    ScrollView {
                                        Layout.fillWidth: true
                                        Layout.preferredHeight: 180
                                        Editor {
                                            readOnly: true
                                            selectByMouse: true
                                            textFormat: Text.PlainText
                                            wrapMode: TextEdit.WrapAnywhere
                                            font.family: "monospace"
                                            font.pixelSize: 12
                                            text: changeItem.modelData[versionColumn.modelData] !== null ? String(changeItem.modelData[versionColumn.modelData]) : "(File does not exist)"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Words {
                    Layout.fillWidth: true
                    visible: panel.reviewing
                    color: panel.muted
                    text: "These checks were reported by the coding assistant. CodePet has not independently verified the result. Read the output and changes before accepting."
                }
                CheckBox {
                    objectName: "work.acknowledge"
                    visible: panel.reviewing
                    Layout.fillWidth: true
                    text: "I've reviewed this result."
                    checked: panel.verificationAcknowledged
                    onToggled: panel.verificationAcknowledged = checked
                    palette.windowText: panel.ink
                }
                Words {
                    objectName: "work.reviewError"
                    Layout.fillWidth: true
                    visible: panel.reviewing && text !== ""
                    color: "#ffc2a8"
                    text: panel.controller.error
                    Accessible.role: Accessible.AlertMessage
                }
                RowLayout {
                    Layout.fillWidth: true
                    Action {
                        objectName: "work.stop"
                        visible: !!panel.task && ["preparing", "running"].indexOf(panel.task.status) >= 0
                        enabled: !panel.busy
                        text: "Stop job"
                        onClicked: panel.controller.action("cancel", { task_id: panel.task.id })
                    }
                    Action {
                        objectName: "work.discard"
                        visible: panel.reviewing || Work.finding(panel.task)
                        enabled: !panel.busy && !!panel.task && panel.task.status !== "applying"
                        text: "Discard"
                        onClicked: panel.controller.action("discard", { task_id: panel.task.id })
                    }
                    Item { Layout.fillWidth: true }
                    Action {
                        objectName: "work.accept"
                        visible: panel.reviewing
                        enabled: Work.canAccept(panel.task, panel.busy, panel.verificationAcknowledged)
                        highlighted: true
                        text: panel.task && panel.task.changes && panel.task.changes.length ? "Apply changes and reward pet" : "Accept result and reward pet"
                        onClicked: panel.accept()
                    }

                }
                Words {
                    visible: !!panel.task && panel.task.status === "accepted"
                    color: panel.accent
                    text: panel.task ? "Accepted. Your pet earned " + panel.task.reward_xp + " XP and care." : ""
                }
            }
            Words { Layout.fillWidth: true; color: panel.muted; font.pixelSize: 12; text: "You can close this window while your pet works. Open Work to return to the same job." }
        }
    }
}
