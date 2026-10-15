// Headless Quickshell harness for omarchy/HostAdapter.qml. See qml/README.md.
import QtQuick
import Quickshell
// Quickshell only loads QML inside the config folder, so run this from a staged copy
// (qml/README.md): <tmp>/shell.qml, <tmp>/fake_rpc.py, <tmp>/omarchy/, <tmp>/shared/.
import "omarchy"
import "shared"

ShellRoot {
    id: shell
    property int failures: 0
    function check(c, l) { console.log((c ? "PASS " : "FAIL ") + l); if (!c) failures += 1 }
    property int companionAsks: 0
    property int lifeAsks: 0
    property int replaced: 0
    HostAdapter {
        id: a
        responseTimeoutMs: 1000
        clientTag: "harness"
        onCompanionRequested: shell.companionAsks += 1
        onLifeRequested: shell.lifeAsks += 1
        onSnapshotReplaced: shell.replaced += 1
    }
    function barState(version, appearance) {
        return {
            "schema": "codepet.bar.v1", "state_version": version,
            "life": {"life_id": "life-000000000000000000000001", "ordinal": 1, "name": "Ginger",
                     "appearance": appearance, "stage": 2, "level": 2, "xp": 120, "status": "warning",
                     "mode": "permadeath", "armed": true, "pace": "steady", "paused": true,
                     "fainted": false, "death_pending": false},
            "meters": {"hunger": 6100, "thirst": 2800, "mental_health": 9000, "hygiene": 9000},
            "care": {"sleeping": false, "sleep_remaining_ms": 120000, "sleep_started_at_ms": null,
                     "sleep_due": false, "bathroom_due": true, "ready_to_work": false,
                     "readiness_reasons": ["Needs water"]},
            "task": null, "unseen_events": [{"seq": 88, "stage": 2}],
            "memorial": {"count": 0, "latest_name": null}
        }
    }
    Item { width: 600; height: 800; Panel { id: panel; width: 600; host: a } }
    // Test-only stub standing in for `codepet rpc`; never a product fallback.
    readonly property string stubPath: Qt.resolvedUrl("fake_rpc.py").toString().replace("file://", "")
    function stub(mode) { a.command = ["python3", "-I", stubPath, mode] }

    Component.onCompleted: {
        check(a.presentationLocks === 0, "presentationLocks starts at zero")
        a.openCompanion()
        a.openLife()
        check(companionAsks === 1 && lifeAsks === 1, "openCompanion/openLife only signal the host")
        check(!a.applyBarState(barState(3, "owl")) && a.snapshot === null && a.lastBarState.state_version === 3,
              "bar state before the first snapshot is kept, not applied")
        stub("ok")
        a.request("state.get", {}, function (r) {
            check(r.ok && r.id.indexOf("harness-") === 0, "request ids carry the clientTag prefix: " + r.id)
            check(shell.replaced === 1 && a.snapshot.state_version === 3 && a.snapshot.life.appearance === "owl",
                  "a buffered newer bar state merges into the first snapshot")
            a.lastBarState = null
            check(r.ok && a.snapshot && a.snapshot.meters.mental_health === 9000, "state.get via stdin/stdout sets snapshot")
            check(!a.busy && a.lastError === "", "not busy, no error")
            a.request("fail", {}, function (r2) {
                check(!r2.ok && a.lastError === "BUSY: slot taken", "error envelope -> lastError")
                a.request("state.get", {}, function () {
                    check(a.lastError === "BUSY: slot taken", "state.get refresh keeps lastError")
                    a.request("task.start", {"task_id": "t1"}, function (r3) {
                        check(r3.ok && r3.result.state === "preparing", "task.start response read before process exit")
                        check(a.supervisorActive, "supervisorActive while start process lives")
                        afterStart.start()
                    }, "fixed-id-1")
                })
            })
        })
    }
    Timer {
        id: afterStart; interval: 2500
        onTriggered: {
            shell.check(!a.supervisorActive, "supervisorActive cleared after supervisor exit")
            shell.stub("noresp")
            a.request("state.get", {}, function (r) {
                shell.check(!r.ok && r.error.code === "TRANSPORT" && r.error.message.indexOf("stub: refusing") !== -1, "exit without response -> TRANSPORT with stderr tail: " + r.error.message)
                shell.stub("garbage")
                a.request("state.get", {}, function (r2) {
                    shell.check(!r2.ok && r2.error.code === "PROTOCOL_MISMATCH", "non-envelope -> PROTOCOL_MISMATCH")
                    shell.stub("wrongid")
                    a.request("state.get", {}, function (r3) {
                        shell.check(!r3.ok && r3.error.code === "PROTOCOL_MISMATCH", "wrong id -> PROTOCOL_MISMATCH")
                        shell.stub("hang")
                        a.request("state.get", {}, function (r4) {
                            shell.check(!r4.ok && r4.error.code === "CLIENT_TIMEOUT", "hang -> CLIENT_TIMEOUT")
                            shell.check(!a.busy, "busy cleared after timeout")
                            a.command = ["/nonexistent/codepet", "rpc"]
                            a.request("state.get", {}, function (r5) {
                                shell.check(!r5.ok, "missing binary -> error " + JSON.stringify(r5.error))
                                shell.check(a.snapshot.meters.mental_health === 9000, "snapshot unchanged by failures")
                                shell.check(panel.meterValue("mental_health") === 9000 && panel.activeTask === null, "shared Panel renders the adapter snapshot")
                                shell.check(a.applyBarState(shell.barState(10, "robot")) && a.snapshot.state_version === 10
                                            && a.snapshot.life.appearance === "robot" && a.snapshot.meters.hunger === 6100
                                            && a.snapshot.care.bathroom_due === true && a.snapshot.decay === undefined
                                            && a.snapshot.echo_params !== undefined
                                            && a.snapshot.unseen_events[0].seq === 88,
                                            "newer bar state merges meters, life and care without RPC")
                                shell.check(!a.applyBarState(shell.barState(9, "frog")) && !a.applyBarState(shell.barState(10, "frog"))
                                            && a.snapshot.life.appearance === "robot", "older or equal bar states are ignored")
                                shell.check(!a.applyBarState(null) && !a.applyBarState({"state_version": "11"}),
                                            "non-numeric bar states are ignored")
                                var versionless = a.snapshot
                                a.snapshot = {"meters": {"hunger": 1}}
                                shell.check(!a.applyBarState(shell.barState(12, "dog")) && a.snapshot.meters.hunger === 1,
                                            "a snapshot without state_version is never overridden")
                                a.snapshot = versionless
                                shell.check(!a.busy && shell.replaced === 2, "merging never issued a request")
                                console.log("DONE failures=" + shell.failures)
                                Qt.exit(shell.failures === 0 ? 0 : 3)
                            })
                        })
                    })
                })
            })
        }
    }
}
