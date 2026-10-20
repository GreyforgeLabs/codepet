pragma ComponentBehavior: Bound
import QtQuick

// The word bubble a tap shows ("Yum! Kibble!", "Ginger is full!").
// compact: the Home pet bubble. It stays short and narrow so it can sit under the
// face instead of covering the head.
Rectangle {
    id: speech
    required property var t
    property string text: ""
    property bool statistic: false
    property bool compact: false
    visible: text !== ""
    implicitWidth: Math.min(t.u(compact ? 148 : 260), label.implicitWidth + t.u(compact ? 16 : 28))
    implicitHeight: label.implicitHeight + t.u(compact ? 8 : 16)
    radius: compact ? t.smallRadius : t.radius
    color: t.accent
    Text {
        id: label
        readonly property bool cpStatistic: speech.statistic
        anchors.centerIn: parent
        width: Math.min(implicitWidth, speech.t.u(speech.compact ? 132 : 232))
        text: speech.text
        textFormat: Text.PlainText
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.WordWrap
        maximumLineCount: speech.compact ? 2 : 4
        font.family: speech.t.family
        font.pixelSize: speech.compact ? speech.t.fontBody : speech.t.fontTitle
        font.bold: !speech.compact
        color: speech.t.onAccent
    }
}
