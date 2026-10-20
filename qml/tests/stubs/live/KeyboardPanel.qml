import QtQuick
import Quickshell
import qs.Commons as Commons

// Test-only stand-in for the Omarchy shell's qs.Ui KeyboardPanel, staged over the real one
// by qml/tests/test_bar_plugin.py: offscreen Quickshell has no layer-shell (PanelWindow)
// backend, so the harness hosts the popup in a plain window with the same API subset the
// CodePet popup uses (anchorItem, bar, owner, open, padding, margin, contentWidth/Height,
// fittedContentWidth/Height, borderSpec, screenH, focusTarget, close, default content).
FloatingWindow {
    id: root
    required property Item anchorItem
    required property QtObject bar
    property var owner: null
    property int margin: Commons.Style.gapsOut
    property int padding: Commons.Style.spacing.popupPadding
    property int contentWidth: Commons.Style.space(280)
    property int contentHeight: Commons.Style.space(200)
    property var borderSpec: Commons.Border.surfaceSpec("popups", "border", Commons.Color.popups.border, Math.max(1, Commons.Style.space(2)))
    property bool open: false
    property Item focusTarget: null
    readonly property real screenW: 1920
    readonly property real screenH: 1080
    readonly property real verticalContentInset: padding * 2 + Commons.Border.top(borderSpec) + Commons.Border.bottom(borderSpec)
    default property alias contentItem: holder.children

    function close() {
        if (owner && "close" in owner) owner.close()
        else root.open = false
    }
    function fittedContentWidth(width, cap) {
        var w = Math.max(1, Number(width) || 1)
        if (cap !== undefined && Number(cap) > 0) w = Math.min(w, Number(cap))
        return Math.round(w)
    }
    function fittedContentHeight(implicitHeight, cap) {
        var h = Math.max(verticalContentInset, (Number(implicitHeight) || 0) + verticalContentInset)
        if (cap !== undefined && Number(cap) > 0) h = Math.min(h, Number(cap))
        return Math.round(h)
    }

    visible: open
    implicitWidth: contentWidth
    implicitHeight: contentHeight
    color: Commons.Color.popups.background
    onOpenChanged: {
        if (!bar) return
        if (open) bar.requestPopout(owner || root)
        else if (bar.activePopout === (owner || root)) bar.releasePopout(owner || root)
    }
    Item {
        id: holder
        anchors.fill: parent
        anchors.margins: root.padding + Commons.Border.left(root.borderSpec)
    }
}
