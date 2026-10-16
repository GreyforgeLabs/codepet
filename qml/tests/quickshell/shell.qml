// Headless Quickshell harness for omarchy/HostAdapter.qml (the overlay's explicit
// `codepet rpc` transport). Quickshell only loads QML inside the config folder, so run
// this from a staged copy (qml/README.md): <tmp>/shell.qml, <tmp>/fake_rpc.py, <tmp>/omarchy/.
import QtQuick
import Quickshell
import "omarchy"

ShellRoot {
    id: shell
    property int failures: 0
    property int answers: 0
    property string lastAnswered: ""
    function check(c, l) { console.log((c ? "PASS " : "FAIL ") + l); if (!c) failures += 1 }
    HostAdapter {
        id: a
        responseTimeoutMs: 1000
        clientTag: "harness"
        onAnswered: (method, response) => { shell.answers += 1; shell.lastAnswered = method }
    }
    // Test-only stub standing in for `codepet rpc`; never a product fallback.
    readonly property string stubPath: Qt.resolvedUrl("fake_rpc.py").toString().replace("file://", "")
    function stub(mode) { a.command = ["python3", "-I", stubPath, mode] }

    Component.onCompleted: {
        stub("ok")
        a.request("state.get", {}, function (r) {
            check(r.ok && r.id.indexOf("harness-") === 0, "request ids carry the clientTag prefix: " + r.id)
            check(r.ok && r.result.echo_params !== undefined, "state.get via stdin/stdout returns the result")
            check(!a.busy && a.lastError === "", "not busy, no error")
            check(shell.answers === 0, "the callback runs before `answered`")
            Qt.callLater(function () {
                check(shell.answers === 1 && shell.lastAnswered === "state.get", "answered(method, response) once")
                a.request("fail", {}, function (r2) {
                    check(!r2.ok && a.lastError === "BUSY: slot taken", "error envelope -> lastError")
                    Qt.callLater(function () {
                        a.request("state.get", {}, function (r3) {
                            check(r3.ok && a.lastError === "", "a later success clears lastError")
                            Qt.callLater(shell.failureModes)
                        })
                        var refused = null
                        a.request("state.get", {}, function (r4) { refused = r4 })
                        check(refused && !refused.ok && refused.error.code === "CLIENT_BUSY",
                              "a second request while one is in flight is refused at once")
                    })
                })
            })
        })
    }
    function failureModes() {
        stub("noresp")
        a.request("state.get", {}, function (r) {
            shell.check(!r.ok && r.error.code === "TRANSPORT" && r.error.message.indexOf("stub: refusing") !== -1,
                        "exit without response -> TRANSPORT with stderr tail: " + r.error.message)
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
                            shell.check(!a.busy, "and the adapter is free again")
                            console.log("DONE failures=" + shell.failures)
                            Qt.exit(shell.failures === 0 ? 0 : 3)
                        })
                    })
                })
            })
        })
    }
}
