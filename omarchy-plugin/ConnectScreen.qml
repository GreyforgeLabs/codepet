pragma ComponentBehavior: Bound
import QtQuick
import "CodePetBar.js" as Model

// The Connect card after Let's go! (SPEC-v0.3 §3.4): the detected apps, pre-ticked (never
// Codex), the privacy lines, Connect and Not now. After Connect each row spins and turns
// into a tick or "Couldn't connect"; the ready check then makes the pet type and celebrate:
// "{name} is ready!" / "Open a new {app} window." Closing the popup counts as Not now.
Item {
    id: card
    required property var app
    property var offered: []
    readonly property var t: app.t
    readonly property var clients: app.doc ? app.doc.clients : ({})
    property var unticked: ({})
    property bool sent: false
    property var sentIds: []

    readonly property var ticked: card.offered.filter(function (id) { return card.unticked[id] !== true })
    readonly property var shownIds: card.sent ? card.sentIds : card.offered
    readonly property bool remembers: card.ticked.indexOf("claude-code") >= 0 || card.ticked.indexOf("hermes") >= 0
    readonly property var rowStates: card.shownIds.map(function (id) { return Model.connectRowState(card.clients[id]) })
    readonly property bool anyFailed: card.sent && card.rowStates.indexOf("failed") >= 0
    readonly property bool anyBusy: card.sent && card.rowStates.indexOf("busy") >= 0
    readonly property var connectedIds: card.sentIds.filter(function (id) { return Model.connectRowState(card.clients[id]) === "done" })
    readonly property bool ready: card.sent && card.app.doc && card.app.doc.agent && card.app.doc.agent.test
        && card.app.doc.agent.test.state === "done" && card.connectedIds.length > 0
    readonly property bool typing: card.sent && card.app.face && card.app.face.state === "testing"

    implicitWidth: t.u(448)
    implicitHeight: column.implicitHeight

    function toggle(id) {
        if (card.sent) return
        var u = {}
        for (var k in card.unticked) u[k] = card.unticked[k]
        u[id] = !(u[id] === true)
        card.unticked = u
    }
    function connect(ids) {
        if (ids.length === 0) { card.app.home(); return }
        card.sentIds = ids
        card.sent = true
        card.app.request("clients.connect", { clients: ids }, function (resp) {
            if (!resp.ok) card.app.failed(resp)
        })
    }
    function retry() {
        var failedIds = card.sentIds.filter(function (id) { return Model.connectRowState(card.clients[id]) === "failed" })
        card.connect(failedIds.length ? failedIds : card.sentIds)
    }

    Column {
        id: column
        width: parent.width
        spacing: 0

        // ---- title (2 lines, 64) or the ready check
        Item {
            width: parent.width
            height: card.t.u(64)
            Text {
                anchors.fill: parent
                visible: !card.typing && !card.ready
                verticalAlignment: Text.AlignVCenter
                horizontalAlignment: Text.AlignHCenter
                text: card.app.s("connect.title")
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                maximumLineCount: 2
                font.family: card.t.family
                font.pixelSize: card.t.fontHeading
                font.bold: true
                color: card.t.text
            }
            Row {
                anchors.centerIn: parent
                visible: card.typing || card.ready
                spacing: card.t.u(12)
                PetPlayer {
                    width: card.t.u(64)
                    height: width
                    registry: card.app.reg
                    species: card.app.life ? card.app.life.appearance : ""
                    stage: Model.stageOf(card.app.life)
                    petState: card.ready ? "celebrate" : "working"
                    clockMs: card.app.clockMs
                    calm: card.app.calm
                    runtimeBase: card.app.runtimeBase
                }
                Column {
                    anchors.verticalCenter: parent.verticalCenter
                    Text {
                        objectName: "cp.connectLine"
                        text: card.ready ? card.app.s("act.ready") : (card.app.faceLine() || card.app.s("row.testing"))
                        textFormat: Text.PlainText
                        font.family: card.t.family
                        font.pixelSize: card.t.fontHeading
                        font.bold: true
                        color: card.t.text
                    }
                    Text {
                        visible: card.ready
                        text: card.app.s("hint.open", { app: card.connectedIds[0] || "" })
                        textFormat: Text.PlainText
                        font.family: card.t.family
                        font.pixelSize: card.t.fontTitle
                        color: card.t.text
                    }
                }
            }
        }
        Item { width: 1; height: card.t.u(8) }

        // ---- one 56 px row per app
        Repeater {
            model: card.shownIds
            Item {
                id: row
                required property string modelData
                required property int index
                readonly property string rowState: card.sent ? Model.connectRowState(card.clients[row.modelData]) : ""
                readonly property bool on: card.unticked[modelData] !== true
                width: column.width
                height: card.t.u(56)
                Rectangle {
                    objectName: "cp.connectRow." + row.modelData
                    readonly property bool cpControl: !card.sent
                    readonly property string cpKind: "control"
                    anchors.fill: parent
                    anchors.bottomMargin: card.t.u(4)
                    radius: card.t.smallRadius
                    color: rowMouse.containsMouse && !card.sent ? card.t.tileHover : card.t.tileFill
                    activeFocusOnTab: !card.sent
                    Accessible.role: Accessible.CheckBox
                    Accessible.checkable: true
                    Accessible.checked: row.on
                    Accessible.name: Model.appName(row.modelData)
                    Accessible.onPressAction: card.toggle(row.modelData)
                    Keys.onSpacePressed: card.toggle(row.modelData)
                    Icon {
                        id: appIcon
                        anchors.left: parent.left
                        anchors.leftMargin: card.t.u(8)
                        anchors.verticalCenter: parent.verticalCenter
                        icon: Model.APP_ICONS[row.modelData] || ""
                        size: card.t.u(40)
                    }
                    Column {
                        anchors.left: appIcon.right
                        anchors.leftMargin: card.t.u(12)
                        anchors.right: mark.left
                        anchors.verticalCenter: parent.verticalCenter
                        Text {
                            width: parent.width
                            text: Model.appName(row.modelData)
                            textFormat: Text.PlainText
                            font.family: card.t.family
                            font.pixelSize: card.t.fontTitle
                            font.bold: true
                            color: card.t.text
                            elide: Text.ElideRight
                        }
                        Text {
                            width: parent.width
                            visible: row.rowState === "failed"
                            text: card.app.s(Model.REASON_COPY[(card.clients[row.modelData] || {}).failed_reason] || "reason.blocked",
                                { app: row.modelData })
                            textFormat: Text.PlainText
                            font.family: card.t.family
                            font.pixelSize: card.t.fontBody
                            color: card.t.urgent
                            elide: Text.ElideRight
                        }
                    }
                    Item {
                        id: mark
                        anchors.right: parent.right
                        anchors.rightMargin: card.t.u(8)
                        anchors.verticalCenter: parent.verticalCenter
                        width: card.t.u(44)
                        height: width
                        Rectangle {
                            anchors.fill: parent
                            visible: !card.sent || row.rowState === "done"
                            radius: card.t.smallRadius
                            color: row.on || row.rowState === "done" ? card.t.accent : "transparent"
                            border.width: Math.max(2, card.t.u(3))
                            border.color: card.t.accent
                            Text {
                                anchors.centerIn: parent
                                visible: row.on || row.rowState === "done"
                                text: "✓"
                                textFormat: Text.PlainText
                                font.family: card.t.family
                                font.pixelSize: card.t.fontDisplay
                                font.bold: true
                                color: card.t.onAccent
                            }
                        }
                        Spinner {
                            anchors.fill: parent
                            visible: row.rowState === "busy" || row.rowState === "pending"
                            t: card.t
                            calm: card.app.calm
                        }
                        Rectangle {
                            anchors.fill: parent
                            visible: row.rowState === "failed"
                            radius: width / 2
                            color: card.t.urgent
                            Text {
                                anchors.centerIn: parent
                                text: "!"
                                textFormat: Text.PlainText
                                font.family: card.t.family
                                font.pixelSize: card.t.fontDisplay
                                font.bold: true
                                color: card.t.onAccent
                            }
                        }
                    }
                    MouseArea {
                        id: rowMouse
                        anchors.fill: parent
                        enabled: !card.sent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: card.toggle(row.modelData)
                    }
                }
            }
        }
        Item { width: 1; height: card.t.u(8) }

        // ---- privacy, 2-3 lines
        Column {
            width: parent.width
            Repeater {
                model: card.remembers && !card.sent ? ["privacy.1", "privacy.2", "privacy.3"] : ["privacy.1", "privacy.2"]
                Text {
                    required property string modelData
                    width: column.width
                    height: card.t.u(24)
                    verticalAlignment: Text.AlignVCenter
                    text: card.app.s(modelData)
                    textFormat: Text.PlainText
                    font.family: card.t.family
                    font.pixelSize: card.t.fontBody
                    color: card.t.dim
                    elide: Text.ElideRight
                }
            }
        }
        Item { width: 1; height: card.t.u(8) }

        // ---- footer
        BigButton {
            objectName: "cp.connect"
            visible: !card.sent
            t: card.t
            width: parent.width
            height: card.t.u(72)
            text: card.app.s("btn.connect")
            onTapped: card.connect(card.ticked)
        }
        BigButton {
            objectName: "cp.connectRetry"
            visible: card.anyFailed && !card.anyBusy
            t: card.t
            width: parent.width
            height: card.t.u(72)
            text: card.app.s("btn.retry")
            onTapped: card.retry()
        }
        BigButton {
            objectName: "cp.connectDone"
            visible: card.sent && !card.anyFailed
            t: card.t
            width: parent.width
            height: card.t.u(72)
            text: card.app.s("btn.done")
            onTapped: card.app.home()
        }
        Item { width: 1; height: card.t.u(8) }
        BigButton {
            objectName: "cp.notNow"
            visible: !card.sent || card.anyFailed
            t: card.t
            kind: "secondary"
            width: parent.width
            text: card.sent ? card.app.s("btn.close") : card.app.s("btn.notnow")
            onTapped: card.app.home()
        }
    }
}
