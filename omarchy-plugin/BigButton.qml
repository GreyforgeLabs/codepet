pragma ComponentBehavior: Bound
import QtQuick

// A big, friendly button. Primary actions (Let's go!, Connect, Turn on, ...) are at least
// 64 px tall and span the card; secondary ones (Not now, Keep, Close, ...) are real buttons
// at least 48 px tall, never text links (SPEC-v0.3 K3). `danger` uses Color.urgent and is
// kept for the Hardcore and goodbye confirmations.
Rectangle {
    id: button
    required property var t
    property string text: ""
    property string icon: ""
    // "primary" | "secondary" | "danger"
    property string kind: "primary"
    readonly property bool cpControl: true
    readonly property string cpKind: kind === "secondary" ? "secondary" : "primary"
    signal tapped

    implicitWidth: t.u(448)
    implicitHeight: kind === "secondary" ? t.u(48) : t.u(64)
    radius: t.radius
    color: kind === "secondary" ? (mouse.pressed ? t.tilePress : mouse.containsMouse ? t.tileHover : t.tileFill)
        : kind === "danger" ? t.urgent : t.accent
    border.width: activeFocus ? Math.max(2, t.u(3)) : (kind === "secondary" ? 1 : 0)
    border.color: activeFocus ? t.focusRing : t.tileBorder
    scale: mouse.pressed ? 0.97 : 1
    opacity: mouse.containsMouse && kind !== "secondary" ? 0.92 : 1
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: text
    Accessible.onPressAction: button.tapped()
    Keys.onReturnPressed: button.tapped()
    Keys.onSpacePressed: button.tapped()
    Behavior on scale { NumberAnimation { duration: 80 } }

    Row {
        anchors.centerIn: parent
        spacing: button.t.u(10)
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            icon: button.icon
            size: button.kind === "secondary" ? button.t.u(28) : button.t.u(40)
            visible: button.icon !== ""
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: button.text
            textFormat: Text.PlainText
            font.family: button.t.family
            font.pixelSize: button.kind === "secondary" ? button.t.fontTitle : button.t.fontHeading
            font.bold: true
            color: button.kind === "secondary" ? button.t.text : button.t.onAccent
            elide: Text.ElideRight
            width: Math.min(implicitWidth, button.width - button.t.u(32) - (button.icon !== "" ? button.t.u(50) : 0))
        }
    }
    MouseArea {
        id: mouse
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: button.tapped()
    }
}
