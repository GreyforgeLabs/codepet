import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../shared"

// Standalone preview window. Uses PreviewHost, which is not connected to Rust.
// The real standalone host is a later minimal Qt C++ launcher (QProcess transport).
ApplicationWindow {
    id: window
    width: 720
    height: 900
    minimumWidth: 480
    minimumHeight: 480
    visible: true
    title: "CodePet (preview, not connected)"

    PreviewHost { id: previewHost }

    header: ToolBar {
        ColumnLayout {
            anchors.fill: parent
            anchors.margins: 8
            Label {
                Layout.fillWidth: true
                text: "PREVIEW – not connected to Rust; no rewards are stored"
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                font.bold: true
                Accessible.name: text
            }
            RowLayout {
                Label { text: "Fixture" }
                ComboBox {
                    id: fixtureBox
                    objectName: "fixtureBox"
                    model: previewHost.fixtureNames
                    Accessible.name: "Preview fixture"
                    onActivated: {
                        previewHost.fixture = currentText
                        panel.view = "home"
                        panel.refresh()
                    }
                }
            }
        }
    }

    ScrollView {
        anchors.fill: parent
        contentWidth: availableWidth
        clip: true
        Panel {
            id: panel
            objectName: "panel"
            x: 16
            width: parent.width - 32
            host: previewHost
        }
    }

    // Opening the window counts as opening the panel: load the (fixture) snapshot.
    Component.onCompleted: panel.refresh()
}
