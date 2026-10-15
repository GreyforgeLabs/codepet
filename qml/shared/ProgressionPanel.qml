pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "LifeText.js" as LifeText

// Companion progress (docs/care-progression.md). The top block is the living companion's own XP,
// level and next growth stage ("Ginger · Level 2 · Desk Prowler"); evolution comes only from
// accepted mock tasks. The "All companions (lifetime)" block keeps the existing account-wide
// demo/real mastery view unchanged. Levels never change access, providers or approvals.
GroupBox {
    id: root
    required property var progression
    property var life: null
    property var registry: null
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

        // ---------------------------------------------------------- this companion
        ColumnLayout {
            objectName: "progression.life"
            Layout.fillWidth: true
            visible: root.life !== null
            spacing: 6
            Label {
                objectName: "progression.headline"
                Layout.fillWidth: true
                text: root.life ? LifeText.lifeHeadline(root.life, root.registry)
                    + (root.life.pace === "playtest" ? " · Playtest" : "") : ""
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                font.bold: true
            }
            ProgressBar {
                objectName: "progression.lifeXp"
                Layout.fillWidth: true
                from: 0
                to: root.life && root.life.next_level_xp ? root.life.next_level_xp : Math.max(1, root.life ? root.life.xp : 1)
                value: root.life ? root.life.xp : 0
                Accessible.name: root.life ? (root.life.next_level_xp
                    ? root.life.xp + " of " + root.life.next_level_xp + " XP toward level " + (root.life.level + 1)
                    : root.life.xp + " XP, highest level") : "XP"
            }
            Label {
                Layout.fillWidth: true
                text: root.life ? root.life.xp + " XP"
                    + (root.life.next_level_xp ? " · next level at " + root.life.next_level_xp + " XP" : "")
                    + " · " + (root.life.tasks_accepted || 0) + " accepted tasks with " + LifeText.cleanName(root.life.name) : ""
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }
            Label {
                objectName: "progression.nextStage"
                Layout.fillWidth: true
                text: LifeText.nextStageText(root.life, root.registry)
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }
            Label {
                Layout.fillWidth: true
                text: "Growth comes only from accepted tasks. Care and time never add XP, and a new companion always starts at level 1."
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }
        }

        // ---------------------------------------------------------- account lifetime
        Label {
            Layout.fillWidth: true
            text: "All companions (lifetime)"
            textFormat: Text.PlainText
            font.bold: true
        }
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
