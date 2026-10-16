pragma ComponentBehavior: Bound
import QtQuick

// A big text field with a hint inside (the typed-name confirmations of Say goodbye and
// Hardcore). Plain text only.
Rectangle {
    id: box
    required property var t
    property string placeholder: ""
    property alias text: input.text
    readonly property bool cpControl: true
    readonly property string cpKind: "control"
    signal accepted

    function focusField() { input.forceActiveFocus() }

    implicitWidth: t.u(448)
    implicitHeight: t.u(56)
    radius: t.radius
    color: t.tileFill
    border.width: Math.max(2, t.u(3))
    border.color: input.activeFocus ? t.accent : t.tileBorder
    TextInput {
        id: input
        anchors.fill: parent
        anchors.leftMargin: box.t.u(16)
        anchors.rightMargin: box.t.u(16)
        verticalAlignment: TextInput.AlignVCenter
        font.family: box.t.family
        font.pixelSize: box.t.fontHeading
        font.bold: true
        color: box.t.text
        selectionColor: box.t.accent
        selectedTextColor: box.t.onAccent
        maximumLength: 64
        clip: true
        onAccepted: box.accepted()
    }
    Text {
        anchors.fill: input
        verticalAlignment: Text.AlignVCenter
        visible: input.text === ""
        text: box.placeholder
        textFormat: Text.PlainText
        font.family: box.t.family
        font.pixelSize: box.t.fontTitle
        color: box.t.dim
        elide: Text.ElideRight
    }
    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.IBeamCursor
        onClicked: input.forceActiveFocus()
    }
}
