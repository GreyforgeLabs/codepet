pragma ComponentBehavior: Bound
import QtQuick
import "UpdateModel.js" as Updates

Item {
    id: page
    objectName: "cp.updates"
    required property var app
    readonly property var t: app.t
    readonly property var snapshot: app.ctl ? app.ctl.updateState : null
    readonly property bool supported: !app.ctl || app.ctl.updateSupported !== false
    readonly property bool busy: !!(app.ctl && app.ctl.updateBusy)
    readonly property bool active: Updates.active(snapshot)
    readonly property string phase: busy && app.ctl.updateOperation === "check" ? "checking"
        : busy && app.ctl.updateOperation === "install" ? "starting"
        : snapshot ? snapshot.state : "loading"
    implicitWidth: t.u(448)
    implicitHeight: Math.min(t.u(400), content.implicitHeight)
    Component.onCompleted: if (app.ctl && typeof app.ctl.openUpdates === "function") app.ctl.openUpdates()
    Component.onDestruction: if (app.ctl && typeof app.ctl.closeUpdates === "function") app.ctl.closeUpdates()
    function activate() {
        if (!supported || busy || active || !app.ctl) return
        if (Updates.canInstall(snapshot, busy)) app.ctl.installUpdate()
        else app.ctl.checkUpdates()
    }
    component Copy: Text {
        width: parent ? parent.width : 0
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        font.family: page.t.family
        font.pixelSize: page.t.fontBody
        color: page.t.text
        lineHeight: 1.15
    }
    Flickable {
        id: scroll
        anchors.fill: parent
        contentWidth: width
        contentHeight: content.implicitHeight
        boundsBehavior: Flickable.StopAtBounds
        clip: true
        Column {
            id: content
            width: scroll.width
            spacing: page.t.u(16)
            Copy {
                objectName: "cp.updates.heading"
                text: page.app.s("updates." + page.phase)
                font.pixelSize: page.t.fontHeading
                font.bold: true
            }
            Column {
                width: parent.width
                spacing: page.t.u(6)
                Copy { objectName: "cp.updates.current"; text: page.app.s("updates.current_version", {version: page.snapshot && page.snapshot.current ? page.snapshot.current : "—"}) }
                Copy { objectName: "cp.updates.latest"; text: page.app.s("updates.latest_version", {version: page.snapshot && page.snapshot.latest ? page.snapshot.latest : "—"}) }
            }
            Copy {
                objectName: "cp.updates.message"
                text: page.snapshot ? page.snapshot.message : page.app.s("updates.connecting")
                visible: text !== ""
                color: page.phase === "error" ? page.t.urgent : page.t.muted
                wrapMode: Text.WrapAnywhere
            }
            Copy { visible: ["downloading", "verifying", "installing", "starting"].indexOf(page.phase) >= 0; text: page.app.s("updates.progress_note"); color: page.t.muted }
            BigButton {
                objectName: "cp.updates.action"
                t: page.t
                width: parent.width
                visible: page.supported && !page.busy && !page.active
                text: page.app.s(Updates.canInstall(page.snapshot, page.busy) ? "updates.install" : "updates.check")
                onTapped: page.activate()
            }
            Copy {
                visible: page.phase === "available"
                text: page.app.s("updates.preserve")
                color: page.t.muted
            }
        }
    }
}
