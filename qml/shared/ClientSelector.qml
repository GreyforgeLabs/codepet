pragma ComponentBehavior: Bound
import QtQuick
import "../omarchy/WorkModel.js" as Work

Column {
    id: selector
    required property var controller
    property bool expanded: false
    readonly property var snapshot: controller ? controller.state : null
    readonly property var task: snapshot ? snapshot.task : null
    readonly property string selectedId: Work.clientId(snapshot)
    readonly property bool saving: !!(controller && controller.selectingClient)
    readonly property string reason: snapshot && snapshot.provider && !snapshot.provider.available
        ? String(snapshot.provider.reason || "This client is not ready.") : ""
    spacing: 4
    onSelectedIdChanged: expanded = false
    onSavingChanged: if (!saving && controller && !controller.error) expanded = false
    onVisibleChanged: if (!visible) expanded = false
    onExpandedChanged: if (!expanded && visible) toggle.forceActiveFocus()
    Keys.onEscapePressed: event => {
        if (expanded) { expanded = false; toggle.forceActiveFocus() }
        else event.accepted = false
    }
    function focusInitial() { toggle.forceActiveFocus(Qt.OtherFocusReason) }

    Row {
        width: parent.width
        spacing: 6
        Choice {
            id: toggle
            objectName: "workClient.toggle"
            width: Math.min(implicitWidth, parent.width)
            text: "Client: " + Work.clientName(selector.snapshot) + (selector.expanded ? " ▴" : " ▾")
            label: text + ". Choose the client for the next CodePet job."
            onChosen: selector.expanded = !selector.expanded
        }
        Text {
            objectName: "workClient.origin"
            visible: !!selector.task && !Work.finished(selector.task)
            width: Math.max(0, parent.width - x)
            anchors.verticalCenter: parent.verticalCenter
            text: (Work.finding(selector.task) ? "Found by " : "Job: ") + Work.taskClientName(selector.task)
            textFormat: Text.PlainText; elide: Text.ElideRight
            font.pixelSize: 10; color: "#b8bfcc"
        }
    }
    Column {
        width: parent.width
        spacing: 4
        visible: selector.expanded
        Text {
            width: parent.width
            text: "For CodePet jobs. Helpers watch separate sessions. T3 is a cockpit; select its underlying client here."
            textFormat: Text.PlainText; wrapMode: Text.Wrap
            font.pixelSize: 11; color: "#b8bfcc"
        }
        Text {
            visible: !!selector.task
            width: parent.width
            text: (Work.finding(selector.task) ? "Finding from " : "This job uses ") + Work.taskClientName(selector.task)
                + (Work.finding(selector.task) ? ". Fix will use " + Work.clientName(selector.snapshot) + "." : ". Changing this choice affects the next job.")
            textFormat: Text.PlainText; wrapMode: Text.Wrap
            font.pixelSize: 11; color: "#b8bfcc"
        }
        Text {
            objectName: "workClient.detail"
            width: parent.width
            text: selector.snapshot && selector.snapshot.provider ? String(selector.snapshot.provider.detail || "") : ""
            visible: text !== ""
            textFormat: Text.PlainText; wrapMode: Text.Wrap
            font.pixelSize: 11; color: "#b8bfcc"
        }
        Repeater {
            model: selector.snapshot && Array.isArray(selector.snapshot.clients) ? selector.snapshot.clients : []
            Choice {
                required property var modelData
                objectName: "workClient.option." + modelData.id
                width: parent.width
                text: String(modelData.name) + (selector.selectedId === modelData.id ? " ✓" : "") + (modelData.available ? "" : " · Not ready")
                label: text + (modelData.available ? "" : ". " + String(modelData.reason || "Setup required."))
                enabled: !!selector.controller && !selector.controller.busy
                onChosen: selector.controller.selectClient(modelData.id)
            }
        }
        Text {
            visible: !selector.snapshot || !Array.isArray(selector.snapshot.clients)
            width: parent.width; text: "Client selection needs an updated CodePet backend."
            textFormat: Text.PlainText; wrapMode: Text.Wrap; font.pixelSize: 11; color: "#ffbea9"
        }
    }
    Text {
        objectName: "workClient.reason"
        visible: selector.saving || selector.reason !== ""
        width: parent.width
        text: selector.saving ? "Saving choice…" : selector.reason
        textFormat: Text.PlainText; wrapMode: Text.Wrap
        maximumLineCount: selector.expanded ? 2147483647 : 2; elide: Text.ElideRight
        font.pixelSize: 11; color: "#ffbea9"
    }
    component Choice: Rectangle {
        id: choice
        property string text: ""
        property string label: text
        signal chosen()
        implicitWidth: caption.implicitWidth + 16
        implicitHeight: 24
        radius: 6
        color: mouse.pressed ? "#465165" : mouse.containsMouse ? "#3b4352" : "#2c333f"
        opacity: enabled ? 1 : 0.5
        border.width: activeFocus ? 2 : 0; border.color: "#f2dca0"
        activeFocusOnTab: true
        Accessible.role: Accessible.Button; Accessible.name: label
        Accessible.onPressAction: if (enabled) chosen()
        Keys.onReturnPressed: if (enabled) chosen()
        Keys.onSpacePressed: if (enabled) chosen()
        Text { id: caption; anchors.fill: parent; anchors.margins: 8; anchors.topMargin: 0; anchors.bottomMargin: 0; verticalAlignment: Text.AlignVCenter; text: choice.text; textFormat: Text.PlainText; elide: Text.ElideRight; font.pixelSize: 11; color: "#dce1ed" }
        MouseArea { id: mouse; anchors.fill: parent; hoverEnabled: true; onClicked: { choice.forceActiveFocus(); choice.chosen() } }
    }
}
