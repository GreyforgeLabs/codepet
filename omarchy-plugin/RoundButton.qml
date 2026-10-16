pragma ComponentBehavior: Bound
import QtQuick

// A 72 px round picture button with its word underneath (Home: Change, Helpers, More).
Item {
    id: round
    required property var t
    property string icon: ""
    property string text: ""
    readonly property bool cpControl: true
    readonly property string cpKind: "round"
    signal tapped

    implicitWidth: Math.max(t.u(72), caption.implicitWidth)
    implicitHeight: t.u(72) + caption.implicitHeight
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: text
    Accessible.onPressAction: round.tapped()
    Keys.onReturnPressed: round.tapped()
    Keys.onSpacePressed: round.tapped()

    Rectangle {
        id: disc
        anchors.horizontalCenter: parent.horizontalCenter
        width: round.t.u(72)
        height: width
        radius: width / 2
        color: mouse.pressed ? round.t.tilePress : mouse.containsMouse ? round.t.tileHover : round.t.tileFill
        border.width: round.activeFocus ? Math.max(2, round.t.u(3)) : 1
        border.color: round.activeFocus ? round.t.focusRing : round.t.tileBorder
        scale: mouse.pressed ? 0.94 : 1
        Behavior on scale { NumberAnimation { duration: 80 } }
        Icon {
            anchors.centerIn: parent
            icon: round.icon
            size: round.t.u(48)
        }
    }
    Text {
        id: caption
        anchors.top: disc.bottom
        anchors.horizontalCenter: parent.horizontalCenter
        text: round.text
        textFormat: Text.PlainText
        font.family: round.t.family
        font.pixelSize: round.t.fontTitle
        font.bold: true
        color: round.t.text
    }
    MouseArea {
        id: mouse
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: round.tapped()
    }
}
