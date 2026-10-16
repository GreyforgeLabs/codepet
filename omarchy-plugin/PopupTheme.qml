pragma ComponentBehavior: Bound
import QtQuick
import qs.Commons

// Sizes, fonts and colours of the CodePet popup, all from the Omarchy theme (Color, Style).
// SPEC-v0.3 §3 states every size in px at font base-size 16; u(px) turns that into
// Style.space() units, so every budget holds at base sizes 12, 16 and 20 (K13).
QtObject {
    id: theme

    // px at base-size 16 -> this shell's px.
    function u(px) { return Style.space(px * 0.75) }
    // The inverse, for tests: this shell's px -> px at base-size 16.
    function px16(v) { return v / Math.max(0.01, Style.effectiveSpacingScale * 0.75) }

    readonly property real scale: Style.effectiveSpacingScale * 0.75
    // Style.font and Color.popups are untyped QtObjects to tooling; read them as var.
    readonly property var styleFont: Style.font
    readonly property var popupColors: Color.popups
    readonly property string family: styleFont.family
    readonly property int fontBody: styleFont.body
    readonly property int fontTitle: styleFont.title
    readonly property int fontHeading: styleFont.heading
    readonly property int fontDisplay: styleFont.display
    readonly property int radius: Math.max(Style.cornerRadius, Style.space(14))
    readonly property int smallRadius: Math.max(Style.cornerRadius, Style.space(10))

    readonly property color text: popupColors.text
    readonly property color background: popupColors.background
    readonly property color accent: Color.accent
    readonly property color urgent: Color.urgent
    readonly property color muted: Color.muted

    // Soft fills derived from the theme's own text and accent colours.
    function mix(c, a) { return Qt.rgba(c.r, c.g, c.b, a) }
    readonly property color tileFill: mix(text, 0.07)
    readonly property color tileHover: mix(text, 0.12)
    readonly property color tilePress: mix(text, 0.18)
    readonly property color tileBorder: mix(text, 0.18)
    readonly property color glowFill: mix(accent, 0.22)
    readonly property color glowBorder: accent
    readonly property color dim: mix(text, 0.55)
    readonly property color onAccent: background
    readonly property color focusRing: accent
}
