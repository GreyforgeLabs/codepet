pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Effects
import "CodePetBar.js" as Model

// The popup's pet frame player, ported from qml/shared/PetSprite.qml (the plugin cannot
// import the runtime QML). It owns no timer: `clockMs` comes from the popup's one
// animation clock, which runs only while the popup is open (SPEC-v0.3 §3.2, §8 rule 5).
// Frames load from the installed runtime (`runtimeBase` = file:// URL of
// ~/.local/share/codepet/current/); a missing frame falls back to the stage thumbnail,
// then to the painted egg.
Item {
    id: player
    property var registry: null
    property string species: ""
    property int stage: 1
    property string petState: "idle"
    property real clockMs: 0
    property bool calm: false
    property string runtimeBase: ""
    property bool dim: false
    property bool greyscale: false
    property bool mirror: false
    property real startMs: 0

    implicitWidth: 160
    implicitHeight: 160
    onPetStateChanged: player.startMs = player.clockMs
    onSpeciesChanged: player.startMs = player.clockMs

    readonly property string frame: Model.frameRel(registry, species, stage, petState, clockMs - startMs, calm)
    readonly property string thumb: Model.thumbPath(species, stage)
    readonly property string rel: frame !== "" ? frame : thumb
    readonly property bool broken: art.status === Image.Error || rel === "" || runtimeBase === ""

    Image {
        id: art
        anchors.fill: parent
        source: player.rel !== "" && player.runtimeBase !== "" ? player.runtimeBase + player.rel : ""
        sourceSize.width: Math.max(96, Math.min(384, Math.ceil(player.width * 2)))
        sourceSize.height: Math.max(96, Math.min(384, Math.ceil(player.height * 2)))
        fillMode: Image.PreserveAspectFit
        smooth: true
        asynchronous: false
        cache: true
        mirror: player.mirror
        opacity: player.dim ? 0.45 : 1
        visible: !player.broken
        layer.enabled: player.greyscale
        layer.effect: MultiEffect { saturation: -1 }
    }
    Icon {
        anchors.centerIn: parent
        icon: "ui/egg"
        size: Math.min(player.width, player.height) * 0.9
        visible: player.broken
        opacity: player.dim ? 0.45 : 1
    }
}
