import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    required property string label
    required property int value
    spacing: 4
    RowLayout {
        Layout.fillWidth: true
        Label { text: root.label; Layout.fillWidth: true }
        Label { text: root.value + " / 100" }
    }
    ProgressBar {
        Layout.fillWidth: true
        from: 0
        to: 100
        value: root.value
        Accessible.name: root.label + ": " + root.value + " out of 100; higher is better"
    }
}
