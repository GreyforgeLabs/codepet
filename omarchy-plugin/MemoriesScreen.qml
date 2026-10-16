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
    property var beforeSeq: null
    property var nextSeq: null
    readonly property real tz: -new Date(app.now).getTimezoneOffset()

    implicitWidth: t.u(448)
    implicitHeight: Math.min(t.u(536), Math.max(t.u(120), list.contentHeight + pager.height))

    function loadPage(cursor) {
        memories.app.request("pet.memories", { before_seq: cursor }, function (resp) {
            memories.loaded = true
            if (!resp.ok || !resp.result || !Array.isArray(resp.result.entries)) return
            memories.beforeSeq = cursor
            memories.nextSeq = resp.result.next_before_seq
            memories.entries = resp.result.entries
            list.positionViewAtBeginning()
        })
    }
    Component.onCompleted: loadPage(null)

    Text {
        objectName: "cp.memoriesEmpty"
        visible: memories.loaded && memories.entries.length === 0
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
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: pager.top
        clip: true
        spacing: memories.t.u(6)
        model: memories.entries
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
                    text: memories.app.sFor("memories." + entry.modelData.kind, "floor", { name: entry.modelData.name })
                    textFormat: Text.PlainText
                    font.family: memories.t.family
                    font.pixelSize: memories.t.fontTitle
                    font.bold: true
                    color: memories.t.text
                    elide: Text.ElideRight
                }
                Text {
                    width: parent.width
                    visible: entry.modelData.at_ms !== null && entry.modelData.at_ms !== undefined
                    text: memories.app.s(Model.whenKey(entry.modelData.at_ms, memories.app.now, memories.tz))
                    textFormat: Text.PlainText
                    font.family: memories.t.family
                    font.pixelSize: memories.t.fontBody
                    color: memories.t.dim
                }
            }
        }
    }
    Row {
        id: pager
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        spacing: memories.t.u(8)
        height: visible ? memories.t.u(48) : 0
        visible: memories.nextSeq !== null || memories.beforeSeq !== null
        BigButton {
            objectName: "cp.memoriesNewest"
            visible: memories.beforeSeq !== null
            width: (pager.width - pager.spacing) / 2
            t: memories.t
            text: memories.app.s("memories.newest")
            kind: "secondary"
            onTapped: memories.loadPage(null)
        }
        BigButton {
            objectName: "cp.memoriesOlder"
            visible: memories.nextSeq !== null
            width: (pager.width - pager.spacing) / 2
            t: memories.t
            text: memories.app.s("memories.older")
            kind: "secondary"
            onTapped: memories.loadPage(memories.nextSeq)
        }
    }
}
