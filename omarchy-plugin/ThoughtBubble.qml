pragma ComponentBehavior: Bound
import QtQuick

// The pet's picture-only thought bubble: the top need, up to three app pictures, the
// bandage, a star or the helpers robot (SPEC-v0.3 §3.2). At most 72 px tall at base 16.
Item {
    id: bubble
    required property var t
    property var icons: []
    property bool big: true
    readonly property real iconSize: t.u(big ? 48 : 36)

    visible: icons.length > 0
    implicitWidth: cloud.width
    implicitHeight: t.u(big ? 72 : 60)

    Rectangle {
        id: cloud
        width: row.implicitWidth + bubble.t.u(24)
        height: bubble.iconSize + bubble.t.u(16)
        radius: height / 2
        color: bubble.t.background
        border.width: Math.max(1, bubble.t.u(2))
        border.color: bubble.t.tileBorder
        Row {
            id: row
            anchors.centerIn: parent
            spacing: bubble.t.u(6)
            Repeater {
                model: bubble.icons
                Icon {
                    required property var modelData
                    icon: String(modelData)
                    size: bubble.iconSize
                }
            }
        }
    }
    // The little thought dots toward the pet.
    Rectangle {
        x: bubble.t.u(6)
        y: cloud.height + bubble.t.u(2)
        width: bubble.t.u(12); height: width; radius: width / 2
        color: bubble.t.background
        border.width: 1
        border.color: bubble.t.tileBorder
    }
}
