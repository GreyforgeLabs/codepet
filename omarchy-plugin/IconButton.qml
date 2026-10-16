pragma ComponentBehavior: Bound
import QtQuick

// A 48 px (or larger) round picture button: the strip arrows, the rules help button and
// the guide's page arrows. `glyph` draws a big arrow when there is no icon. Never smaller
// than 48 x 48 at base-size 16 (SPEC-v0.3 K3).
Rectangle {
    id: ib
    required property var t
    property string icon: ""
    property string glyph: ""
    property string accessibleName: ""
    property real size: t.u(48)
    readonly property bool cpControl: true
    readonly property string cpKind: "control"
    signal tapped

    implicitWidth: size
    implicitHeight: size
    radius: width / 2
    color: mouse.pressed ? t.tilePress : mouse.containsMouse ? t.tileHover : t.tileFill
    border.width: activeFocus ? Math.max(2, t.u(3)) : 1
    border.color: activeFocus ? t.focusRing : t.tileBorder
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: accessibleName
    Accessible.onPressAction: ib.tapped()
    Keys.onReturnPressed: ib.tapped()
    Keys.onSpacePressed: ib.tapped()

    Icon {
        anchors.centerIn: parent
        icon: ib.icon
        size: ib.width * 0.72
        visible: ib.icon !== ""
    }
    Text {
        anchors.centerIn: parent
        visible: ib.icon === ""
        text: ib.glyph
        textFormat: Text.PlainText
        font.family: ib.t.family
        font.pixelSize: ib.t.fontDisplay
        font.bold: true
        color: ib.t.text
    }
    MouseArea {
        id: mouse
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: ib.tapped()
    }
}
