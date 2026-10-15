import QtQuick
import Quickshell.Io
import qs.Ui
import qs.Commons
import "CodePetBar.js" as Model

// The CodePet popup card: life status, four needs, readiness, quick care and the
// desktop-pet launcher verbs. It exists only while open (a Loader in BarWidget), so
// its sleep countdown timer and launcher probe cost nothing when closed.
// Every action button sends one explicit request and shows success only after the
// response; buttons are disabled while a request is in flight.
PopupCard {
    id: card

    required property Item widget
    readonly property var st: widget.barState
    readonly property var life: st && st.life ? st.life : null
    readonly property var reg: widget.registry
    readonly property bool busy: widget.rpcBusy
    readonly property bool sleeping: st && st.care ? st.care.sleeping === true : false
    readonly property bool canCare: life !== null && life.death_pending !== true
    readonly property string appearance: life ? String(life.appearance || "") : ""
    property bool launcherPresent: true
    property double nowMs: Date.now()

    contentWidth: fittedContentWidth(Math.max(Style.space(340), body.implicitWidth))
    contentHeight: fittedContentHeight(body.implicitHeight)

    // Non-visual helpers live in an Item: the card's default property takes Items only.
    Item {
        visible: false

        // Zero-process install probe: the launcher is a small script; reading it once
        // tells whether CodePet is installed. The binary is probed by the RPC itself.
        FileView {
            path: card.widget.paths.launcher
            printErrors: false
            onLoaded: card.launcherPresent = true
            onLoadFailed: card.launcherPresent = false
        }

        // Runs only while this popup is open and the pet is sleeping; it only updates the
        // displayed countdown and never sends a request.
        Timer {
            id: sleepCountdown
            objectName: "codepetBar.sleepCountdown"
            interval: 1000
            repeat: true
            running: card.open && card.sleeping
            onTriggered: card.nowMs = Date.now()
        }
    }

    component Label: Text {
        color: Color.popups.text
        font.family: Style.font.family
        font.pixelSize: Style.font.body
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        width: Math.min(implicitWidth, Style.space(420))
    }
    component Action: Rectangle {
        id: action
        property string text: ""
        property bool enabledAction: true
        signal activated
        implicitWidth: actionLabel.implicitWidth + Style.space(16)
        implicitHeight: actionLabel.implicitHeight + Style.space(8)
        radius: Style.cornerRadius
        color: actionMouse.containsMouse && enabledAction ? Style.hoverFill : Style.normalFill
        border.width: activeFocus ? Math.max(1, Style.focusBorderWidth) : Style.normalBorderWidth
        border.color: activeFocus ? Style.focusBorderColor : Style.normalBorderColor
        opacity: enabledAction ? 1 : 0.45
        activeFocusOnTab: enabledAction
        Accessible.role: Accessible.Button
        Accessible.name: text
        Accessible.onPressAction: if (enabledAction) activated()
        Keys.onReturnPressed: if (enabledAction) activated()
        Keys.onSpacePressed: if (enabledAction) activated()
        Text {
            id: actionLabel
            anchors.centerIn: parent
            text: action.text
            color: Color.popups.text
            font.family: Style.font.family
            font.pixelSize: Style.font.body
            textFormat: Text.PlainText
        }
        MouseArea {
            id: actionMouse
            anchors.fill: parent
            hoverEnabled: true
            enabled: action.enabledAction
            onClicked: action.activated()
        }
    }

    Column {
        id: body
        spacing: Style.space(8)

        Row {
            spacing: Style.space(10)
            Item {
                width: 48
                height: 48
                Image {
                    id: headerThumb
                    anchors.fill: parent
                    readonly property string rel: card.life ? Model.thumbPath(card.life.appearance, card.life.stage) : ""
                    visible: rel !== "" && status !== Image.Error
                    source: rel !== "" ? "file://" + card.widget.paths.runtime + rel : ""
                    sourceSize.width: 96
                    sourceSize.height: 96
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                    cache: true
                }
                Text {
                    anchors.centerIn: parent
                    visible: !headerThumb.visible
                    text: "CP"
                    textFormat: Text.PlainText
                    color: Color.popups.text
                    font.bold: true
                    font.pixelSize: Style.font.body
                }
            }
            Column {
                spacing: Style.space(2)
                Label { text: Model.headerText(card.st, card.reg); font.bold: true }
                Label { visible: text !== ""; text: Model.badgesText(card.st); opacity: 0.7 }
            }
        }

        Label { visible: card.widget.paths.testState; text: "Test state: " + card.widget.paths.stateHome; opacity: 0.8 }
        Label {
            visible: !card.launcherPresent || card.widget.rpcNotInstalled
            text: "CodePet is not installed: run packaging/omarchy/install.sh"
        }
        Label {
            visible: card.st === null && card.launcherPresent && !card.widget.rpcNotInstalled
            text: card.widget.stateMissing ? "Open CodePet once to set up." : "Reading CodePet state…"
        }
        Label { visible: text !== ""; text: card.widget.grewText; font.bold: true }
        Label {
            visible: card.st !== null
            text: Model.lifecycleText(card.st, { predicted: card.widget.summary.predicted ? card.widget.summary.status : "" })
        }

        Column {
            visible: card.life !== null
            spacing: Style.space(3)
            Repeater {
                model: Model.needRows(card.st, card.reg)
                Label {
                    required property var modelData
                    text: modelData.label + ": " + (modelData.percent >= 0 ? modelData.percent + "%" : "?") + " · " + modelData.stateText
                        + (modelData.timeText ? " · " + modelData.timeText : "")
                    Accessible.name: text
                }
            }
        }

        Label {
            visible: card.sleeping
            text: {
                var left = Model.sleepRemainingMs(card.st, card.nowMs)
                return left > 0 ? "Napping · about " + Model.formatClock(left) + " left"
                    : "Nap time is up. Wake or Refresh to update."
            }
        }
        Label { visible: text !== ""; text: Model.readinessText(card.st) }
        Label {
            visible: text !== ""
            text: card.st && card.st.task ? "Task: " + String(card.st.task.state || "").replace(/_/g, " ") + " (simulation)" : ""
        }
        Label {
            visible: card.st !== null
            text: Model.formatAsOf(card.st ? card.st.generated_at_ms : 0) + " · simulation only"
            opacity: 0.7
        }

        Flow {
            width: Style.space(420)
            spacing: Style.space(6)
            visible: card.life !== null
            Repeater {
                model: ["food", "water", "play", "wash", "bathroom", "routine"]
                Action {
                    required property string modelData
                    text: Model.careLabel(card.reg, card.appearance, modelData)
                    enabledAction: !card.busy && card.canCare && !card.sleeping
                    onActivated: card.widget.care(modelData)
                }
            }
            Action {
                text: card.sleeping ? "Wake" : Model.careLabel(card.reg, card.appearance, "sleep")
                enabledAction: !card.busy && card.canCare
                onActivated: card.widget.sleepOrWake()
            }
            Action {
                text: card.life && card.life.paused ? "Resume" : "Pause"
                enabledAction: !card.busy && card.canCare
                onActivated: card.widget.togglePause()
            }
        }

        Flow {
            width: Style.space(420)
            spacing: Style.space(6)
            Action { text: "Show/hide pet"; enabledAction: card.launcherPresent; onActivated: card.widget.launch("toggle") }
            Action { text: "Tasks…"; enabledAction: card.launcherPresent; onActivated: card.widget.launch("tasks") }
            Action { text: "Companion & looks…"; enabledAction: card.launcherPresent && card.life !== null; onActivated: card.widget.launch("companion") }
            Action { text: Model.lifeButtonLabel(card.st); enabledAction: card.launcherPresent; onActivated: card.widget.launch("life") }
            Action { text: "Refresh"; enabledAction: !card.busy; onActivated: card.widget.refresh() }
        }

        Row {
            spacing: Style.space(6)
            Label { text: "Bar label:"; opacity: 0.7 }
            Repeater {
                model: ["name", "status", "none"]
                Action {
                    required property string modelData
                    text: modelData
                    opacity: card.widget.cfg.label === modelData ? 1 : 0.55
                    onActivated: card.widget.setLabelMode(modelData)
                }
            }
        }

        Label { visible: text !== ""; text: card.widget.feedback }
    }
}
