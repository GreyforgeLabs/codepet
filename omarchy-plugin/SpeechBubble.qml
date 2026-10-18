pragma ComponentBehavior: Bound
import QtQuick

// The word bubble a tap shows next to the pet ("Yum! Kibble!", "Ginger is full!").
Rectangle {
    id: speech
    required property var t
    property string text: ""
    property bool statistic: false
    visible: text !== ""
    implicitWidth: Math.min(t.u(260), label.implicitWidth + t.u(28))
    implicitHeight: label.implicitHeight + t.u(16)
    radius: t.radius
    color: t.accent
    Text {
        id: label
        readonly property bool cpStatistic: speech.statistic
        anchors.centerIn: parent
        width: Math.min(implicitWidth, speech.t.u(232))
        text: speech.text
        textFormat: Text.PlainText
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.WordWrap
        font.family: speech.t.family
        font.pixelSize: speech.t.fontTitle
        font.bold: true
        color: speech.t.onAccent
    }
}
