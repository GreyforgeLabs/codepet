// Verifies the task timeline opens on the latest saved events, pages older history only
// on an explicit action, and keeps its display bounded. The host serves 130 events the
// way storage does: seq > after_seq, oldest first, at most `limit`.
import QtQuick
import "../shared"

Item {
    id: top
    width: 700; height: 900
    property int failures: 0
    property int step: 0
    property var requests: []
    QtObject {
        id: host
        property bool busy: false
        property string lastError: ""
        property int lastSeq: 130
        property var snapshot: ({last_seq: 130})
        function request(method, params, callback) {
            top.requests.push({method: method, after_seq: params.after_seq, limit: params.limit})
            busy = true
            Qt.callLater(function () {
                var events = []
                for (var seq = params.after_seq + 1; seq <= host.lastSeq && events.length < params.limit; seq++)
                    events.push({v: 1, event: "task.state", seq: seq, task_id: "task-" + Math.ceil(seq / 6),
                                 data: {state: "draft", from: null, revision: 1, at_ms: 1760400000000 + seq}})
                host.busy = false
                callback({ok: true, result: {events: events, last_seq: host.lastSeq}})
            })
        }
    }
    TaskTimeline { id: timeline; host: host; width: 680 }

    function check(cond, label) {
        console.log((cond ? "PASS " : "FAIL ") + label)
        if (!cond)
            failures += 1
    }
    function seqs() {
        return timeline.entries.length > 0
            ? timeline.entries[0].seq + ".." + timeline.entries[timeline.entries.length - 1].seq + "/" + timeline.entries.length
            : "empty"
    }
    Timer {
        interval: 50
        repeat: true
        running: true
        onTriggered: {
            top.step += 1
            switch (top.step) {
            case 1:
                timeline.loadMore()
                break
            case 3:
                top.check(top.requests[0].after_seq === 80 && top.requests[0].limit === 50, "first load asks for the latest window")
                top.check(top.seqs() === "81..130/50", "latest 50 events shown first, not the oldest history")
                top.check(timeline.hasOlder && timeline.cursor === 130 && timeline.floor === 80, "older history is available on request")
                timeline.loadOlder()
                break
            case 5:
                top.check(top.seqs() === "31..130/100", "Load older prepends the previous page")
                timeline.loadOlder()
                break
            case 7:
                top.check(top.requests[2].after_seq === 0 && top.requests[2].limit === 30, "older paging never overlaps loaded events")
                top.check(top.seqs() === "1..100/100" && !timeline.hasOlder, "display stays bounded at 100 entries")
                top.check(timeline.cursor === 100, "dropped newest events stay reachable through Load more")
                timeline.loadMore()
                break
            case 9:
                top.check(top.seqs() === "31..130/100" && timeline.cursor === 130, "Load more restores the newest events")
                top.check(timeline.hasOlder && timeline.floor === 30, "bounding the newest window re-enables Load older")
                host.lastSeq = 133
                timeline.loadMore()
                break
            case 11:
                top.check(top.seqs() === "34..133/100", "new events append after the cursor")
                // Without a snapshot, one bounded request learns last_seq first.
                fresh.loadMore()
                break
            case 14:
                top.check(fresh.entries.length === 50 && fresh.entries[49].seq === 133 && fresh.cursor === 133,
                          "latest window found without a snapshot")
                console.log("DONE failures=" + top.failures)
                Qt.exit(top.failures === 0 ? 0 : 3)
                break
            }
        }
    }
    QtObject {
        id: bareHost
        property bool busy: false
        property string lastError: ""
        property var snapshot: null
        function request(method, params, callback) { host.request(method, params, callback) }
    }
    TaskTimeline { id: fresh; host: bareHost; width: 680; y: 400 }
}
