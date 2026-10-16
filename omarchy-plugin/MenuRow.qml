pragma ComponentBehavior: Bound
import QtQuick

// A 64 px row for More and the right-click menu: a picture, one line and, on the right, a
// switch, an action button or a "›" that opens a page. The whole row is the tap target
// (a switch row toggles its switch).
Rectangle {
    id: row
    required property var t
    property string icon: ""
    property string text: ""
    // "page" | "switch" | "action"
    property string kind: "page"
    property bool checked: false
    property string actionText: ""
    property string actionIcon: ""
    property string textOn: "On"
    property string textOff: "Off"
    property bool danger: false
    readonly property bool cpControl: kind === "page"
    readonly property string cpKind: "row"
    signal tapped
    signal toggled(bool checked)

    implicitWidth: t.u(448)
    implicitHeight: t.u(64)
    radius: t.smallRadius
    color: mouse.pressed && kind !== "action" ? t.tilePress : mouse.containsMouse && kind !== "action" ? t.tileHover : t.tileFill
    border.width: activeFocus ? Math.max(2, t.u(3)) : 0
    border.color: t.focusRing
    activeFocusOnTab: kind === "page"
    Accessible.role: kind === "switch" ? Accessible.CheckBox : Accessible.Button
    Accessible.name: text
    Accessible.onPressAction: row.activate()
    Keys.onReturnPressed: row.activate()
    Keys.onSpacePressed: row.activate()

    function activate() {
        if (row.kind === "switch") row.toggled(!row.checked)
        else if (row.kind === "page") row.tapped()
    }

    Icon {
        id: pic
        anchors.left: parent.left
        anchors.leftMargin: row.t.u(10)
        anchors.verticalCenter: parent.verticalCenter
        icon: row.icon
        size: row.t.u(44)
    }
    Text {
        anchors.left: pic.right
        anchors.leftMargin: row.t.u(12)
        anchors.right: right.left
        anchors.rightMargin: row.t.u(8)
        anchors.verticalCenter: parent.verticalCenter
        text: row.text
        textFormat: Text.PlainText
        font.family: row.t.family
        font.pixelSize: row.t.fontTitle
        font.bold: true
        color: row.danger ? row.t.urgent : row.t.text
        elide: Text.ElideRight
    }
    MouseArea {
        id: mouse
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: row.kind === "action" ? Qt.ArrowCursor : Qt.PointingHandCursor
        onClicked: row.activate()
    }
    Item {
        id: right
        anchors.right: parent.right
        anchors.rightMargin: row.t.u(8)
        anchors.verticalCenter: parent.verticalCenter
        width: row.kind === "page" ? chevron.implicitWidth : row.kind === "switch" ? sw.width : action.width
        height: row.t.u(48)
        Text {
            id: chevron
            visible: row.kind === "page"
            anchors.verticalCenter: parent.verticalCenter
            text: "›"
            textFormat: Text.PlainText
            font.family: row.t.family
            font.pixelSize: row.t.fontDisplay
            font.bold: true
            color: row.t.dim
        }
        SwitchPill {
            id: sw
            visible: row.kind === "switch"
            t: row.t
            anchors.verticalCenter: parent.verticalCenter
            checked: row.checked
            textOn: row.textOn
            textOff: row.textOff
            accessibleName: row.text
            onToggled: c => row.toggled(c)
        }
        BigButton {
            id: action
            visible: row.kind === "action"
            t: row.t
            kind: "secondary"
            anchors.verticalCenter: parent.verticalCenter
            width: row.t.u(150)
            icon: row.actionIcon
            text: row.actionText
            onTapped: row.tapped()
        }
    }
}
