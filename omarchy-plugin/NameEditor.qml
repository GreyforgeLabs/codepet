pragma ComponentBehavior: Bound
import QtQuick

// The rename field (SPEC-v0.3 §3.2): a big text field, a big Done and a Cancel. It replaces
// the screen body in place while open; the header's ‹ Back also cancels.
Item {
    id: editor
    required property var app
    readonly property var t: app.t

    implicitWidth: t.u(448)
    implicitHeight: column.implicitHeight
    Component.onCompleted: Qt.callLater(function () { field.forceActiveFocus(); field.selectAll() })

    Column {
        id: column
        width: parent.width
        spacing: editor.t.u(12)

        Item { width: 1; height: editor.t.u(24) }
        Rectangle {
            id: box
            objectName: "cp.nameField"
            readonly property bool cpControl: true
            readonly property string cpKind: "control"
            width: parent.width
            height: editor.t.u(64)
            radius: editor.t.radius
            color: editor.t.tileFill
            border.width: Math.max(2, editor.t.u(3))
            border.color: field.activeFocus ? editor.t.accent : editor.t.tileBorder
            TextInput {
                id: field
                objectName: "cp.nameInput"
                anchors.fill: parent
                anchors.leftMargin: editor.t.u(18)
                anchors.rightMargin: editor.t.u(18)
                verticalAlignment: TextInput.AlignVCenter
                text: editor.app.editValue
                font.family: editor.t.family
                font.pixelSize: editor.t.fontDisplay
                font.bold: true
                color: editor.t.text
                selectionColor: editor.t.accent
                selectedTextColor: editor.t.onAccent
                maximumLength: 64
                clip: true
                onAccepted: editor.app.finishEdit(text)
                Keys.onEscapePressed: editor.app.editing = false
            }
            Text {
                anchors.fill: field
                verticalAlignment: Text.AlignVCenter
                visible: field.text === ""
                text: editor.app.s("name.type")
                textFormat: Text.PlainText
                font.family: editor.t.family
                font.pixelSize: editor.t.fontDisplay
                color: editor.t.dim
            }
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.IBeamCursor
                onClicked: field.forceActiveFocus()
            }
        }
        Text {
            objectName: "cp.nameError"
            width: parent.width
            visible: editor.app.editError !== ""
            text: editor.app.editError
            textFormat: Text.PlainText
            horizontalAlignment: Text.AlignHCenter
            font.family: editor.t.family
            font.pixelSize: editor.t.fontHeading
            font.bold: true
            color: editor.t.urgent
        }
        BigButton {
            objectName: "cp.nameDone"
            t: editor.t
            width: parent.width
            text: editor.app.s("btn.done")
            onTapped: editor.app.finishEdit(field.text)
        }
        BigButton {
            objectName: "cp.nameCancel"
            t: editor.t
            kind: "secondary"
            width: parent.width
            text: editor.app.s("btn.cancel")
            onTapped: editor.app.editing = false
        }
    }
}
