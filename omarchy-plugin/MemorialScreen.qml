pragma ComponentBehavior: Bound
import QtQuick

// After a goodbye (or a Hardcore ending): the pet's picture in grey, "{name} has gone. Thank
// you for caring." and Meet a new pet (SPEC-v0.3 §2.4; popup_routing memorial).
Item {
    id: memorial
    required property var app
    readonly property var t: app.t
    readonly property var latest: app.ctl && app.ctl.snapshot && app.ctl.snapshot.memorial ? app.ctl.snapshot.memorial.latest : null
    readonly property string name: latest && latest.name ? latest.name
        : (app.doc && app.doc.memorial && app.doc.memorial.latest_name ? app.doc.memorial.latest_name : "")

    implicitWidth: t.u(448)
    implicitHeight: column.implicitHeight

    Column {
        id: column
        width: parent.width
        spacing: memorial.t.u(8)
        PetPlayer {
            anchors.horizontalCenter: parent.horizontalCenter
            width: memorial.t.u(160)
            height: width
            registry: memorial.app.reg
            species: memorial.latest ? String(memorial.latest.appearance || "") : ""
            stage: memorial.latest ? (Number(memorial.latest.final_stage) || 1) : 1
            petState: "sleep"
            calm: true
            runtimeBase: memorial.app.runtimeBase
            greyscale: true
        }
        Text {
            objectName: "cp.memorialLine"
            width: parent.width
            height: memorial.t.u(64)
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: Text.AlignHCenter
            text: memorial.app.sFor("memorial.line", "floor", { name: memorial.name })
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            maximumLineCount: 2
            font.family: memorial.t.family
            font.pixelSize: memorial.t.fontHeading
            font.bold: true
            color: memorial.t.text
        }
        BigButton {
            objectName: "cp.meetNew"
            t: memorial.t
            width: parent.width
            height: memorial.t.u(72)
            text: memorial.app.s("btn.meet_new")
            onTapped: { memorial.app.meetNew = true; memorial.app.home() }
        }
    }
}
