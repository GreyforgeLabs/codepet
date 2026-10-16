pragma ComponentBehavior: Bound
import QtQuick

// A "working on it" ring for a Connecting… row. It spins with a RotationAnimator (no
// Timer) and stands still in Calm mode.
Item {
    id: spinner
    required property var t
    property bool calm: false
    Rectangle {
        id: ring
        anchors.centerIn: parent
        width: Math.min(parent.width, parent.height) * 0.8
        height: width
        radius: width / 2
        color: "transparent"
        border.width: Math.max(3, spinner.t.u(5))
        border.color: spinner.t.tileBorder
        Rectangle {
            width: Math.max(6, spinner.t.u(10))
            height: width
            radius: width / 2
            color: spinner.t.accent
            anchors.horizontalCenter: parent.horizontalCenter
            y: -height / 4
        }
        RotationAnimator on rotation {
            from: 0
            to: 360
            duration: 1100
            loops: Animation.Infinite
            running: spinner.visible && !spinner.calm
        }
    }
}
