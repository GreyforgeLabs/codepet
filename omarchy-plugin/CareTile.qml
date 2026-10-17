pragma ComponentBehavior: Bound
import QtQuick

// One of Home's four care tiles: painted action, saved percentage and a proportional bar.
// `glow` "now" lights the tile up and gently pulses it (no pulse in Calm mode); "soon" adds
// an accent dot (SPEC-v0.3 §2.4).
Rectangle {
    id: tile
    required property var t
    property string icon: ""
    property string text: ""
    property string detail: ""
    property string glow: ""
    property bool calm: false
    property var meter: ({ known: false, fraction: 0 })
    property string meterText: ""
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
    Accessible.description: meterText
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

    Item {
        anchors.left: parent.left
        anchors.leftMargin: tile.t.u(14)
        y: tile.t.u(6)
        width: parent.width - tile.t.u(28)
        height: tile.t.u(76)
        Icon {
            y: tile.t.u(2)
            icon: tile.icon
            size: tile.t.u(tile.detail !== "" ? 54 : 64)
        }
        Text {
            x: tile.t.u(76)
            height: tile.t.u(56)
            verticalAlignment: Text.AlignVCenter
            width: parent.width - x
            text: tile.text
            textFormat: Text.PlainText
            font.family: tile.t.family
            font.pixelSize: tile.t.fontTitle
            font.bold: true
            color: tile.t.text
            wrapMode: Text.WordWrap
            maximumLineCount: 2
            elide: Text.ElideRight
        }
        Text {
            anchors.left: parent.left
            anchors.bottom: parent.bottom
            text: tile.detail
            textFormat: Text.PlainText
            font.family: tile.t.family
            font.pixelSize: tile.t.fontTitle
            color: tile.t.text
        }
        Text {
            objectName: "cp.meter.value"
            readonly property bool cpStatistic: true
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            text: tile.meterText
            textFormat: Text.PlainText
            font.family: tile.t.family
            font.pixelSize: tile.t.fontBody
            font.bold: true
            color: tile.t.text
        }
    }
    Rectangle {
        id: meterTrack
        objectName: "cp.meter.track"
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: tile.t.u(12)
        height: tile.t.u(6)
        radius: height / 2
        color: tile.t.tileBorder
        Rectangle {
            objectName: "cp.meter.fill"
            width: meterTrack.width * tile.meter.fraction
            height: parent.height
            radius: height / 2
            color: tile.glow === "now" ? tile.t.urgent : tile.t.accent
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
