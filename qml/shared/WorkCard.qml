pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Window
import "../omarchy/WorkModel.js" as Work
import "PetAge.js" as PetAge

Rectangle {
    id: card
    required property var controller
    property var meters: null
    property bool closable: true
    property bool expanded: false
    property real now: Date.now()
    readonly property var life: controller ? controller.pet : null
    Timer { interval: 60000; repeat: true; running: card.visible; onTriggered: card.now = Date.now() }
    onVisibleChanged: if (visible) now = Date.now()
    readonly property var snapshot: controller ? controller.state : null
    readonly property var task: snapshot ? snapshot.task : null
    readonly property bool finding: Work.finding(task)
    readonly property bool working: !!task && ["preparing", "running"].indexOf(task.status) >= 0
    readonly property bool review: Work.review(task)
    readonly property bool previous: Work.finished(task)
    readonly property bool preparationNotice: !task && !!(snapshot && snapshot.scout && snapshot.scout.notice)
    readonly property bool scouting: !!task && task.source_kind === "scout"
    readonly property bool checksOn: !!(snapshot && snapshot.scout && snapshot.scout.enabled)
    readonly property string taskKey: task ? String(task.id) + ":" + task.status + ":" + String(task.review_digest || "") : ""
    readonly property string heading: finding ? "Suspected bug · " + Work.repoName(task)
        : working ? (scouting ? "Looking for bugs…" : "Working on " + Work.repoName(task) + "…")
        : review ? (task.status === "applying" ? "Finish applying · " : task.kind === "bug" ? "Fix ready · " : "Work ready · ") + Work.repoName(task)
        : !snapshot ? "Checking status…"
        : snapshot.readiness && !snapshot.readiness.ready ? String(snapshot.readiness.reason || "Time for care")
        : snapshot.provider && !snapshot.provider.available ? Work.clientName(snapshot) + " unavailable"
        : checksOn ? "Ready to help" : "Ready · checks paused"
    readonly property string message: finding ? (task.finding ? String(task.finding.title) : "Open again to read this finding.")
        : working ? String(task.progress || "Taking a closer look.")
        : review ? (task.status === "applying" ? "Review this job to finish its interrupted application." : "Review the changes before applying them.")
        : snapshot && snapshot.readiness && !snapshot.readiness.ready ? String(snapshot.readiness.reason || "Your pet needs care before working.")
        : snapshot && snapshot.provider && !snapshot.provider.available ? String(snapshot.provider.reason || "Set up the selected client to check your repositories.")
        : checksOn ? "Ready to help. Checks local repositories when eligible."
        : "Automatic bug checks are paused."
    signal closeRequested()
    signal homeRequested()
    signal reviewRequested()
    signal updatesRequested()
    implicitWidth: 280
    implicitHeight: body.implicitHeight + 20
    radius: 16
    color: "#f51f242c"
    border.color: "#49505e"
    border.width: 1
    clip: true
    onTaskKeyChanged: { expanded = false; now = Date.now() }
    Keys.onEscapePressed: event => {
        if (closable) closeRequested()
        else event.accepted = false
    }
    function focusInitial() {
        if (!visible) return
        if (close.visible) close.forceActiveFocus(Qt.OtherFocusReason)
        else clientSelector.focusInitial()
    }

    function revealFocus() {
        var item = card.Window.window ? card.Window.window.activeFocusItem : null
        for (var parent = item; parent; parent = parent.parent) {
            if (parent !== body) continue
            var point = item.mapToItem(body, 0, 0)
            if (point.y < scroll.contentY) scroll.contentY = point.y
            else if (point.y + item.height > scroll.contentY + scroll.height)
                scroll.contentY = point.y + item.height - scroll.height
            scroll.contentY = Math.max(0, Math.min(scroll.contentY, scroll.contentHeight - scroll.height))
            return
        }
    }
    Connections {
        target: card.Window.window
        function onActiveFocusItemChanged() { card.revealFocus() }
    }

    // The card itself is an input island. Blank space must not drag the pet.
    MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons }
    Flickable {
        id: scroll
        objectName: "workCard.scroll"
        anchors.fill: parent
        anchors.margins: 10
        contentWidth: width
        contentHeight: body.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        Column {
            id: body
            width: parent.width
            spacing: 6
            Row {
                width: parent.width
                spacing: 8
                Text {
                    objectName: "workCard.heading"
                    width: parent.width - (close.visible ? close.width + 8 : 0)
                    text: card.heading; textFormat: Text.PlainText
                    wrapMode: Text.Wrap; maximumLineCount: card.expanded ? 2147483647 : 2; elide: Text.ElideRight; font.pixelSize: 12; font.bold: true; color: "#b9c5e9"
                }
                CardButton { id: close; objectName: "workCard.close"; visible: card.closable; text: "×"; label: "Close card"; width: 24; height: 24; onTapped: card.closeRequested() }
            }
            Text {
                objectName: "workCard.stats"
                visible: !!card.life
                width: parent.width
                text: (card.life && typeof card.life.level === "number" && typeof card.life.xp === "number"
                    ? "Lv " + card.life.level + " · " + card.life.xp + " XP · " : "") + PetAge.shortText(card.life, card.now) + (Work.energy(card.snapshot) === null ? "" : " · Energy " + Work.energy(card.snapshot) + "%")
                textFormat: Text.PlainText; wrapMode: Text.Wrap
                font.pixelSize: 11; color: "#b8bfcc"
            }
            Text {
                objectName: "workCard.needs"
                width: parent.width
                text: Work.meterText(card.meters)
                textFormat: Text.PlainText; wrapMode: Text.Wrap
                font.pixelSize: 10; color: "#b8bfcc"
            }
            Text {
                objectName: "workCard.finding"
                visible: card.finding
                width: parent.width; text: card.finding ? card.message : ""
                textFormat: Text.PlainText; wrapMode: Text.Wrap
                maximumLineCount: 1; elide: Text.ElideRight
                font.pixelSize: 12; color: "#edf0f5"
            }
            ClientSelector { id: clientSelector; width: parent.width; controller: card.controller }
            Flow {
                width: parent.width; spacing: 8
                CardButton {
                    objectName: "workCard.primary"
                    visible: card.finding || card.working || card.review
                    text: card.finding ? "Fix" : card.working ? "Stop" : "Review"
                    primary: !card.working
                    enabled: !!card.controller && !card.controller.busy && (!card.finding || !!(card.task.finding && card.task.review_digest && card.snapshot.provider && card.snapshot.provider.available))
                    onTapped: {
                        if (card.finding) card.controller.fix()
                        else if (card.working) card.controller.action("cancel", { task_id: card.task.id })
                        else card.reviewRequested()
                    }
                }
                CardButton { objectName: "workCard.care"; text: "Care"; height: 28; onTapped: card.homeRequested() }
                CardButton { objectName: "workCard.details"; text: card.expanded ? "Less" : "Details"; height: 28; onTapped: card.expanded = !card.expanded }
                CardButton { objectName: "workCard.updates"; text: "Updates"; height: 28; onTapped: card.updatesRequested() }

            }
            Text {
                objectName: "workCard.error"
                visible: !!(card.controller && card.controller.error)
                width: parent.width
                text: card.controller && card.controller.error ? "Last action: " + card.controller.error : ""
                textFormat: Text.PlainText; wrapMode: Text.Wrap
                maximumLineCount: card.expanded ? 2147483647 : 1; elide: Text.ElideRight
                font.pixelSize: 11; color: "#ffbea9"
            }
            Row {
                visible: card.previous || card.preparationNotice
                width: parent.width; spacing: 6
                Text {
                    objectName: "workCard.previousTitle"
                    width: parent.width - dismissButton.width - 6
                    anchors.verticalCenter: parent.verticalCenter
                    text: Work.previousTitle(card.task)
                    textFormat: Text.PlainText; elide: Text.ElideRight
                    color: "#b8bfcc"; font.pixelSize: 11
                }
                CardButton {
                    id: dismissButton
                    objectName: "workCard.dismiss"
                    text: "Dismiss"; label: "Dismiss previous result"; height: 24
                    enabled: !!card.controller && !card.controller.busy
                    onTapped: card.controller.dismiss()
                }
            }
            Column {
                visible: card.expanded
                width: parent.width; spacing: 8
                Text {
                    objectName: "workCard.message"
                    width: parent.width; text: card.message
                    textFormat: Text.PlainText; wrapMode: Text.Wrap
                    font.pixelSize: 12; color: "#edf0f5"
                }
                Text {
                    objectName: "workCard.previousTime"
                    visible: card.previous
                    width: parent.width; text: Work.startedText(card.task, card.now)
                    textFormat: Text.PlainText; wrapMode: Text.Wrap
                    font.pixelSize: 11; color: "#b8bfcc"
                }
                Text {
                    objectName: "workCard.previousMessage"
                    visible: card.previous || card.preparationNotice
                    width: parent.width
                    text: card.previous ? Work.previousMessage(card.task)
                        : card.preparationNotice ? String(card.snapshot.scout.last_error || card.snapshot.scout.notice) : ""
                    textFormat: Text.PlainText; wrapMode: Text.WrapAnywhere
                    font.pixelSize: 12; color: "#c5cbd7"
                }
                Flow {
                    width: parent.width; spacing: 6
                    CardButton {
                        visible: card.finding
                        text: "Dismiss finding"
                        enabled: !!card.controller && !card.controller.busy
                        onTapped: card.controller.action("discard", { task_id: card.task.id })
                    }
                CardButton {
                    objectName: "workCard.checks"
                    visible: card.expanded
                    text: card.checksOn ? "Checks on" : "Checks off"; height: 28
                    enabled: !!card.snapshot && !!card.snapshot.scout && !card.controller.busy
                    onTapped: card.controller.action("scout-settings", { enabled: !card.checksOn })
                }
                }
            }
            Text {
                visible: card.expanded
                width: parent.width
                text: card.task && card.task.finding
                    ? String(card.task.finding.path || "") + (card.task.finding.line ? ":" + card.task.finding.line : "")
                      + "\n\n" + String(card.task.finding.evidence || "") + "\n\n" + String(card.task.finding.check || "")
                      + "\n\nThis is a suspected bug reported by the provider. Fix prepares a candidate for your review."
                    : String(card.task && card.task.output || "Checks use your selected client to inspect local repositories. A check does not edit your original files. Fixes need your review before applying.")
                textFormat: Text.PlainText; wrapMode: Text.WrapAnywhere
                font.pixelSize: 12; color: "#b8bfcc"
            }
        }
    }
    component CardButton: Rectangle {
        id: button
        property string text: ""
        property string label: text
        property bool primary: false
        signal tapped()
        implicitWidth: caption.implicitWidth + 18
        implicitHeight: 28
        radius: 8
        opacity: enabled ? 1 : 0.45
        color: primary ? (mouse.pressed ? "#98a6da" : "#bbc8ff") : (mouse.containsMouse ? "#3b4352" : "#2c333f")
        border.width: activeFocus ? 2 : 0; border.color: "#f2dca0"
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: label
        Accessible.onPressAction: if (enabled) tapped()
        Keys.onReturnPressed: if (enabled) tapped()
        Keys.onSpacePressed: if (enabled) tapped()
        Text { id: caption; anchors.centerIn: parent; text: button.text; textFormat: Text.PlainText; font.pixelSize: 11; font.bold: button.primary; color: button.primary ? "#1b2336" : "#dce1ed" }
        MouseArea { id: mouse; anchors.fill: parent; hoverEnabled: true; onClicked: { button.forceActiveFocus(); button.tapped() } }
    }
}
