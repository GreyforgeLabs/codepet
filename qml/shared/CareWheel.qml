pragma ComponentBehavior: Bound
import QtQuick

Rectangle {
    id: wheel
    property string careSet: "floor"
    property bool pinned: false
    property bool sleeping: false
    property bool paused: false
    property bool fainted: false
    property string message: ""
    signal chosen(string action)
    signal closeRequested()
    implicitWidth: 252
    implicitHeight: 304
    radius: 24
    color: "#f2232b36"
    border.color: "#897d8b9b"
    Keys.onEscapePressed: closeRequested()
    function focusInitial() {
        var first = actions.itemAt(0)
        if (visible && first) first.forceActiveFocus(Qt.OtherFocusReason)
    }

    Item {
        anchors.centerIn: parent
        width: 252; height: 304
        scale: Math.min(wheel.width / width, wheel.height / height)
        Repeater {
            id: actions
            model: [
                { action: "pet", label: "Pet", icon: "care-floor/hand", x: 94, y: 8 },
                { action: "food", label: "Snack", icon: "care-" + wheel.careSet + "/food", x: 176, y: 68 },
                { action: "water", label: "Drink", icon: "care-" + wheel.careSet + "/water", x: 176, y: 154 },
                { action: "wash", label: "Bath", icon: "care-" + wheel.careSet + "/wash", x: 132, y: 234 },
                { action: "play", label: "Play", icon: "care-" + wheel.careSet + "/play", x: 56, y: 234 },
                { action: "sleep", label: wheel.paused ? "Resume" : wheel.sleeping ? "Wake" : "Nap", icon: "care-floor/" + (wheel.sleeping || wheel.paused ? "sun_wake" : "bed"), x: 12, y: 154 },
                { action: "pin", label: wheel.pinned ? "Roam" : "Pin", icon: "care-floor/pin", x: 12, y: 68 }
            ]
            Rectangle {
                required property var modelData
                objectName: "desktopPet.wheel." + modelData.action
                x: modelData.x; y: modelData.y
                width: 64; height: 62; radius: 18
                enabled: modelData.action !== "sleep" || !wheel.fainted || wheel.sleeping || wheel.paused
                opacity: enabled ? 1 : 0.45
                color: pointer.pressed ? "#667c91" : pointer.containsMouse ? "#49596c" : "#333f50"
                border.color: activeFocus ? "#a8dacc" : "#586679"
                activeFocusOnTab: true
                Accessible.role: Accessible.Button
                Accessible.name: modelData.label
                Accessible.description: modelData.action === "sleep" && !enabled ? "Give Snack and Drink to recover before a nap." : ""
                Accessible.onPressAction: if (enabled) wheel.chosen(modelData.action)
                Keys.onReturnPressed: if (enabled) wheel.chosen(modelData.action)
                Keys.onSpacePressed: if (enabled) wheel.chosen(modelData.action)
                Image {
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: 3; width: 36; height: 36
                    source: Qt.resolvedUrl("../assets/icons/" + parent.modelData.icon + ".webp")
                    fillMode: Image.PreserveAspectFit
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: 41; text: parent.modelData.label
                    textFormat: Text.PlainText
                    font.pixelSize: 12; color: "#f0f4fa"
                }
                MouseArea { id: pointer; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { parent.forceActiveFocus(); wheel.chosen(parent.modelData.action) } }
            }
        }
        Rectangle {
            objectName: "desktopPet.wheel.close"
            x: 102; y: 102; width: 48; height: 40; radius: 16
            color: closeMouse.containsMouse ? "#49596c" : "#333f50"
            border.color: activeFocus ? "#a8dacc" : "#586679"
            activeFocusOnTab: true
            Accessible.role: Accessible.Button
            Accessible.name: "Close care wheel"
            Accessible.onPressAction: wheel.closeRequested()
            Keys.onReturnPressed: wheel.closeRequested()
            Keys.onSpacePressed: wheel.closeRequested()
            Text { anchors.centerIn: parent; text: "×"; font.pixelSize: 24; color: "white" }
            MouseArea { id: closeMouse; anchors.fill: parent; hoverEnabled: true; onClicked: wheel.closeRequested() }
        }
        Text {
            objectName: "desktopPet.wheel.message"
            x: 77; y: 148; width: 98; height: 48
            text: wheel.message || (wheel.fainted ? "Snack + Drink\nto recover" : wheel.paused ? "Paused" : wheel.sleeping ? "Sleeping" : wheel.pinned ? "Pinned" : "Roaming")
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            font.pixelSize: 11; color: "#cedce9"
        }
    }
}
