pragma ComponentBehavior: Bound
import QtQuick
import "CodePetBar.js" as Model
import "PetAge.js" as PetAge

// Exact saved level/XP, with progress between evolution thresholds (not level thresholds).
Item {
    id: progress
    required property var app
    readonly property var t: app.t
    readonly property var stats: Model.petProgress(app.life)
    property real ageClock: 0
    readonly property var age: PetAge.age(app.life, Math.max(app.now || 0, ageClock))
    readonly property string bornText: age.known ? app.s("stats.age_since", { date: Qt.formatDateTime(new Date(age.born), "MMM d, yyyy · h:mm AP") }) : ""
    implicitHeight: t.u(80)
    Timer {
        objectName: "codepetPopup.age"
        interval: 60000
        repeat: true
        running: progress.visible && progress.app.isOn && progress.app.ctl && progress.app.ctl.popupVisible === true
        onTriggered: progress.ageClock = Date.now()
    }

    Column {
        width: parent.width
        spacing: progress.t.u(3)
        Item {
            width: parent.width
            height: level.implicitHeight
            Text {
                id: level
                objectName: "cp.progress.level"
                readonly property bool cpStatistic: true
                text: progress.stats.known ? progress.app.s("stats.level", { level: progress.stats.level }) : progress.app.s("stats.unknown")
                textFormat: Text.PlainText
                font.family: progress.t.family
                font.pixelSize: progress.t.fontTitle
                font.bold: true
                color: progress.t.text
            }
            Text {
                objectName: "cp.progress.xp"
                readonly property bool cpStatistic: true
                anchors.right: parent.right
                anchors.baseline: level.baseline
                text: progress.stats.known ? progress.app.s("stats.xp", { xp: progress.stats.xp }) : progress.app.s("stats.unknown")
                textFormat: Text.PlainText
                font.family: progress.t.family
                font.pixelSize: progress.t.fontBody
                color: progress.t.text
            }
        }
        Rectangle {
            objectName: "cp.progress.track"
            width: parent.width
            height: progress.t.u(6)
            radius: height / 2
            color: progress.t.tileBorder
            Rectangle {
                objectName: "cp.progress.fill"
                width: parent.width * progress.stats.fraction
                height: parent.height
                radius: height / 2
                color: progress.t.accent
            }
        }
        Text {
            objectName: "cp.progress.next"
            readonly property bool cpStatistic: true
            width: parent.width
            text: !progress.stats.known ? progress.app.s("stats.unknown")
                : progress.stats.evolved ? progress.app.s("stats.evolved")
                : progress.stats.remaining !== null ? progress.app.s("stats.evolve", { xp: progress.stats.remaining })
                : progress.app.s("stats.unknown")
            textFormat: Text.PlainText
            font.family: progress.t.family
            font.pixelSize: progress.t.fontBody
            color: progress.t.text
        }
        Text {
            id: ageLine
            objectName: "cp.progress.age"
            readonly property bool cpStatistic: true
            width: parent.width
            text: !progress.age.known ? progress.app.s("stats.age_unknown")
                : progress.age.value === 0 ? progress.app.s("stats.age_new")
                : progress.app.s("stats.age_" + progress.age.unit, { value: progress.age.value })
            textFormat: Text.PlainText
            font.family: progress.t.family
            font.pixelSize: progress.t.fontBody
            color: progress.t.dim
            Accessible.description: progress.bornText
            HoverHandler { id: ageHover }
        }
    }
    Rectangle {
        visible: ageHover.hovered && progress.age.known
        anchors.right: parent.right
        y: ageLine.y + ageLine.height + progress.t.u(6)
        z: 20
        width: Math.max(parent.width, progress.t.u(270))
        height: born.implicitHeight + progress.t.u(16)
        radius: progress.t.smallRadius
        color: progress.t.background
        border.color: progress.t.tileBorder
        Text {
            id: born
            objectName: "cp.progress.birth"
            readonly property bool cpStatistic: true
            anchors.fill: parent
            anchors.margins: progress.t.u(8)
            text: progress.bornText
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            font.family: progress.t.family
            font.pixelSize: progress.t.fontBody
            color: progress.t.text
        }
    }
}
