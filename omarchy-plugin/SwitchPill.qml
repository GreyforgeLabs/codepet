pragma ComponentBehavior: Bound
import QtQuick

// A big On/Off switch (at least 96 x 48 at base-size 16). With `icon` "care-floor/power"
// it is the power switch of the shared header (SPEC-v0.3 §3). The words come from
// strings.json header.on / header.off, passed in as textOn / textOff.
Rectangle {
    id: sw
    required property var t
    property bool checked: false
    property string icon: ""
    property string textOn: "On"
    property string textOff: "Off"
    property string accessibleName: ""
    readonly property bool cpControl: true
    readonly property string cpKind: "switch"
    signal toggled(bool checked)

    implicitWidth: t.u(icon !== "" ? 104 : 96)
    implicitHeight: t.u(48)
    radius: height / 2
    color: checked ? t.accent : (mouse.containsMouse ? t.tileHover : t.tileFill)
    border.width: activeFocus ? Math.max(2, t.u(3)) : 1
    border.color: activeFocus ? t.focusRing : t.tileBorder
    activeFocusOnTab: true
    Accessible.role: Accessible.CheckBox
    Accessible.checkable: true
    Accessible.checked: checked
    Accessible.name: accessibleName !== "" ? accessibleName : (checked ? textOn : textOff)
    Accessible.onPressAction: sw.toggled(!sw.checked)
    Keys.onReturnPressed: sw.toggled(!sw.checked)
    Keys.onSpacePressed: sw.toggled(!sw.checked)
    Behavior on color { ColorAnimation { duration: 120 } }

    Rectangle {
        id: knob
        width: sw.height - sw.t.u(10)
        height: width
        radius: width / 2
        anchors.verticalCenter: parent.verticalCenter
        x: sw.checked ? sw.width - width - sw.t.u(5) : sw.t.u(5)
        color: sw.checked ? sw.t.onAccent : sw.t.mix(sw.t.text, 0.35)
        Behavior on x { NumberAnimation { duration: 120 } }
        Icon {
            anchors.centerIn: parent
            icon: sw.icon
            size: parent.width * 0.78
            visible: sw.icon !== ""
        }
    }
    Text {
        anchors.verticalCenter: parent.verticalCenter
        x: sw.checked ? sw.t.u(14) : knob.width + sw.t.u(14)
        width: sw.width - knob.width - sw.t.u(24)
        horizontalAlignment: Text.AlignHCenter
        text: sw.checked ? sw.textOn : sw.textOff
        textFormat: Text.PlainText
        font.family: sw.t.family
        font.pixelSize: sw.t.fontTitle
        font.bold: true
        color: sw.checked ? sw.t.onAccent : sw.t.text
    }
    MouseArea {
        id: mouse
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: sw.toggled(!sw.checked)
    }
}
