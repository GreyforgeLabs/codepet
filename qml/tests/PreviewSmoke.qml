// Headless smoke test for the preview host and shared components. See qml/README.md.
// The v0.2 task form, review and recovery views are retired (SPEC-v0.3 §6.5).
import QtQuick
import "../shared"
import "../standalone"

Item {
    id: top
    width: 700
    height: 900
    property int step: 0
    property int failures: 0

    PreviewHost { id: host; fixture: "ready" }
    Panel { id: panel; width: 680; host: host }

    function find(obj, name) {
        if (!obj)
            return null
        if (obj.objectName === name)
            return obj
        var kids = obj.children || []
        for (var i = 0; i < kids.length; i++) {
            var r = find(kids[i], name)
            if (r)
                return r
        }
        if (obj.contentItem && kids.indexOf(obj.contentItem) === -1) {
            var c = find(obj.contentItem, name)
            if (c)
                return c
        }
        return null
    }
    function check(cond, label) {
        console.log((cond ? "PASS " : "FAIL ") + label)
        if (!cond)
            failures += 1
    }
    function meters() { return JSON.stringify(host.snapshot.meters) }
    property string initialMeters: ""

    Timer {
        interval: 120
        repeat: true
        running: true
        onTriggered: {
            top.step += 1
            switch (top.step) {
            case 1:
                panel.refresh()
                break
            case 2:
                top.check(host.snapshot !== null && host.snapshot.meters.hunger === 10000, "ready state.get fixture loaded")
                top.check(!top.find(panel, "panel.taskForm") && !top.find(panel, "panel.taskReview")
                          && !top.find(panel, "panel.taskBox") && !top.find(panel, "panel.recent")
                          && !top.find(panel, "panel.timeline"), "no task form, review, box, results or timeline")
                top.check(typeof panel.openNewTask === "undefined" && typeof panel.openReview === "undefined",
                          "the task entry points are gone")
                // v0.2 life fixtures: lifecycle mutations are refused like every other mutation.
                host.fixture = "unarmed"
                panel.view = "life"
                panel.refresh()
                break
            case 4:
                top.check(panel.life !== null && panel.life.armed === false, "unarmed fixture loaded")
                top.check(top.find(panel, "life.rulesCard").visible, "unarmed life shows the rules card in the preview")
                top.find(panel, "rulesCard.ack").checked = true
                top.find(panel, "rulesCard.start").clicked()
                break
            case 6:
                top.check(host.lastError.indexOf("PREVIEW_ONLY") === 0, "preview pet.rules refused: " + host.lastError)
                top.check(host.snapshot.life.armed === false, "preview never arms a life")
                host.fixture = "no_life_memorial"
                panel.view = "home"
                panel.refresh()
                break
            case 9:
                var adoption = top.find(panel, "panel.adoption")
                top.check(adoption.visible && adoption.rulesLoaded, "preview serves lifecycle.rules to the adoption form")
                top.find(panel, "adoption.ack").checked = true
                top.find(panel, "adoption.adopt").clicked()
                break
            case 11:
                top.check(host.lastError.indexOf("PREVIEW_ONLY") === 0 && host.snapshot.life === null, "preview pet.adopt refused; still no companion")
                panel.view = "life"
                top.find(panel, "memorial.show").clicked()
                break
            case 13:
                top.check(top.find(panel, "life.memorial").entries.length === 3, "preview serves pet.memorial pages")
                // Every fixture renders in every view without a request.
                var before = host.seq
                var views = ["home", "care", "companion", "life"]
                for (var i = 0; i < host.fixtureNames.length; i++) {
                    host.fixture = host.fixtureNames[i]
                    host.snapshot = host.buildSnapshot(host.fixtureNames[i])
                    for (var v = 0; v < views.length; v++) {
                        panel.view = views[v]
                        top.check(panel.inView, host.fixtureNames[i] + " renders in " + views[v])
                    }
                    var snap = host.snapshot
                    top.check(snap.lifecycle && typeof snap.lifecycle.frozen_reason === "string"
                              && ("life" in snap) && Array.isArray(snap.unseen_events) && snap.memorial,
                              host.fixtureNames[i] + " has the v0.2 snapshot blocks")
                }
                top.check(host.seq === before, "switching fixtures and views sends no request")
                top.check(host.fixtureNames.length === 20, "20 preview fixtures")
                var wanted = ["unarmed", "warning_thirst", "critical_hunger", "gentle_fainted", "death_pending_review",
                              "no_life_memorial", "evolved_unseen", "paused", "aquatic_goldfish", "robot_working"]
                top.check(wanted.every(function (n) { return host.fixtureNames.indexOf(n) >= 0 }), "all spec §12 life fixtures exist")
                host.snapshot = host.buildSnapshot("aquatic_goldfish")
                top.check(host.snapshot.life.appearance === "goldfish" && host.snapshot.life.stage === 3, "aquatic fixture is a mature goldfish")
                host.snapshot = host.buildSnapshot("robot_working")
                top.check(host.snapshot.life.appearance === "robot" && host.snapshot.life.species === "cat",
                          "robot look on a cat life")
                console.log("DONE failures=" + top.failures)
                Qt.exit(top.failures === 0 ? 0 : 3)
                break
            }
        }
    }
}
