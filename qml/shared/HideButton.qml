import QtQuick

// The desktop pet's HIDE pill: an eye with a slash and the word HIDE. It appears while
// the pet is hovered (DesktopPet decides when); a click asks the host to hide the pet.
// The host brings it back with `codepet-overlay show` or `toggle` (a hotkey).
Rectangle {
    id: button
    readonly property bool hovered: pointer.containsMouse
    signal activated()
    implicitWidth: row.implicitWidth + 16
    implicitHeight: 26
    radius: height / 2
    color: pointer.pressed ? "#667c91" : pointer.containsMouse ? "#49596c" : "#f2232b36"
    border.color: activeFocus ? "#a8dacc" : "#897d8b9b"
    border.width: 1
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: "Hide pet"
    Accessible.description: "Hides the desktop pet until you show it again."
    Accessible.onPressAction: button.activated()
    Keys.onReturnPressed: button.activated()
    Keys.onSpacePressed: button.activated()

    Row {
        id: row
        anchors.centerIn: parent
        spacing: 5
        Canvas {
            id: eye
            width: 16; height: 12
            anchors.verticalCenter: parent.verticalCenter
            onPaint: {
                var c = getContext("2d")
                c.reset()
                c.strokeStyle = "#f0f4fa"
                c.fillStyle = "#f0f4fa"
                c.lineWidth = 1.6
                c.lineCap = "round"
                // Almond outline, pupil, then the slash.
                c.beginPath()
                c.moveTo(1, 6)
                c.quadraticCurveTo(8, -1.5, 15, 6)
                c.quadraticCurveTo(8, 13.5, 1, 6)
                c.closePath()
                c.stroke()
                c.beginPath()
                c.arc(8, 6, 2.2, 0, Math.PI * 2)
                c.fill()
                c.beginPath()
                c.moveTo(2.5, 11)
                c.lineTo(13.5, 1)
                c.stroke()
            }
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: "HIDE"
            textFormat: Text.PlainText
            font.pixelSize: 11; font.bold: true; font.letterSpacing: 0.8
            color: "#f0f4fa"
        }
    }
    MouseArea {
        id: pointer
        objectName: "desktopPet.hide.mouse"
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: button.activated()
    }
}
