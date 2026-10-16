pragma ComponentBehavior: Bound
import QtQuick
import "CodePetBar.js" as Model

// Say goodbye (SPEC-v0.3 §3.6, typed-name confirmation kept from v0.2): the pet, the
// question, the pet's name typed in, then Say goodbye in Color.urgent, or Keep.
Item {
    id: bye
    required property var app
    readonly property var t: app.t
    readonly property var life: app.life
    readonly property bool matches: !!(life && Model.cleanName(field.text) === life.name)

    implicitWidth: t.u(448)
    implicitHeight: column.implicitHeight
    Component.onCompleted: Qt.callLater(function () { field.focusField() })

    function goodbye() {
        if (!bye.life || !bye.matches) return
        bye.app.request("pet.retire", { life_id: bye.life.life_id, confirm_name: Model.cleanName(field.text) }, function (resp) {
            if (resp.ok) bye.app.home()
            else bye.app.failed(resp)
        })
    }

    Column {
        id: column
        width: parent.width
        spacing: bye.t.u(8)
        PetPlayer {
            anchors.horizontalCenter: parent.horizontalCenter
            width: bye.t.u(160)
            height: width
            registry: bye.app.reg
            species: bye.life ? bye.life.appearance : ""
            stage: Model.stageOf(bye.life)
            petState: "idle"
            clockMs: bye.app.clockMs
            calm: bye.app.calm
            runtimeBase: bye.app.runtimeBase
        }
        Text {
            width: parent.width
            height: bye.t.u(36)
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: Text.AlignHCenter
            text: bye.app.s("goodbye.ask")
            textFormat: Text.PlainText
            font.family: bye.t.family
            font.pixelSize: bye.t.fontHeading
            font.bold: true
            color: bye.t.text
            elide: Text.ElideRight
        }
        TypedField {
            id: field
            objectName: "cp.goodbyeName"
            t: bye.t
            width: parent.width
            placeholder: bye.app.s("goodbye.type")
            onAccepted: bye.goodbye()
        }
        BigButton {
            objectName: "cp.goodbye"
            visible: bye.matches
            t: bye.t
            kind: "danger"
            width: parent.width
            icon: "status/wave_goodbye"
            text: bye.app.s("btn.goodbye")
            onTapped: bye.goodbye()
        }
        BigButton {
            objectName: "cp.goodbyeKeep"
            t: bye.t
            kind: "secondary"
            width: parent.width
            text: bye.app.s("btn.keep")
            onTapped: bye.app.back()
        }
    }
}
