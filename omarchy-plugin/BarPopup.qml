import QtQuick
import qs.Ui
import qs.Commons

// The CodePet popup card (SPEC-v0.3 §3): an Omarchy KeyboardPanel (the PopupCard API with
// keyboard focus on demand, so the name field and the typed-name confirmations can be
// typed into) holding PopupApp. It exists only while open (a Loader in BarWidget), so its
// animation clock costs nothing when closed. Width Style.space(360) (about 480 px at font
// base-size 16); height capped at Style.space(480) and 90% of the output.
KeyboardPanel {
    id: card

    required property var widget
    property string mode: "popup"
    property string startPage: ""
    readonly property Item app: content

    padding: Style.space(12)
    readonly property real sideInset: padding * 2 + Border.left(borderSpec) + Border.right(borderSpec)
    contentWidth: fittedContentWidth(content.implicitWidth + sideInset)
    contentHeight: fittedContentHeight(content.implicitHeight, Math.min(Style.space(480), Math.floor(screenH * 0.9)))

    // Opens a page of an already open popup ("" = Home).
    function show(page) {
        if (page === "") content.home()
        else content.go(page, {})
    }

    PopupApp {
        id: content
        objectName: "codepetPopup.app"
        anchors.top: parent.top
        anchors.horizontalCenter: parent.horizontalCenter
        width: implicitWidth
        ctl: card.widget
        mode: card.mode
        startPage: card.startPage
    }
}
