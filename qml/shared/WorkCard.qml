pragma ComponentBehavior: Bound
import QtQuick
import "../omarchy/WorkModel.js" as Work
import "PetAge.js" as PetAge

Rectangle {
    id: card
    required property var controller
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
    readonly property bool scouting: !!task && task.source_kind === "scout"
    readonly property bool checksOn: !!(snapshot && snapshot.scout && snapshot.scout.enabled)
    readonly property string heading: finding ? "Suspected bug · " + Work.repoName(task)
        : working ? (scouting ? "Looking for bugs…" : "Working on " + Work.repoName(task) + "…")
        : review ? (task.kind === "bug" ? "Fix ready · " : "Work ready · ") + Work.repoName(task)
        : task && task.status === "accepted" ? "Nice work!"
        : task && task.status === "failed" ? "This check couldn’t finish"
        : task && task.status === "cancelled" ? "Work stopped"
        : task && task.status === "scouted" ? "No bug found this time"
        : "Your local repo buddy"
    readonly property string message: finding ? (task.finding ? String(task.finding.title) : "Open again to read this finding.")
        : working ? String(task.progress || "Taking a closer look.")
        : review ? "Review the changes before applying them."
        : task && task.status === "accepted" ? "+20 XP · " + (task.kind === "research" ? "Thirst restored." : task.kind === "build" ? "Mood lifted." : "A well-earned meal.")
        : task && task.status === "failed" ? String(task.error || "You can try again later.")
        : snapshot && snapshot.provider && !snapshot.provider.available ? String(snapshot.provider.reason || "Connect Codex to check your repositories.")
        : snapshot && snapshot.readiness && !snapshot.readiness.ready ? String(snapshot.readiness.reason)
        : snapshot && snapshot.scout && snapshot.scout.notice ? String(snapshot.scout.notice)
        : checksOn ? "Checks local repositories when your pet is ready."
        : "Automatic bug checks are paused."
    signal closeRequested()
    signal homeRequested()
    signal reviewRequested()
    implicitWidth: 280
    implicitHeight: body.implicitHeight + 28
    radius: 16
    color: "#f51f242c"
    border.color: "#49505e"
    border.width: 1
    clip: true
    onTaskChanged: expanded = false

    // The card itself is an input island. Blank card space must not drag the pet
    // or fall through; the rest of the transparent desktop remains untouched.
    MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons }
    Flickable {
        anchors.fill: parent
        anchors.margins: 14
        contentWidth: width
        contentHeight: body.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        Column {
            id: body
            width: parent.width
            spacing: 10
            Row {
                width: parent.width
                spacing: 8
                Text {
                    objectName: "workCard.heading"
                    width: parent.width - (close.visible ? close.width + 8 : 0)
                    text: card.heading; textFormat: Text.PlainText
                    wrapMode: Text.Wrap; font.pixelSize: 13; font.bold: true; color: "#b9c5e9"
                }
                CardButton { id: close; visible: card.closable; text: "×"; label: "Close card"; width: 24; height: 24; onTapped: card.closeRequested() }
            }
            Text {
                objectName: "workCard.message"
                width: parent.width
                text: card.message; textFormat: Text.PlainText
                wrapMode: Text.Wrap; maximumLineCount: card.expanded ? 30 : 3; elide: Text.ElideRight
                font.pixelSize: 15; color: "#edf0f5"
            }
            Text {
                visible: card.finding
                width: parent.width
                text: "Reported by Codex · needs review"
                textFormat: Text.PlainText; wrapMode: Text.Wrap
                font.pixelSize: 11; color: "#b9b5a5"
            }
            Text {
                objectName: "workCard.stats"
                visible: !!card.life
                width: parent.width
                text: (card.life && typeof card.life.level === "number" && typeof card.life.xp === "number"
                    ? "Lv " + card.life.level + " · " + card.life.xp + " XP · " : "") + PetAge.shortText(card.life, card.now)
                textFormat: Text.PlainText; wrapMode: Text.Wrap
                font.pixelSize: 11; color: "#b8bfcc"
            }
            Text {
                objectName: "workCard.error"
                visible: !!(card.controller && card.controller.error)
                width: parent.width
                text: card.controller ? card.controller.error : ""
                textFormat: Text.PlainText; wrapMode: Text.Wrap
                font.pixelSize: 12; color: "#ffbea9"
            }
            Row {
                spacing: 8
                visible: card.finding || card.working || card.review
                CardButton {
                    objectName: "workCard.primary"
                    text: card.finding ? "Fix" : card.working ? "Stop" : "Review"
                    primary: !card.working
                    enabled: !!card.controller && !card.controller.busy && (!card.finding || !!(card.task.finding && card.task.review_digest))
                    onTapped: {
                        if (card.finding) card.controller.fix()
                        else if (card.working) card.controller.action("cancel", { task_id: card.task.id })
                        else card.reviewRequested()
                    }
                }
                CardButton {
                    visible: card.finding && card.expanded
                    text: "Dismiss"
                    enabled: !!card.controller && !card.controller.busy
                    onTapped: card.controller.action("discard", { task_id: card.task.id })
                }
            }
            Text {
                visible: card.expanded
                width: parent.width
                text: card.task && card.task.finding
                    ? String(card.task.finding.path || "") + (card.task.finding.line ? ":" + card.task.finding.line : "")
                      + "\n\n" + String(card.task.finding.evidence || "") + "\n\n" + String(card.task.finding.check || "")
                      + "\n\nThis is a suspected bug reported by the provider. Fix prepares a candidate for your review."
                    : String(card.task && card.task.output || "Checks use Codex to inspect local repositories. A check does not edit your original files. Fixes need your review before applying.")
                textFormat: Text.PlainText; wrapMode: Text.WrapAnywhere
                font.pixelSize: 12; color: "#b8bfcc"
            }
            Row {
                width: parent.width; spacing: 6
                CardButton { text: "Care"; height: 28; onTapped: card.homeRequested() }
                CardButton { text: card.expanded ? "Less" : "Details"; height: 28; onTapped: card.expanded = !card.expanded }
                CardButton {
                    objectName: "workCard.checks"
                    visible: card.expanded
                    text: card.checksOn ? "Checks on" : "Checks off"; height: 28
                    enabled: !!card.snapshot && !!card.snapshot.scout && !card.controller.busy
                    onTapped: card.controller.action("scout-settings", { enabled: !card.checksOn })
                }
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
        implicitHeight: 32
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
        Text { id: caption; anchors.centerIn: parent; text: button.text; textFormat: Text.PlainText; font.pixelSize: 12; font.bold: button.primary; color: button.primary ? "#1b2336" : "#dce1ed" }
        MouseArea { id: mouse; anchors.fill: parent; hoverEnabled: true; onClicked: button.tapped() }
    }
}
