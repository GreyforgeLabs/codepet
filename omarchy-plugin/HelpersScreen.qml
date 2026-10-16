pragma ComponentBehavior: Bound
import QtQuick
import "CodePetBar.js" as Model

// Helpers (SPEC-v0.3 §3.5, agent_vectors.json helpers_rows): one row per installed coding
// app with its state in plain words, the hint shown at once while waiting, Connect all
// (never Codex), and per row a sheet with the one next step, Stop watching (asks first) and
// Close. Codex's grown-up step opens a picture guide. Opening Helpers runs only the
// file-only clients.status; a client program runs only after Connect, Try again or Stop.
Item {
    id: helpers
    required property var app
    readonly property var t: app.t
    readonly property var clients: app.doc ? app.doc.clients : ({})
    readonly property real tz: -new Date(app.now).getTimezoneOffset()
    readonly property var rows: Model.helperRows(helpers.clients, app.now, helpers.tz)
    readonly property var connectAll: Model.connectAll(helpers.clients)
    // "list" | "sheet" | "confirm" | "guide"
    property string view: "list"
    property string current: ""
    property string guide: ""
    property int guidePage: 0
    readonly property var currentRow: {
        for (var i = 0; i < helpers.rows.length; i++) if (helpers.rows[i].id === helpers.current) return helpers.rows[i]
        return null
    }

    implicitWidth: t.u(448)
    implicitHeight: view === "list" ? list.implicitHeight : view === "sheet" ? sheet.implicitHeight
        : view === "confirm" ? confirm.implicitHeight : guideView.implicitHeight

    Component.onCompleted: helpers.app.request("clients.status", {}, null)
    Connections {
        target: helpers.app
        function onBackRequested() { if (helpers.handleBack()) helpers.app.backTaken = true }
    }

    // ‹ Back walks back inside Helpers before it leaves it.
    function handleBack() {
        if (helpers.view === "list") return false
        helpers.view = helpers.view === "sheet" ? "list" : "sheet"
        return true
    }
    function openSheet(id) { helpers.current = id; helpers.view = "sheet" }
    function step(row) {
        if (!row || !row.step) return
        if (row.step === "btn.showme") { helpers.guide = row.guide; helpers.guidePage = 0; helpers.view = "guide"; return }
        helpers.connect([row.id])
    }
    function connect(ids) {
        if (ids.length === 0) return
        helpers.app.request("clients.connect", { clients: ids }, function (resp) { if (!resp.ok) helpers.app.failed(resp) })
    }
    function stopWatching(id) {
        helpers.view = "sheet"
        helpers.app.request("clients.disconnect", { client: id }, function (resp) { if (!resp.ok) helpers.app.failed(resp) })
    }
    function rowText(row) { return row && row.row ? helpers.app.s(row.row, { app: row.id }) : "" }
    function lineText(key, id) { return helpers.app.s(key, { app: id }) }

    // ---------------------------------------------------------------- the list
    Column {
        id: list
        visible: helpers.view === "list"
        width: parent.width
        spacing: 0
        Repeater {
            model: helpers.rows
            Item {
                id: rowItem
                required property var modelData
                width: list.width
                height: rowBox.height + helpers.t.u(4)
                Rectangle {
                    id: rowBox
                    objectName: "cp.helperRow." + rowItem.modelData.id
                    readonly property bool cpControl: true
                    readonly property string cpKind: "row"
                    width: parent.width
                    height: helpers.t.u(56) + ((rowItem.modelData.hint || rowItem.modelData.reason) ? helpers.t.u(24) : 0)
                    radius: helpers.t.smallRadius
                    color: rowMouse.pressed ? helpers.t.tilePress : rowMouse.containsMouse ? helpers.t.tileHover : helpers.t.tileFill
                    border.width: activeFocus ? Math.max(2, helpers.t.u(3)) : 0
                    border.color: helpers.t.focusRing
                    activeFocusOnTab: true
                    Accessible.role: Accessible.Button
                    Accessible.name: Model.appName(rowItem.modelData.id) + ". " + helpers.rowText(rowItem.modelData)
                    Accessible.onPressAction: helpers.openSheet(rowItem.modelData.id)
                    Keys.onReturnPressed: helpers.openSheet(rowItem.modelData.id)
                    Keys.onSpacePressed: helpers.openSheet(rowItem.modelData.id)
                    Icon {
                        id: rowIcon
                        x: helpers.t.u(8)
                        y: helpers.t.u(8)
                        icon: Model.APP_ICONS[rowItem.modelData.id] || ""
                        size: helpers.t.u(40)
                    }
                    // The app's name over its state, both at title size across the whole row:
                    // "… Waiting for Claude Code" needs the width (a monospace theme font
                    // fits about 19 characters beside a name column, notes.md §8 item 15).
                    Text {
                        id: rowName
                        anchors.left: rowIcon.right
                        anchors.leftMargin: helpers.t.u(10)
                        anchors.right: rowSpinner.visible ? rowSpinner.left : parent.right
                        anchors.rightMargin: helpers.t.u(8)
                        y: helpers.t.u(4)
                        height: helpers.t.u(24)
                        verticalAlignment: Text.AlignVCenter
                        text: Model.appName(rowItem.modelData.id)
                        textFormat: Text.PlainText
                        font.family: helpers.t.family
                        font.pixelSize: helpers.t.fontTitle
                        font.bold: true
                        color: helpers.t.text
                        elide: Text.ElideRight
                    }
                    Text {
                        objectName: "cp.helperState." + rowItem.modelData.id
                        anchors.left: rowName.left
                        anchors.right: rowName.right
                        y: helpers.t.u(28)
                        height: helpers.t.u(24)
                        verticalAlignment: Text.AlignVCenter
                        text: helpers.rowText(rowItem.modelData)
                        textFormat: Text.PlainText
                        font.family: helpers.t.family
                        font.pixelSize: helpers.t.fontTitle
                        color: rowItem.modelData.row === "row.failed" ? helpers.t.urgent : helpers.t.text
                        elide: Text.ElideRight
                    }
                    Spinner {
                        id: rowSpinner
                        visible: rowItem.modelData.spinner === true
                        anchors.right: parent.right
                        anchors.rightMargin: helpers.t.u(8)
                        y: helpers.t.u(14)
                        width: helpers.t.u(28)
                        height: width
                        t: helpers.t
                        calm: helpers.app.calm
                    }
                    Text {
                        visible: !!(rowItem.modelData.hint || rowItem.modelData.reason)
                        anchors.left: rowName.left
                        anchors.right: parent.right
                        anchors.rightMargin: helpers.t.u(8)
                        y: helpers.t.u(54)
                        height: helpers.t.u(24)
                        verticalAlignment: Text.AlignVCenter
                        text: helpers.lineText(rowItem.modelData.hint || rowItem.modelData.reason || "", rowItem.modelData.id)
                        textFormat: Text.PlainText
                        font.family: helpers.t.family
                        font.pixelSize: helpers.t.fontBody
                        color: helpers.t.dim
                        elide: Text.ElideRight
                    }
                    MouseArea {
                        id: rowMouse
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: helpers.openSheet(rowItem.modelData.id)
                    }
                }
            }
        }
        Text {
            objectName: "cp.helpersNone"
            visible: helpers.rows.length === 0
            width: parent.width
            height: helpers.t.u(56)
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: Text.AlignHCenter
            text: helpers.app.s("helpers.none")
            textFormat: Text.PlainText
            font.family: helpers.t.family
            font.pixelSize: helpers.t.fontHeading
            font.bold: true
            color: helpers.t.text
        }
        Item { width: 1; height: helpers.t.u(4) }
        Repeater {
            model: ["privacy.1", "privacy.2"]
            Text {
                required property string modelData
                width: list.width
                height: helpers.t.u(24)
                verticalAlignment: Text.AlignVCenter
                text: helpers.app.s(modelData)
                textFormat: Text.PlainText
                font.family: helpers.t.family
                font.pixelSize: helpers.t.fontBody
                color: helpers.t.dim
                elide: Text.ElideRight
            }
        }
        Item { width: 1; height: helpers.t.u(8); visible: helpers.connectAll.length > 0 }
        BigButton {
            objectName: "cp.connectAll"
            visible: helpers.connectAll.length > 0
            t: helpers.t
            width: parent.width
            height: helpers.t.u(72)
            text: helpers.app.s("btn.connect_all")
            onTapped: helpers.connect(helpers.connectAll)
        }
    }

    // ---------------------------------------------------------------- a row's sheet
    Column {
        id: sheet
        visible: helpers.view === "sheet"
        width: parent.width
        spacing: helpers.t.u(8)
        readonly property var row: helpers.currentRow
        Row {
            spacing: helpers.t.u(14)
            height: helpers.t.u(64)
            Icon {
                anchors.verticalCenter: parent.verticalCenter
                icon: Model.APP_ICONS[helpers.current] || ""
                size: helpers.t.u(64)
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: Model.appName(helpers.current)
                textFormat: Text.PlainText
                font.family: helpers.t.family
                font.pixelSize: helpers.t.fontDisplay
                font.bold: true
                color: helpers.t.text
            }
        }
        Column {
            width: parent.width
            Repeater {
                model: sheet.row ? [sheet.row.row].concat(sheet.row.reason ? [sheet.row.reason] : []).concat(sheet.row.sheet_hints || []) : []
                Text {
                    required property string modelData
                    required property int index
                    width: sheet.width
                    height: helpers.t.u(index === 0 ? 32 : 26)
                    verticalAlignment: Text.AlignVCenter
                    text: helpers.lineText(modelData, helpers.current)
                    textFormat: Text.PlainText
                    font.family: helpers.t.family
                    font.pixelSize: index === 0 ? helpers.t.fontHeading : helpers.t.fontTitle
                    font.bold: index === 0
                    color: modelData === "row.failed" ? helpers.t.urgent : helpers.t.text
                    elide: Text.ElideRight
                }
            }
        }
        BigButton {
            objectName: "cp.sheetStep"
            visible: !!(sheet.row && sheet.row.step)
            t: helpers.t
            width: parent.width
            text: sheet.row && sheet.row.step ? helpers.app.s(sheet.row.step) : ""
            onTapped: helpers.step(sheet.row)
        }
        Row {
            width: parent.width
            spacing: helpers.t.u(8)
            BigButton {
                objectName: "cp.stopWatching"
                visible: !!(sheet.row && sheet.row.stop_watching)
                t: helpers.t
                kind: "secondary"
                width: (parent.width - helpers.t.u(8)) / 2
                text: helpers.app.s("btn.stop_watching")
                onTapped: helpers.view = "confirm"
            }
            BigButton {
                objectName: "cp.sheetClose"
                t: helpers.t
                kind: "secondary"
                width: sheet.row && sheet.row.stop_watching ? (parent.width - helpers.t.u(8)) / 2 : parent.width
                text: helpers.app.s("btn.close")
                onTapped: helpers.view = "list"
            }
        }
    }

    // ---------------------------------------------------------------- Stop watching?
    Column {
        id: confirm
        visible: helpers.view === "confirm"
        width: parent.width
        spacing: helpers.t.u(12)
        Item { width: 1; height: helpers.t.u(16) }
        Icon {
            anchors.horizontalCenter: parent.horizontalCenter
            icon: Model.APP_ICONS[helpers.current] || ""
            size: helpers.t.u(72)
        }
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: helpers.app.s("ask.stop", { app: helpers.current })
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            font.family: helpers.t.family
            font.pixelSize: helpers.t.fontHeading
            font.bold: true
            color: helpers.t.text
        }
        BigButton {
            objectName: "cp.stop"
            t: helpers.t
            kind: "secondary"
            width: parent.width
            text: helpers.app.s("btn.stop")
            onTapped: helpers.stopWatching(helpers.current)
        }
        BigButton {
            objectName: "cp.keep"
            t: helpers.t
            width: parent.width
            text: helpers.app.s("btn.keep")
            onTapped: helpers.view = "sheet"
        }
    }

    // ---------------------------------------------------------------- Codex picture guide
    CodexGuide {
        id: guideView
        visible: helpers.view === "guide"
        width: parent.width
        app: helpers.app
        kind: helpers.guide
        page: helpers.guidePage
        onPageRequested: p => helpers.guidePage = p
        onDone: helpers.view = "sheet"
        onRetry: { helpers.view = "sheet"; helpers.connect(["codex"]) }
    }
}
