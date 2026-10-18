// Headless Quickshell end-to-end harness: the overlay's inputs against the REAL
// `codepet rpc` binary in an isolated XDG_STATE_HOME/HOME (never the owner's state).
// omarchy/HostAdapter.qml sends the explicit requests a test plays for the user (the bar
// popup sends them in the product); after each one the exported bar-state.json is read
// as the overlay reads it (omarchy/BarState.js), the face is computed with the shared
// priority (shared/petstate.js) and the exit rule applied (omarchy/OverlayFlow.js).
// Stage like shell.qml (qml/README.md) and pass the paths through CODEPET_BIN and
// CODEPET_TEST_ROOT. Run by test_quickshell_host.py when CODEPET_TEST_BIN is set.
import QtQuick
import Quickshell
import Quickshell.Io
import "omarchy"
import "omarchy/BarState.js" as BarState
import "omarchy/OverlayFlow.js" as OverlayFlow
import "shared/petstate.js" as PetState

ShellRoot {
    id: shell
    property int failures: 0
    readonly property string root: Quickshell.env("CODEPET_TEST_ROOT")
    readonly property string stateDir: root + "/state/codepet"
    readonly property var seen: ({ finished_seq: 0, hello_seq: 0, test_at_ms: 0 })
    function check(c, l) { console.log((c ? "PASS " : "FAIL ") + l); if (!c) failures += 1 }
    function finish() { console.log("DONE failures=" + failures); Qt.exit(failures === 0 ? 0 : 3) }
    HostAdapter {
        id: a
        clientTag: "real"
        command: ["env", "XDG_STATE_HOME=" + shell.root + "/state", "HOME=" + shell.root + "/home",
                  Quickshell.env("CODEPET_BIN"), "rpc"]
    }
    FileView { id: barFile; path: shell.stateDir + "/bar-state.json"; blockLoading: true; printErrors: false }
    FileView { id: offFlag; path: shell.stateDir + "/off"; blockLoading: true; printErrors: false }
    // The export as the overlay sees it (null when missing or rejected).
    function exported() {
        barFile.reload()
        barFile.waitForJob()
        return BarState.parse(barFile.text())
    }
    function offFlagPresent() {
        offFlag.reload()
        offFlag.waitForJob()
        return offFlag.loaded
    }
    function face(doc) { return PetState.currentPriority(doc, Date.now(), shell.seen) }

    Component.onCompleted: {
        check(root.length > 0 && Quickshell.env("CODEPET_BIN").length > 0, "harness environment set")
        a.request("state.get", {}, function (r) {
            check(r.ok, "state.get on a fresh state home: " + JSON.stringify(r.error || {}))
            var doc = exported()
            check(doc !== null, "the export parses as the overlay reads it (" + (doc ? doc.schema : "none") + ")")
            check(doc && doc.enabled === true && doc.life === null && doc.treats.jar === 0,
                  "fresh install: on, no pet, an empty jar")
            check(doc && face(doc).state === "no_pet" && face(doc).overlay === "none", "no pet: nothing on the desktop")
            check(OverlayFlow.exitReason(doc, null) === "", "the overlay keeps running")
            a.request("pet.start", {"species": "cat", "mode": "gentle", "acknowledge_rules": true}, function (r2) {
                check(r2.ok, "Let's go! (pet.start) adopts the cat: " + JSON.stringify(r2.error || {}))
                var d2 = exported()
                check(d2 && d2.life && d2.life.appearance === "cat" && d2.life.armed === true && d2.life.mode === "gentle",
                      "the export carries the armed, gentle cat")
                var f2 = d2 ? face(d2) : null
                check(f2 && f2.overlay !== "none" && f2.state !== "no_pet" && f2.state !== "welcome",
                      "the overlay draws it: " + (f2 ? f2.state + "/" + f2.overlay : "none"))
                a.request("app.set_enabled", {"enabled": false}, function (r3) {
                    check(r3.ok, "Off (app.set_enabled false)")
                    var d3 = exported()
                    check(d3 && d3.enabled === false && d3.presence.wanted === false, "the export says Off")
                    check(d3 && face(d3).state === "off" && face(d3).wake_at_ms === null, "Off face: no timer to arm")
                    check(OverlayFlow.exitReason(d3, true) === "off", "the overlay process would exit")
                    check(offFlagPresent(), "the hooks' off flag exists")
                    a.request("app.set_enabled", {"enabled": true}, function (r4) {
                        check(r4.ok, "On again")
                        var d4 = exported()
                        check(d4 && d4.enabled === true && d4.life && d4.life.name === d2.life.name,
                              "On resumes the same pet")
                        check(!offFlagPresent(), "and the off flag is gone")
                        shell.finish()
                    })
                })
            })
        })
    }
}
