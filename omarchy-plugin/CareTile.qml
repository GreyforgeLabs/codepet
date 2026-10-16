pragma ComponentBehavior: Bound
import QtQuick

// One of Home's four big care tiles (220 x 104 at base-size 16): a painted icon and one word.
// `glow` "now" lights the tile up and gently pulses it (no pulse in Calm mode); "soon" adds
// an accent dot (SPEC-v0.3 §2.4).
Rectangle {
    id: tile
    required property var t
    property string icon: ""
    property string text: ""
    property string glow: ""
    property bool calm: false
    readonly property bool cpControl: true
    readonly property string cpKind: "tile"
    readonly property bool pulsing: glow === "now" && !calm && visible
    signal tapped

    implicitWidth: t.u(220)
    implicitHeight: t.u(104)
    radius: t.radius
    color: mouse.pressed ? t.tilePress : glow === "now" ? t.glowFill : mouse.containsMouse ? t.tileHover : t.tileFill
    border.width: activeFocus || glow === "now" ? Math.max(2, t.u(4)) : 1
    border.color: activeFocus ? t.focusRing : glow === "now" ? t.glowBorder : t.tileBorder
    scale: mouse.pressed ? 0.96 : 1
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: text
    Accessible.onPressAction: tile.tapped()
    Keys.onReturnPressed: tile.tapped()
    Keys.onSpacePressed: tile.tapped()
    Behavior on scale { NumberAnimation { duration: 80 } }

    SequentialAnimation on opacity {
        running: tile.pulsing
        loops: Animation.Infinite
        alwaysRunToEnd: true
        NumberAnimation { to: 0.78; duration: 700; easing.type: Easing.InOutSine }
        NumberAnimation { to: 1; duration: 700; easing.type: Easing.InOutSine }
    }

    Row {
        anchors.left: parent.left
        anchors.leftMargin: tile.t.u(14)
        anchors.verticalCenter: parent.verticalCenter
        spacing: tile.t.u(12)
        Icon {
            anchors.verticalCenter: parent.verticalCenter
            icon: tile.icon
            size: tile.t.u(72)
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            width: tile.width - tile.t.u(72 + 14 + 12 + 10)
            text: tile.text
            textFormat: Text.PlainText
            font.family: tile.t.family
            font.pixelSize: tile.t.fontHeading
            font.bold: true
            color: tile.t.text
            wrapMode: Text.WordWrap
            maximumLineCount: 2
            elide: Text.ElideRight
        }
    }
    Rectangle {
        visible: tile.glow === "soon"
        anchors.top: parent.top
        anchors.right: parent.right
        anchors.margins: tile.t.u(10)
        width: tile.t.u(16)
        height: width
        radius: width / 2
        color: tile.t.accent
    }
    MouseArea {
        id: mouse
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: tile.tapped()
    }
}
