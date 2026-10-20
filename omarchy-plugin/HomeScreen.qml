pragma ComponentBehavior: Bound
import QtQuick
import "CodePetBar.js" as Model
import "Strings.js" as Strings

// Home (SPEC-v0.3 §3.2) and its Off look (§3.7). A big animated pet, exact level/XP,
// evolution progress and Treats, one sentence, four care meters and three round
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
    readonly property int jarCount: doc && doc.treats ? doc.treats.jar : 0
    readonly property real growthSeq: face && face.celebrate && face.celebrate.kind === "grew" ? face.celebrate.seq : 0
    readonly property string growthKey: (life ? life.life_id : "") + ":" + growthSeq
    property string shownGrowthLife: ""
    property real shownGrowthSeq: 0
    property bool growthReady: false
    onGrowthKeyChanged: showNewGrowth()
    onOffChanged: if (off) growthFlash.stop()
    onVisibleChanged: if (!visible) growthFlash.stop()
    function showNewGrowth() {
        if (!growthReady) return
        var id = life ? life.life_id : ""
        if (id !== shownGrowthLife) { shownGrowthLife = id; shownGrowthSeq = growthSeq; return }
        if (growthSeq <= shownGrowthSeq) return
        shownGrowthSeq = growthSeq
        if (visible && !off && !app.calm && app.screen === "home" && app.ctl.popupVisible)
            growthFlash.restart()
    }
    Timer { id: growthFlash; objectName: "codepetPopup.growthAccent"; interval: 1600; repeat: false }
    property bool carePending: false
    property var lifetime: ({ alive: true })
    Component.onDestruction: home.lifetime.alive = false

    implicitWidth: t.u(448)
    implicitHeight: column.implicitHeight

    // ------------------------------------------------------------- taps (K5)

    function care(need) {
        if (home.carePending) return
        var name = home.life ? home.life.name : ""
        var lifetime = home.lifetime
        home.carePending = true
        home.app.say(home.app.s("care.wait"), "")
        home.app.request("pet.care", { action: need }, function (resp) {
            if (!lifetime.alive) return
            home.carePending = false
            if (!resp.ok) { home.app.failed(resp); return }
            var changed = !!(resp.result && resp.result.changed === true)
            var anim = (changed || need === "play") ? need : ""
            home.app.say(Model.reactText(Strings.STRINGS, home.app.reg, home.appearance, need, !changed, name), anim, changed ? need : "voice")
        })
    }
    function giveTreat() {
        if (!home.life || home.carePending) return
        if (home.jarCount <= 0) { home.app.say(home.app.s("care.jar_empty"), ""); return }
        home.carePending = true
        var lifetime = home.lifetime
        home.app.say(home.app.s("care.wait"), "")
        home.app.request("treats.give", { life_id: home.life.life_id }, function (resp) {
            if (!lifetime.alive) return
            home.carePending = false
            if (!resp.ok) {
                if (resp.error && resp.error.code === "NO_TREATS") home.app.say(home.app.s("care.jar_empty"), "")
                else home.app.failed(resp)
                return
            }
            home.app.say(home.app.s("stats.treat_given"), "food", "treat")
        })
    }
    function wake() {
        if (home.controls.wake === "pet.pause") home.app.request("pet.pause", { paused: false }, function (r) { if (!r.ok) home.app.failed(r) })
        else home.app.request("pet.wake", {}, function (r) { if (!r.ok) home.app.failed(r) })
    }
    function petAffection() {
        if (!home.controls.round) return
        home.app.say(home.app.s("care.petted"), "pet")
    }

    // The treat jar wiggles once when it is full and a new piece of work just finished: when
    // the turn finishes while Home is open, or when Home opens on that turn's unseen
    // celebration ("The treat jar is full!", act.done_full). `seenFinished` is a plain
    // value, set once here and then by the handler, never a binding (a binding could update
    // before the handler runs and hide the change).
    property int seenFinished: 0
    readonly property bool jarWiggling: jarWiggle.running
    readonly property int finishedSeq: doc && doc.agent ? doc.agent.finished_seq : 0
    readonly property bool jarFull: !!(doc && doc.treats && doc.treats.full)
    onFinishedSeqChanged: {
        if (home.finishedSeq > home.seenFinished && home.jarFull) jarWiggle.restart()
        home.seenFinished = home.finishedSeq
    }
    Component.onCompleted: {
        // Opening an existing growth result shows its text without replaying the accent.
        home.shownGrowthLife = home.life ? home.life.life_id : ""
        home.shownGrowthSeq = home.growthSeq
        home.growthReady = true
        home.seenFinished = home.finishedSeq
        if (home.jarFull && home.face && home.face.line === "act.done_full") jarWiggle.restart()
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
                x: home.t.u(48)
                width: home.t.u(160)
                height: width
                registry: home.app.reg
                species: home.appearance
                stage: Model.stageOf(home.life)
                petState: home.app.actionState !== "" ? home.app.actionState : Model.petStateFor(home.face, home.app.clockMs)
                clockMs: home.app.clockMs
                replay: home.app.actionReplay
                affectionate: home.app.actionState === "pet"
                calm: home.app.calm
                runtimeBase: home.app.runtimeBase
                dim: home.off
                activeFocusOnTab: home.controls.round
                Accessible.role: Accessible.Button
                Accessible.name: home.app.s("care.pet_action")
                Accessible.onPressAction: home.petAffection()
                Keys.onReturnPressed: home.petAffection()
                Keys.onSpacePressed: home.petAffection()
                MouseArea { anchors.fill: parent; enabled: home.controls.round; onClicked: home.petAffection() }
            }
            Icon {
                id: confetti
                objectName: "cp.confetti"
                icon: "status/confetti"
                size: home.t.u(24)
                x: pet.x - width - home.t.u(8)
                y: home.t.u(68)
                visible: growthFlash.running && !home.app.calm && !home.off
                opacity: 0.9
            }
            SpeechBubble {
                id: speech
                objectName: "cp.speech"
                t: home.t
                compact: true
                text: home.app.bubbleText
                statistic: home.app.bubbleText === home.app.s("stats.treat_given")
                // Under the face. The sprite's head sits in the top of the 160 px frame.
                x: pet.x
                y: Math.max(0, stage.height - height - home.t.u(4))
                z: 2
            }
            PetProgress {
                objectName: "cp.progress"
                app: home.app
                anchors.right: parent.right
                width: home.t.u(216)
                y: 0
                visible: !!home.life
            }
            Rectangle {
                id: treats
                objectName: "cp.treats"
                readonly property bool cpControl: true
                readonly property string cpKind: "treats"
                visible: home.controls.treats
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                width: home.t.u(192)
                height: home.t.u(72)
                radius: home.t.radius
                color: treatsMouse.pressed ? home.t.tilePress : home.t.glowFill
                border.width: activeFocus ? Math.max(2, home.t.u(3)) : Math.max(1, home.t.u(2))
                border.color: activeFocus ? home.t.focusRing : home.t.accent
                activeFocusOnTab: true
                Accessible.role: Accessible.Button
                Accessible.name: home.app.s("stats.treats", { count: home.jarCount })
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
                        size: home.t.u(40)
                    }
                    Text {
                        objectName: "cp.treats.count"
                        readonly property bool cpStatistic: true
                        anchors.verticalCenter: parent.verticalCenter
                        text: home.app.s("stats.treats", { count: home.jarCount })
                        textFormat: Text.PlainText
                        font.family: home.t.family
                        font.pixelSize: home.t.fontBody
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
            readonly property bool cpStatistic: true
            width: parent.width
            height: home.t.u(36)
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: Text.AlignHCenter
            text: home.face && ["off", "asleep", "paused", "fainted", "need_now", "need_soon", "celebrate"].indexOf(home.face.state) >= 0
                ? home.app.faceLine()
                : home.app.workState && home.app.workState.task && ["preparing", "running", "review", "applying"].indexOf(home.app.workState.task.status) >= 0
                ? home.app.s(home.app.workState.task.status === "review" ? "work.review" : "work.working")
                : home.jarFull && home.face && home.face.state === "idle"
                ? home.app.s("treats.full") : Model.faceLine(Strings.STRINGS, home.app.reg, home.doc, home.face)
            textFormat: Text.PlainText
            font.family: home.t.family
            font.pixelSize: home.t.fontHeading
            font.bold: true
            color: home.t.text
            elide: Text.ElideRight
            function recordPresented() {
                if (visible && home.app.screen === "home" && home.app.ctl.popupVisible)
                    home.app.recordPresented(home.doc, home.face)
            }
            onTextChanged: Qt.callLater(recordPresented)
            Component.onCompleted: Qt.callLater(recordPresented)
        }
        Text {
            objectName: "cp.energyLine"
            readonly property bool cpStatistic: true
            visible: !!(home.app.workState && home.app.workState.readiness) && !home.off
            width: parent.width
            height: visible ? home.t.u(18) : 0
            text: visible ? home.app.s("work.energy", { value: home.app.workState.readiness.energy }) + " · "
                + home.app.s(home.app.workState.readiness.ready ? "work.ready" : "work.rest") : ""
            textFormat: Text.PlainText
            horizontalAlignment: Text.AlignHCenter
            font.family: home.t.family
            font.pixelSize: home.t.fontBody
            color: home.t.dim
            elide: Text.ElideRight
        }
        Item { width: 1; height: home.app.workState && home.app.workState.readiness && !home.off ? 0 : home.t.u(8) }

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
                    text: home.app.s(modelData.need === "food" ? "care.snack" : modelData.need === "water" ? "care.drink" : "tile." + modelData.need)
                    detail: ""
                    glow: modelData.glow
                    calm: home.app.calm
                    meter: Model.careMeter(home.doc, modelData.need)
                    meterText: meter.known ? home.app.s("stats.percent", { value: meter.percent }) : home.app.s("stats.unknown")
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
        BigButton {
            objectName: "cp.workRecovery"
            visible: !home.off && !!home.life && !home.controls.round
            t: home.t
            width: parent.width
            text: home.app.s("work.open")
            icon: "status/laptop"
            kind: "secondary"
            onTapped: home.app.openWork("compact")
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
                    objectName: "cp.round.work"
                    t: home.t
                    icon: "status/laptop"
                    text: home.app.s("work.open")
                    onTapped: home.app.openWork("compact")
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
