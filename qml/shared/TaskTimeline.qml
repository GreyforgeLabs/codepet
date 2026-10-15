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
    property int cursor: 0
    property bool omitted: false
    property bool expanded: false
    property string status: "Load the saved task-state history when you want an update."
    title: "Task timeline · simulation"
    function loadMore() {
        if (root.host.busy)
            return
        root.host.request("events.subscribe", {after_seq: root.cursor, limit: 50}, function (response) {
            if (!response.ok || !response.result || !Array.isArray(response.result.events)) {
                root.status = "Could not load saved events. No progress is inferred."
                return
            }
            var next = root.entries.slice()
            var malformed = 0
            var list = response.result.events
            for (var i = 0; i < Math.min(list.length, 50); i++) {
                var item = list[i]
                if (!item || item.event !== "task.state" || !Number.isSafeInteger(item.seq) || item.seq <= root.cursor
                        || typeof item.task_id !== "string" || !item.data || typeof item.data.state !== "string"
                        || typeof item.data.at_ms !== "number") {
                    malformed += 1
                    continue
                }
                root.cursor = item.seq
                next.push(item)
            }
            if (next.length > 100) {
                next = next.slice(-100)
                root.omitted = true
            }
            root.entries = next
            root.expanded = true
            root.status = malformed > 0 ? "Some malformed or unsupported events were omitted. Refresh the saved state."
                : list.length === 50 ? "50 events loaded. Load more to continue." : "Saved state events loaded. Use Load more after a task changes."
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
        Button { objectName: "taskTimeline.load"; text: root.cursor === 0 ? "Load saved timeline" : "Load more / refresh timeline"; enabled: !root.host.busy; onClicked: root.loadMore() }
        Label { Layout.fillWidth: true; text: root.status; textFormat: Text.PlainText; wrapMode: Text.WordWrap }
        Label { visible: root.omitted; text: "Earlier events omitted from this 100-event display."; textFormat: Text.PlainText }
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
