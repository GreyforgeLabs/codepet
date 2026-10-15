pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Only persisted task.state events are available in this milestone. No inferred
// reads, edits, shell commands, percentages or test outcomes are fabricated.
GroupBox {
    id: root
    required property var host
    property var entries: []
    // Newest loaded seq (the after_seq for newer events) and the boundary below the
    // oldest loaded event: seq <= floor is not loaded. The first load shows the latest
    // window, not the oldest history; older events are an explicit "Load older" action.
    property int cursor: 0
    property int floor: 0
    property bool loaded: false
    readonly property int pageSize: 50
    readonly property int maxEntries: 100
    readonly property bool hasOlder: loaded && floor > 0
    property bool expanded: false
    property string status: "Load the saved task-state history when you want an update."
    title: "Task timeline · simulation"

    function validEvents(response, lo, hi) {
        var list = response.result.events
        var items = []
        var malformed = 0
        for (var i = 0; i < Math.min(list.length, root.pageSize); i++) {
            var item = list[i]
            if (!item || item.event !== "task.state" || !Number.isSafeInteger(item.seq)
                    || typeof item.task_id !== "string" || !item.data || typeof item.data.state !== "string"
                    || typeof item.data.at_ms !== "number") {
                malformed += 1
                continue
            }
            if (item.seq > lo && item.seq <= hi)
                items.push(item)
        }
        return { "items": items, "malformed": malformed }
    }
    function okEvents(response) {
        return response.ok && response.result && Array.isArray(response.result.events)
    }
    function loadFailed() {
        root.status = "Could not load saved events. No progress is inferred."
    }
    function loadedStatus(malformed, more) {
        if (malformed > 0)
            return "Some malformed or unsupported events were omitted. Refresh the saved state."
        if (more)
            return root.pageSize + " newer events loaded. Load more to continue."
        return root.hasOlder ? "Latest saved state events loaded. Use Load older for earlier history."
                             : "Saved state events loaded. Use Load more after a task changes."
    }
    // First click: the latest window. Later clicks: newer events after the cursor.
    function loadMore() {
        if (root.host.busy)
            return
        if (!root.loaded) {
            root.loadLatest()
            return
        }
        root.host.request("events.subscribe", {after_seq: root.cursor, limit: root.pageSize}, function (response) {
            if (!root.okEvents(response))
                return root.loadFailed()
            var got = root.validEvents(response, root.cursor, Number.MAX_SAFE_INTEGER)
            var next = root.entries.concat(got.items)
            if (got.items.length > 0)
                root.cursor = got.items[got.items.length - 1].seq
            if (next.length > root.maxEntries) {
                next = next.slice(-root.maxEntries)
                root.floor = next[0].seq - 1
            }
            root.entries = next
            root.expanded = true
            root.status = root.loadedStatus(got.malformed, response.result.events.length === root.pageSize)
        })
    }
    function loadLatest() {
        var snap = root.host.snapshot
        if (snap && Number.isSafeInteger(snap.last_seq) && snap.last_seq >= 0) {
            root.loadWindowEnding(snap.last_seq)
            return
        }
        // No snapshot yet: one bounded request reports the current last_seq.
        root.host.request("events.subscribe", {after_seq: 0, limit: 1}, function (response) {
            if (!root.okEvents(response) || !Number.isSafeInteger(response.result.last_seq))
                return root.loadFailed()
            root.loadWindowEnding(response.result.last_seq)
        })
    }
    function loadWindowEnding(last) {
        var after = Math.max(0, last - root.pageSize)
        root.host.request("events.subscribe", {after_seq: after, limit: root.pageSize}, function (response) {
            if (!root.okEvents(response))
                return root.loadFailed()
            var got = root.validEvents(response, after, Number.MAX_SAFE_INTEGER)
            root.entries = got.items
            root.floor = after
            root.cursor = got.items.length > 0 ? got.items[got.items.length - 1].seq : after
            root.loaded = true
            root.expanded = true
            root.status = root.loadedStatus(got.malformed, false)
        })
    }
    // Explicit: the page of events just below the oldest loaded one.
    function loadOlder() {
        if (root.host.busy || !root.hasOlder)
            return
        var top = root.floor
        var after = Math.max(0, top - root.pageSize)
        root.host.request("events.subscribe", {after_seq: after, limit: top - after}, function (response) {
            if (!root.okEvents(response))
                return root.loadFailed()
            var got = root.validEvents(response, after, top)
            var next = got.items.concat(root.entries)
            root.floor = after
            if (next.length > root.maxEntries) {
                // Keep the display bounded; the dropped newest events reload with Load more.
                next = next.slice(0, root.maxEntries)
                root.cursor = next[next.length - 1].seq
            }
            root.entries = next
            root.expanded = true
            root.status = got.malformed > 0 ? root.loadedStatus(got.malformed, false)
                : root.hasOlder ? "Older events loaded. Load older to continue." : "Oldest saved events loaded."
        })
    }
    function safeText(value) {
        // Controls remain inert plain text, with bidi/terminal controls omitted.
        return String(value).replace(/[\x00-\x1f\x7f-\x9f\u202a-\u202e\u2066-\u2069]/g, "�").slice(0, 200)
    }
    ColumnLayout {
        anchors.fill: parent
        spacing: 6
        Label {
            Layout.fillWidth: true
            text: "SIMULATION: state transitions only. No repository files are read or changed, no commands run, and no tests pass as a result of these events."
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
        }
        Flow {
            Layout.fillWidth: true
            spacing: 8
            Button { objectName: "taskTimeline.load"; text: root.loaded ? "Load more / refresh timeline" : "Load saved timeline"; enabled: !root.host.busy; onClicked: root.loadMore() }
            Button { objectName: "taskTimeline.loadOlder"; text: "Load older"; visible: root.hasOlder; enabled: !root.host.busy; onClicked: root.loadOlder() }
        }
        Label { Layout.fillWidth: true; text: root.status; textFormat: Text.PlainText; wrapMode: Text.WordWrap }
        Label { visible: root.hasOlder; text: "Earlier events are not shown. Use Load older to see them (up to " + root.maxEntries + " at a time)."; textFormat: Text.PlainText; wrapMode: Text.WordWrap; Layout.fillWidth: true }
        CheckBox {
            visible: root.entries.length > 0
            text: "Show saved state history (" + root.entries.length + ")"
            checked: root.expanded
            onToggled: root.expanded = checked
        }
        Repeater {
            model: root.expanded ? root.entries : []
            delegate: Label {
                required property var modelData
                Layout.fillWidth: true
                text: "#" + modelData.seq + " · " + new Date(modelData.data.at_ms).toLocaleString()
                    + " · " + root.safeText(modelData.task_id)
                    + " · " + root.safeText(modelData.data.from || "new") + " → " + root.safeText(modelData.data.state)
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                Accessible.name: text
            }
        }
    }
}
