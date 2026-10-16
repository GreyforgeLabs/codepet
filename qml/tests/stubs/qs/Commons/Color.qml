pragma Singleton
import QtQuick

// Test-only stand-in for the Omarchy shell's qs.Commons Color (see Style.qml here).
QtObject {
    id: root
    property color foreground: "#cacccc"
    property color background: "#101315"
    property color accent: "#7aa2f7"
    property color urgent: "#a55555"
    property color muted: "#707880"
    readonly property QtObject popups: QtObject {
        property color background: root.background
        property color text: root.foreground
        property color border: root.accent
    }
}
