pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

GroupBox {
    id: root
    required property var progression
    readonly property var demo: progression && progression.demo ? progression.demo : null
    readonly property var real: progression && progression.real ? progression.real : null
    property int lastDemoLevel: 0
    property string levelNotice: ""
    onDemoChanged: {
        if (root.demo) {
            if (root.lastDemoLevel > 0 && root.demo.level > root.lastDemoLevel)
                root.levelNotice = "Demo level " + root.demo.level + " reached · " + root.levelName(root.demo.level)
                    + ". A new guided template preview is available; execution permissions are unchanged."
            root.lastDemoLevel = root.demo.level
        }
    }
    title: "Skills · simulation progress"
    function levelName(level) {
        return ["", "Hatchling", "Helper", "Problem Solver", "Craftspet", "Specialist"][Math.min(5, Math.max(1, level))]
    }
    function rank(xp) {
        return xp >= 400 ? "III" : xp >= 200 ? "II" : xp >= 80 ? "I" : "Learning"
    }
    ColumnLayout {
        anchors.fill: parent
        spacing: 8
        Label {
            Layout.fillWidth: true
            text: root.demo ? "DEMO ONLY · Level " + root.demo.level + " · " + root.levelName(root.demo.level)
                + " · " + root.demo.lifetime_xp + " XP" : "Refresh to load saved progression"
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            font.bold: true
        }
        Label {
            Layout.fillWidth: true
            visible: root.levelNotice !== ""
            text: root.levelNotice
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            font.bold: true
            Accessible.name: text
        }
        ProgressBar {
            Layout.fillWidth: true
            visible: root.demo !== null
            from: 0
            to: root.demo && root.demo.next_level_xp ? root.demo.next_level_xp : Math.max(1, root.demo ? root.demo.lifetime_xp : 1)
            value: root.demo ? root.demo.lifetime_xp : 0
            Accessible.name: root.demo ? root.demo.lifetime_xp + " lifetime demo XP" : "Demo XP"
        }
        Label {
            Layout.fillWidth: true
            text: root.demo && root.demo.next_level_xp ? "Next level at " + root.demo.next_level_xp + " total demo XP" : "Highest current demo level"
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
        }
        Repeater {
            model: [ {key: "bugfix", label: "Bugfix"}, {key: "quality_performance", label: "Quality & Performance"},
                {key: "learning_specification", label: "Learning & Specification"}, {key: "cleanup", label: "Cleanup"} ]
            delegate: Label {
                required property var modelData
                readonly property int xp: root.demo && root.demo.mastery ? root.demo.mastery[modelData.key] || 0 : 0
                Layout.fillWidth: true
                text: modelData.label + " · " + root.rank(xp) + " · " + xp + " demo XP"
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }
        }
        Label {
            Layout.fillWidth: true
            text: "Real-work XP: " + (root.real ? root.real.lifetime_xp : 0)
                + ". Mock acceptance never earns real-work progression. Care and sleep grant no XP. Levels never change access, providers or approvals."
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
        }
        Label {
            Layout.fillWidth: true
            text: "Guided templates: a small bounded simulation is available now. Focused test (L2), reproduce-and-fix (L3), bounded multi-file plan (L4), and scoped checkpoints (L5) are previews only. Real execution and higher reward tiers remain unavailable."
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
        }
    }
}
