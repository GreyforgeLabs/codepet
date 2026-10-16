pragma ComponentBehavior: Bound
import QtQuick

// The shared 64 px header of every popup screen (SPEC-v0.3 §3): ‹ Back (hidden on Home,
// first run and welcome back), the title or the pet's name button (whole name + pencil,
// opens the rename field), and the power switch, which is on every screen.
Item {
    id: header
    required property var t
    property string title: ""
    property bool showBack: false
    property bool nameButton: false
    property bool powerOn: true
    property bool showPower: true
    property string backText: "Back"
    property string textOn: "On"
    property string textOff: "Off"
    signal back
    signal rename
    signal power(bool on)

    implicitWidth: t.u(448)
    implicitHeight: t.u(64)

    Rectangle {
        id: backButton
        objectName: "cp.back"
        readonly property bool cpControl: true
        readonly property string cpKind: "control"
        visible: header.showBack
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        width: Math.max(header.t.u(48), backRow.implicitWidth + header.t.u(24))
        height: header.t.u(48)
        radius: height / 2
        color: backMouse.pressed ? header.t.tilePress : backMouse.containsMouse ? header.t.tileHover : header.t.tileFill
        border.width: activeFocus ? Math.max(2, header.t.u(3)) : 1
        border.color: activeFocus ? header.t.focusRing : header.t.tileBorder
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: header.backText
        Accessible.onPressAction: header.back()
        Keys.onReturnPressed: header.back()
        Keys.onSpacePressed: header.back()
        Row {
            id: backRow
            anchors.centerIn: parent
            spacing: header.t.u(4)
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: "‹"
                textFormat: Text.PlainText
                font.family: header.t.family
                font.pixelSize: header.t.fontDisplay
                font.bold: true
                color: header.t.text
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: header.backText
                textFormat: Text.PlainText
                font.family: header.t.family
                font.pixelSize: header.t.fontTitle
                font.bold: true
                color: header.t.text
            }
        }
        MouseArea {
            id: backMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: header.back()
        }
    }

    readonly property real titleLeft: header.showBack ? backButton.width + header.t.u(8) : 0
    readonly property real titleRight: header.showPower ? power.width + header.t.u(8) : 0

    Text {
        id: titleText
        visible: !header.nameButton
        x: header.titleLeft
        width: header.width - header.titleLeft - header.titleRight
        anchors.verticalCenter: parent.verticalCenter
        horizontalAlignment: header.showBack ? Text.AlignHCenter : Text.AlignLeft
        text: header.title
        textFormat: Text.PlainText
        font.family: header.t.family
        font.pixelSize: header.t.fontHeading
        font.bold: true
        color: header.t.text
        elide: Text.ElideRight
    }

    Rectangle {
        id: nameButton
        objectName: "cp.nameButton"
        readonly property bool cpControl: true
        readonly property string cpKind: "control"
        visible: header.nameButton
        x: header.titleLeft
        anchors.verticalCenter: parent.verticalCenter
        width: Math.min(header.width - header.titleLeft - header.titleRight, nameRow.implicitWidth + header.t.u(28))
        height: header.t.u(52)
        radius: header.t.radius
        color: nameMouse.pressed ? header.t.tilePress : nameMouse.containsMouse ? header.t.tileHover : header.t.tileFill
        border.width: activeFocus ? Math.max(2, header.t.u(3)) : 1
        border.color: activeFocus ? header.t.focusRing : header.t.tileBorder
        activeFocusOnTab: true
        Accessible.role: Accessible.Button
        Accessible.name: header.title
        Accessible.onPressAction: header.rename()
        Keys.onReturnPressed: header.rename()
        Keys.onSpacePressed: header.rename()
        Row {
            id: nameRow
            anchors.centerIn: parent
            spacing: header.t.u(8)
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: header.title
                textFormat: Text.PlainText
                font.family: header.t.family
                font.pixelSize: header.t.fontHeading
                font.bold: true
                color: header.t.text
                elide: Text.ElideRight
                width: Math.min(implicitWidth, header.t.u(250))
            }
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                icon: "ui/pencil"
                size: header.t.u(28)
            }
        }
        MouseArea {
            id: nameMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: header.rename()
        }
    }

    SwitchPill {
        id: power
        objectName: "cp.power"
        t: header.t
        visible: header.showPower
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        icon: "care-floor/power"
        checked: header.powerOn
        textOn: header.textOn
        textOff: header.textOff
        onToggled: on => header.power(on)
    }
}
