// Headless Quickshell end-to-end harness: omarchy/HostAdapter.qml against the REAL
// `codepet rpc` binary with an isolated XDG_STATE_HOME/HOME. Stage like shell.qml
// (qml/README.md) and pass the paths through CODEPET_BIN and CODEPET_TEST_ROOT.
import QtQuick
import Quickshell
import "omarchy"

ShellRoot {
    id: shell
    property int failures: 0
    property string taskId: ""
    property string digest: ""
    readonly property string root: Quickshell.env("CODEPET_TEST_ROOT")
    function check(c, l) { console.log((c ? "PASS " : "FAIL ") + l); if (!c) failures += 1 }
    function finish() { console.log("DONE failures=" + failures); Qt.exit(failures === 0 ? 0 : 3) }
    HostAdapter {
        id: a
        command: ["env", "XDG_STATE_HOME=" + shell.root + "/state", "HOME=" + shell.root + "/home",
                  Quickshell.env("CODEPET_BIN"), "rpc"]
    }

    Component.onCompleted: {
        check(root.length > 0 && Quickshell.env("CODEPET_BIN").length > 0, "harness environment set")
        a.request("state.get", {}, function (r) {
            check(r.ok && a.snapshot.meters.mental_health === 7000 && a.snapshot.active_task === null,
                  "fresh state: meters 70, idle")
            a.request("pet.care", {"action": "routine"}, function (care) {
                check(care.ok && care.result.care.ready_to_work && care.result.meters.hunger === 10000,
                      "explicit care routine fills bars before preflight")
            a.request("task.create", {
                "category": "mental_health", "provider": "mock", "folder": shell.root + "/project",
                "scope": ["src"], "instructions": "Explain the state machine",
                "mock": {"outcome": "artifact", "step_ms": 50}
            }, function (r2) {
                check(r2.ok && r2.result.state === "draft", "task.create -> draft")
                shell.taskId = r2.result.task_id
                a.request("task.preflight", {"task_id": shell.taskId, "revision": 1}, function (r3) {
                    check(r3.ok && r3.result.preflight_token.length > 0, "preflight issues a token")
                    a.request("task.start", {"task_id": shell.taskId, "revision": 1,
                                             "preflight_token": r3.result.preflight_token}, function (r4) {
                        check(r4.ok && r4.result.state === "preparing", "explicit start -> preparing")
                        afterRun.start()
                    })
                })
            })
            }, "care-routine-1")
        })
    }
    Timer {
        id: afterRun; interval: 2000
        onTriggered: a.request("task.review", {"task_id": shell.taskId}, function (r) {
            shell.check(r.ok && r.result.state === "review_ready" && r.result.review.verification === "not_verified",
                        "mock supervisor reached review_ready, NOT VERIFIED")
            shell.digest = r.result.review.digest
            var p = {"task_id": shell.taskId, "review_digest": shell.digest, "acknowledged": true,
                     "override_reason": "useful mock explanation"}
            a.request("task.accept_artifact", p, function (r2) {
                shell.check(r2.ok && r2.result.receipt.amount_hundredths === 0 && r2.result.receipt.demo_xp === 20
                            && r2.result.receipt.real_xp === 0 && !r2.result.duplicate,
                            "explicit acceptance grants demo XP only, no care reward")
                a.request("task.accept_artifact", p, function (r3) {
                    shell.check(r3.ok && r3.result.receipt.request_id === "accept-1", "same request id replays the receipt")
                    a.request("state.get", {}, function (r4) {
                        shell.check(r4.ok && a.snapshot.meters.mental_health === 9500
                                    && a.snapshot.meters.hunger === 9000 && a.snapshot.active_task === null
                                    && a.snapshot.progression.demo.lifetime_xp === 20 && a.snapshot.progression.real.lifetime_xp === 0,
                                    "Rust-owned snapshot: care debit remains, demo XP20, real XP0, slot free")
                        shell.finish()
                    })
                }, "accept-1")
            }, "accept-1")
        })
    }
}
