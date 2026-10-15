pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "LifeText.js" as LifeText

// Memorial of ended lives (SPEC-v0.2 §12). Read-only and paged: nothing loads until the
// user presses "Show memorial", and further pages load only on the explicit "More" button
// (pet.memorial {before_ordinal, limit}). Entries are plain text from the saved summaries.
GroupBox {
    id: root
    required property var host
    property var registry: null
    property int pageSize: 10
    property var entries: []
    property var nextBefore: null
    property bool loaded: false
    property string notice: ""

    readonly property var snap: host && host.snapshot ? host.snapshot : null
    readonly property int count: snap && snap.memorial && typeof snap.memorial.count === "number" ? snap.memorial.count : 0
    readonly property bool hostBusy: !!(host && host.busy)
    readonly property bool hasMore: loaded && nextBefore !== null

    title: "Memorial" + (count > 0 ? " · " + count : "")

    function reset() {
        root.entries = []
        root.nextBefore = null
        root.loaded = false
        root.notice = ""
    }
    // Loads the first page (from the top) or the next one. Explicit user action only.
    function loadPage(more) {
        if (root.hostBusy || (more && !root.hasMore))
            return
        var params = { "limit": root.pageSize }
        if (more)
            params.before_ordinal = root.nextBefore
        root.host.request("pet.memorial", params, function (resp) {
            if (resp && resp.ok && resp.result) {
                var page = Array.isArray(resp.result.entries) ? resp.result.entries : []
                root.entries = (more ? root.entries : []).concat(page)
                root.nextBefore = typeof resp.result.next_before_ordinal === "number" ? resp.result.next_before_ordinal : null
                root.loaded = true
                root.notice = root.entries.length === 0 ? "No companions in the memorial yet." : ""
            } else {
                var e = resp && resp.error ? resp.error : {}
                root.notice = "The memorial could not be read: " + String(e.message || e.code || "unknown error")
            }
        })
    }
    function entryTitle(e) {
        var cause = e.end_cause === "retired" ? "retired" : "passed away"
        return "#" + e.ordinal + " · " + LifeText.cleanName(e.name) + " · " + LifeText.displayName(e.species, root.registry)
            + (e.appearance && e.appearance !== e.species ? " (looked like a " + LifeText.displayName(e.appearance, root.registry).toLowerCase() + ")" : "")
            + " · " + cause + (e.pace === "playtest" ? " · Playtest" : "")
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 6
        Label {
            Layout.fillWidth: true
            text: root.count === 0 ? "No companions in the memorial yet."
                : "Companions who have passed away or retired are remembered here. Their stories are kept and never change."
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
        }
        Button {
            objectName: "memorial.show"
            visible: !root.loaded && root.count > 0
            text: "Show memorial"
            enabled: !root.hostBusy
            onClicked: root.loadPage(false)
        }
        Repeater {
            model: root.entries
            delegate: ColumnLayout {
                id: entry
                required property var modelData
                Layout.fillWidth: true
                spacing: 2
                Accessible.role: Accessible.StaticText
                Accessible.name: root.entryTitle(modelData)
                Label {
                    Layout.fillWidth: true
                    text: root.entryTitle(entry.modelData)
                    textFormat: Text.PlainText
                    wrapMode: Text.WordWrap
                    font.bold: true
                }
                Label {
                    Layout.fillWidth: true
                    text: LifeText.memorialStory(entry.modelData, root.registry)
                    textFormat: Text.PlainText
                    wrapMode: Text.WordWrap
                }
            }
        }
        Button {
            objectName: "memorial.more"
            visible: root.hasMore
            text: "More"
            enabled: !root.hostBusy
            onClicked: root.loadPage(true)
        }
        Label {
            objectName: "memorial.notice"
            Layout.fillWidth: true
            visible: root.notice !== ""
            text: root.notice
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
        }
    }
}
