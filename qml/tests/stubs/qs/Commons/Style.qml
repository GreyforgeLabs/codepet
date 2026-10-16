pragma Singleton
import QtQuick

// Test-only stand-in for the Omarchy shell's qs.Commons Style (/usr/share/omarchy/shell),
// for PySide offscreen smokes that cannot load Quickshell. It keeps exactly the members the
// CodePet popup uses, with the real formulas; test_bar_plugin.py checks every member the
// plugin reads exists in the real Style when the Omarchy sources are installed.
QtObject {
    id: root
    property int cornerRadius: 0
    property int gapsOut: 5
    property int fontBaseSize: 12
    property real spacingScale: 1.0
    property bool spacingScaleWithFont: true
    readonly property real fontScale: Math.max(1 / 12, fontBaseSize / 12)
    readonly property real effectiveSpacingScale: spacingScale * (spacingScaleWithFont ? fontScale : 1)

    function spaceReal(px) {
        var n = Number(px)
        if (!isFinite(n) || n <= 0) return 0
        return n * effectiveSpacingScale
    }
    function space(px) {
        var n = spaceReal(px)
        if (n <= 0) return 0
        return Math.max(1, Math.round(n))
    }
    function fontPx(mult) { return Math.max(1, Math.round(fontBaseSize * mult)) }

    readonly property QtObject font: QtObject {
        readonly property string family: "monospace"
        readonly property int baseSize: root.fontBaseSize
        readonly property int caption: root.fontPx(0.833)
        readonly property int bodySmall: root.fontPx(0.917)
        readonly property int body: root.fontPx(1.0)
        readonly property int subtitle: root.fontPx(1.083)
        readonly property int title: root.fontPx(1.167)
        readonly property int heading: root.fontPx(1.333)
        readonly property int display: root.fontPx(2.0)
        readonly property int displayLarge: root.fontPx(2.333)
    }
    readonly property QtObject spacing: QtObject {
        readonly property int popupPadding: root.space(14)
    }
}
