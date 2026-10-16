pragma ComponentBehavior: Bound
import QtQuick
import "CodePetBar.js" as Model
import "Strings.js" as Strings

// Home (SPEC-v0.3 §3.2) and its Off look (§3.7). A big animated pet with its thought
// bubble and the Treats button, one sentence, the four picture tiles and the three round
// buttons. Fainted, asleep and Off use the same frame with fewer controls (K4, K6), so
// every look fits the 640 px budget at base sizes 12, 16 and 20 (K13).
Item {
    id: home
    required property var app
    readonly property var t: app.t
    readonly property var doc: app.doc
    readonly property var life: app.life
    readonly property var face: app.face
    readonly property var controls: Model.homeControls(doc, face)
    readonly property bool off: face && face.state === "off"
    readonly property string appearance: life ? String(life.appearance || "") : ""

    implicitWidth: t.u(448)
    implicitHeight: column.implicitHeight

    // ------------------------------------------------------------- taps (K5)

    function care(need) {
        var full = Model.careIsFull(home.doc, need)
        var name = home.life ? home.life.name : ""
        home.app.say(Model.reactText(Strings.STRINGS, home.app.reg, home.appearance, need, full, name), need)
        home.app.request("pet.care", { action: need }, function (resp) {
            if (!resp.ok) { home.app.failed(resp); return }
            if (resp.result && resp.result.changed === false && !full)
                home.app.say(Model.reactText(Strings.STRINGS, home.app.reg, home.appearance, need, true, name), "")
        })
    }
    function giveTreat() {
        if (!home.life) return
        home.app.say(home.app.s("react.treat"), "food")
        home.app.request("treats.give", { life_id: home.life.life_id }, function (resp) {
            if (!resp.ok) home.app.failed(resp)
        })
    }
    function wake() {
        if (home.controls.wake === "pet.pause") home.app.request("pet.pause", { paused: false }, function (r) { if (!r.ok) home.app.failed(r) })
        else home.app.request("pet.wake", {}, function (r) { if (!r.ok) home.app.failed(r) })
    }

    // The treat jar wiggles once when it is full and a new piece of work just finished.
    property int seenFinished: doc && doc.agent ? doc.agent.finished_seq : 0
    readonly property int finishedSeq: doc && doc.agent ? doc.agent.finished_seq : 0
    onFinishedSeqChanged: {
        if (home.finishedSeq > home.seenFinished && home.doc && home.doc.treats.full) jarWiggle.restart()
        home.seenFinished = home.finishedSeq
    }

    Column {
        id: column
        width: parent.width
        spacing: 0

        // ---- pet stage: 160 px, the bubble and Treats on the right
        Item {
            id: stage
            objectName: "cp.stage"
            width: parent.width
            height: home.t.u(160)

            PetPlayer {
                id: pet
                objectName: "cp.pet"
                x: home.off ? (parent.width - width) / 2 : home.t.u(48)
                width: home.t.u(160)
                height: width
                registry: home.app.reg
                species: home.appearance
                stage: Model.stageOf(home.life)
                petState: home.app.actionState !== "" ? home.app.actionState : Model.petStateFor(home.face, home.app.clockMs)
                clockMs: home.app.clockMs
                calm: home.app.calm
                runtimeBase: home.app.runtimeBase
                dim: home.off
            }
            Icon {
                id: confetti
                objectName: "cp.confetti"
                icon: "status/confetti"
                size: home.t.u(96)
                x: pet.x + home.t.u(32)
                y: 0
                visible: home.face && home.face.state === "celebrate"
                opacity: 0.9
                SequentialAnimation on scale {
                    running: confetti.visible && !home.app.calm
                    loops: 2
                    NumberAnimation { from: 0.6; to: 1.15; duration: 450; easing.type: Easing.OutBack }
                    NumberAnimation { to: 1; duration: 250 }
                }
            }
            SpeechBubble {
                id: speech
                objectName: "cp.speech"
                t: home.t
                text: home.app.bubbleText
                x: Math.max(0, pet.x - home.t.u(36))
                y: 0
                z: 2
            }
            ThoughtBubble {
                id: thought
                objectName: "cp.thought"
                t: home.t
                visible: !home.off && icons.length > 0
                icons: home.off ? [] : Model.bubbleIcons(home.app.reg, home.doc, home.face)
                big: home.face ? ["need_soon", "idle"].indexOf(home.face.state) < 0 : true
                x: pet.x + pet.width + home.t.u(8)
                y: 0
            }
            Rectangle {
                id: treats
                objectName: "cp.treats"
                readonly property bool cpControl: true
                readonly property string cpKind: "treats"
                visible: home.controls.treats
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                width: home.t.u(150)
                height: home.t.u(72)
                radius: home.t.radius
                color: treatsMouse.pressed ? home.t.tilePress : home.t.glowFill
                border.width: activeFocus ? Math.max(2, home.t.u(3)) : Math.max(1, home.t.u(2))
                border.color: activeFocus ? home.t.focusRing : home.t.accent
                activeFocusOnTab: true
                Accessible.role: Accessible.Button
                Accessible.name: home.app.s("btn.treats")
                Accessible.onPressAction: home.giveTreat()
                Keys.onReturnPressed: home.giveTreat()
                Keys.onSpacePressed: home.giveTreat()
                SequentialAnimation on rotation {
                    id: jarWiggle
                    running: false
                    NumberAnimation { to: -6; duration: 90 }
                    NumberAnimation { to: 6; duration: 120 }
                    NumberAnimation { to: -4; duration: 110 }
                    NumberAnimation { to: 0; duration: 90 }
                }
                Row {
                    anchors.centerIn: parent
                    spacing: home.t.u(6)
                    Icon {
                        anchors.verticalCenter: parent.verticalCenter
                        icon: "care-floor/treat_jar"
                        size: home.t.u(52)
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: home.app.s("btn.treats")
                        textFormat: Text.PlainText
                        font.family: home.t.family
                        font.pixelSize: home.t.fontTitle
                        font.bold: true
                        color: home.t.text
                    }
                }
                MouseArea {
                    id: treatsMouse
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: home.giveTreat()
                }
            }
        }

        // ---- the one sentence (shared priority)
        Text {
            id: line
            objectName: "cp.statusLine"
            readonly property bool cpStatusLine: true
            width: parent.width
            height: home.t.u(36)
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: Text.AlignHCenter
            text: home.doc && home.doc.treats.full && home.face && home.face.state === "idle"
                ? home.app.s("treats.full") : Model.faceLine(Strings.STRINGS, home.app.reg, home.doc, home.face)
            textFormat: Text.PlainText
            font.family: home.t.family
            font.pixelSize: home.t.fontHeading
            font.bold: true
            color: home.t.text
            // A long pair of app names in a wide monospace theme font steps down (at most to
            // Style.font.body) instead of wrapping (notes.md §7); the elide is the last resort.
            fontSizeMode: Text.HorizontalFit
            minimumPixelSize: home.t.fontBody
            elide: Text.ElideRight
        }
        Item { width: 1; height: home.t.u(8) }

        // ---- care tiles (two rows of two; the two that revive while fainted)
        Grid {
            objectName: "cp.tiles"
            visible: home.controls.tiles.length > 0
            columns: 2
            spacing: home.t.u(8)
            Repeater {
                model: home.controls.tiles
                CareTile {
                    required property var modelData
                    objectName: "cp.tile." + modelData.need
                    t: home.t
                    icon: Model.careIcon(home.app.reg, home.appearance, modelData.need)
                    text: home.app.s("tile." + modelData.need)
                    glow: modelData.glow
                    calm: home.app.calm
                    onTapped: home.care(modelData.need)
                }
            }
        }

        BigButton {
            objectName: "cp.wake"
            visible: home.controls.wake !== ""
            t: home.t
            width: parent.width
            height: home.t.u(72)
            icon: "care-floor/sun_wake"
            text: home.app.s("btn.wake")
            onTapped: home.wake()
        }
        BigButton {
            objectName: "cp.turnOn"
            visible: home.controls.turnOn
            t: home.t
            width: parent.width
            height: home.t.u(72)
            icon: "care-floor/power"
            text: home.app.s("btn.turn_on")
            onTapped: home.app.setPower(true)
        }

        Item { width: 1; height: home.t.u(8); visible: home.controls.round }

        // ---- round buttons
        Item {
            visible: home.controls.round
            width: parent.width
            height: roundRow.implicitHeight
            Row {
                id: roundRow
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: home.t.u(56)
                RoundButton {
                    objectName: "cp.round.change"
                    t: home.t
                    icon: "care-floor/wand_change"
                    text: home.app.s("round.change")
                    onTapped: home.app.go("change", {})
                }
                RoundButton {
                    objectName: "cp.round.helpers"
                    t: home.t
                    icon: "status/laptop"
                    text: home.app.s("round.helpers")
                    onTapped: home.app.go("helpers", {})
                }
                RoundButton {
                    objectName: "cp.round.more"
                    t: home.t
                    icon: "care-floor/gear_more"
                    text: home.app.s("round.more")
                    onTapped: home.app.go("more", {})
                }
            }
        }
    }
}
