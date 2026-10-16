pragma ComponentBehavior: Bound
import QtQuick
import "CodePetBar.js" as Model

// Memories (SPEC-v0.3 §3.6): the pet of today and the pets of before, in words and pictures,
// with times as words only ("today", "a while ago"). Past pets are drawn in grey.
Item {
    id: memories
    required property var app
    readonly property var t: app.t
    property var entries: []
    property bool loaded: false
    readonly property var life: app.life
    readonly property var snapLife: app.ctl && app.ctl.snapshot && app.ctl.snapshot.life ? app.ctl.snapshot.life : null
    readonly property real tz: -new Date(app.now).getTimezoneOffset()
    readonly property var items: {
        var out = []
        if (memories.life)
            out.push({ name: memories.life.name, appearance: memories.life.appearance, stage: Model.stageOf(memories.life),
                       key: "memories.born", at: memories.snapLife ? memories.snapLife.adopted_at_ms : null, past: false })
        for (var i = 0; i < memories.entries.length && out.length < 12; i++) {
            var e = memories.entries[i]
            out.push({ name: e.name, appearance: e.appearance, stage: e.final_stage,
                       key: e.end_cause === "retired" ? "memories.retired" : "memories.died", at: e.ended_at_ms, past: true })
        }
        return out
    }

    implicitWidth: t.u(448)
    implicitHeight: Math.min(t.u(536), Math.max(t.u(120), list.contentHeight))

    Component.onCompleted: memories.app.request("pet.memorial", {}, function (resp) {
        memories.loaded = true
        if (resp.ok && resp.result && Array.isArray(resp.result.entries)) memories.entries = resp.result.entries
    })

    Text {
        objectName: "cp.memoriesEmpty"
        visible: memories.items.length === 0
        anchors.centerIn: parent
        text: memories.app.s("memories.empty")
        textFormat: Text.PlainText
        font.family: memories.t.family
        font.pixelSize: memories.t.fontHeading
        font.bold: true
        color: memories.t.text
    }
    ListView {
        id: list
        anchors.fill: parent
        clip: true
        spacing: memories.t.u(6)
        model: memories.items
        boundsBehavior: Flickable.StopAtBounds
        delegate: Rectangle {
            id: entry
            required property var modelData
            width: list.width
            height: memories.t.u(72)
            radius: memories.t.smallRadius
            color: memories.t.tileFill
            PetPlayer {
                id: pic
                x: memories.t.u(8)
                anchors.verticalCenter: parent.verticalCenter
                width: memories.t.u(60)
                height: width
                registry: memories.app.reg
                species: String(entry.modelData.appearance || "")
                stage: Number(entry.modelData.stage) || 1
                petState: entry.modelData.past ? "sleep" : "idle"
                calm: true
                runtimeBase: memories.app.runtimeBase
                greyscale: entry.modelData.past === true
            }
            Column {
                anchors.left: pic.right
                anchors.leftMargin: memories.t.u(12)
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                Text {
                    width: parent.width
                    text: memories.app.sFor(entry.modelData.key, "floor", { name: entry.modelData.name })
                    textFormat: Text.PlainText
                    font.family: memories.t.family
                    font.pixelSize: memories.t.fontTitle
                    font.bold: true
                    color: memories.t.text
                    elide: Text.ElideRight
                }
                Text {
                    width: parent.width
                    visible: entry.modelData.at !== null && entry.modelData.at !== undefined
                    text: memories.app.s(Model.whenKey(entry.modelData.at, memories.app.now, memories.tz))
                    textFormat: Text.PlainText
                    font.family: memories.t.family
                    font.pixelSize: memories.t.fontBody
                    color: memories.t.dim
                }
            }
        }
    }
}
