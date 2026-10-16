pragma ComponentBehavior: Bound
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
    readonly property var views: ["home", "care", "companion", "life"]

    PreviewHost {
        id: previewHost
        onCompanionRequested: panel.openCompanion()
        onLifeRequested: panel.openLife()
    }

    header: ToolBar {
        implicitHeight: headerLayout.implicitHeight + 16
        ColumnLayout {
            id: headerLayout
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
                        if (window.views.indexOf(panel.view) < 0)
                            panel.view = "home"
                        panel.refresh()
                    }
                }
            }
            // Home · Care · Companion · Life (the v0.2 window views; the v0.3 UI is the bar popup).
            TabBar {
                id: viewTabs
                objectName: "viewTabs"
                Layout.fillWidth: true
                currentIndex: Math.max(0, window.views.indexOf(panel.view))
                Repeater {
                    model: ["Home", "Care", "Companion", "Life"]
                    TabButton {
                        required property string modelData
                        required property int index
                        text: modelData
                        Accessible.name: modelData + " tab"
                        onClicked: panel.view = window.views[index]
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
