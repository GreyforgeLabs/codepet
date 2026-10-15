import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// One high-is-good meter. `hundredths` comes straight from the Rust snapshot
// (7000 = 70.0); this component only formats it.
ColumnLayout {
    id: root
    required property string label
    required property int hundredths
    readonly property string display: (Math.max(0, Math.min(10000, hundredths)) / 100).toFixed(1)
    spacing: 4
    RowLayout {
        Layout.fillWidth: true
        Label { text: root.label; textFormat: Text.PlainText; Layout.fillWidth: true }
        Label { text: root.display + " / 100"; textFormat: Text.PlainText }
    }
    ProgressBar {
        Layout.fillWidth: true
        from: 0
        to: 10000
        value: root.hundredths
        Accessible.name: root.label + ": " + root.display + " out of 100; higher is better"
    }
}
