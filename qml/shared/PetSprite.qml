import QtQuick

// All animation timing belongs to the scene's single scheduler. This component
// is deliberately a passive renderer: no timers, animations, or backend calls.
Item {
    id: root
    implicitWidth: 112
    implicitHeight: 112
    property string pose: "idle"
    property int frame: 0
    property bool facingLeft: false
    readonly property var supportedPoses: ["idle", "walk", "sleep", "play", "food", "water", "wash", "bathroom"]
    readonly property string effectivePose: supportedPoses.indexOf(pose) >= 0 ? pose : "idle"
    readonly property int effectiveFrame: ((frame % 4) + 4) % 4

    Image {
        id: artwork
        objectName: "petSpriteArtwork"
        anchors.fill: parent
        source: Qt.resolvedUrl("../assets/ginger-" + root.effectivePose + "-" + root.effectiveFrame + ".svg")
        sourceSize: Qt.size(224, 224)
        fillMode: Image.PreserveAspectFit
        mirror: root.facingLeft
        smooth: true
        mipmap: false
        asynchronous: false
        cache: true
    }
}
