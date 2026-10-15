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
    HostAdapter { id: a; responseTimeoutMs: 1000 }
    Item { width: 600; height: 800; Panel { id: panel; width: 600; host: a } }
    // Test-only stub standing in for `codepet rpc`; never a product fallback.
    readonly property string stubPath: Qt.resolvedUrl("fake_rpc.py").toString().replace("file://", "")
    function stub(mode) { a.command = ["python3", "-I", stubPath, mode] }

    Component.onCompleted: {
        stub("ok")
        a.request("state.get", {}, function (r) {
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
