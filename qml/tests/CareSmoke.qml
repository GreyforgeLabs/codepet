// Care UI policy tests (the shared PetCare card of the standalone preview). Fixed
// fixtures are the only state authority in this test. The v0.2 task form and timeline
// are retired (SPEC-v0.3 §6.5); the panel must not show them.
import QtQuick
import "../shared"
import "../standalone"
import "../shared/PetCatalog.js" as PetCatalog

Item {
    id: top
    width: 720; height: 1000
    property int step: 0
    property int failures: 0
    property var care: null
    property int requestsBefore: 0
    property int savedFrame: 0
    property int lifeRequests: 0
    PreviewHost { id: host; onLifeRequested: top.lifeRequests += 1 }
    Panel { id: panel; width: 700; host: host }
    // A host without openLife(): the See Life button must stay hidden there.
    QtObject {
        id: bareHost
        property var snapshot: host.snapshot
        property bool busy: false
        function request(method, params, callback) {}
    }
    PetCare { id: bareCare; visible: false; width: 400; host: bareHost; showSprite: false }
    function find(obj, name) {
        if (!obj) return null
        if (obj.objectName === name) return obj
        var children = obj.children || []
        for (var i = 0; i < children.length; i++) {
            var found = find(children[i], name)
            if (found) return found
        }
        if (obj.contentItem && children.indexOf(obj.contentItem) === -1)
            return find(obj.contentItem, name)
        return null
    }
    function check(condition, label) {
        console.log((condition ? "PASS " : "FAIL ") + label)
        if (!condition) failures += 1
    }
    function fixture(name) {
        host.fixture = name
        host.snapshot = host.buildSnapshot(name)
    }
    Timer {
        interval: 200; running: true; repeat: true
        onTriggered: {
            top.step += 1
            switch (top.step) {
            case 1:
                panel.refresh()
                break
            case 2:
                top.care = top.find(panel, "panel.petCare")
                top.check(top.care !== null, "the care component loaded")
                top.check(!top.find(panel, "panel.taskForm") && !top.find(panel, "panel.timeline")
                          && !top.find(panel, "panel.taskBox") && !top.find(panel, "panel.careMeters")
                          && !top.find(panel, "panel.progression"), "no task form, timeline, meters or progression")
                top.check(!host.snapshot.care.ready_to_work && top.care.readinessText.indexOf("food") >= 0, "low bars show actionable readiness")
                top.check(top.care.canCare, "awake low-bar fixture enables explicit care")
                top.find(top.care, "petCare.food").clicked()
                break
            case 4:
                top.check(host.lastError.indexOf("PREVIEW_ONLY") === 0, "preview care refused without mutation")
                top.check(host.snapshot.meters.hunger === 7000, "care animation never grants stats")
                top.fixture("bathroom_due")
                top.check(top.find(top.care, "petCare.wash").text === PetCatalog.careLabel(top.care.petId, "bathroom"),
                          "bathroom due directs user to actual bathroom action")
                top.check(top.care.petId === "cat" && top.find(top.care, "petCare.food").text === PetCatalog.careLabel("cat", "food"),
                          "care labels come from the catalog for the v0.1 pet")
                top.check(top.find(top.care, "petCare.footer").text
                          === "Needs only change during active time while CodePet is on your bar. See Life.", "footer explains active time")
                top.check(top.find(top.care, "petCare.seeLife").visible, "See Life shows when the host can open it")
                top.find(top.care, "petCare.seeLife").clicked()
                top.check(top.lifeRequests === 1, "See Life asks the host to open the life view")
                top.check(!top.find(bareCare, "petCare.seeLife").visible, "See Life is hidden when the host cannot open it")
                top.check(!host.snapshot.care.ready_to_work, "full bars plus bathroom due is not ready")
                top.fixture("sleep_due")
                top.check(top.care.canSleep, "sleep due offers bed")
                top.fixture("sleeping")
                top.check(!top.care.canCare, "asleep blocks care")
                top.requestsBefore = host.seq
                break
            case 10:
                top.check(host.seq === top.requestsBefore, "sleep countdown launches no RPC process")
                top.check(top.care.remainingMs < 92000 && host.snapshot.care.sleep_remaining_ms === 92000,
                          "countdown is local presentation only")
                top.fixture("ready")
                top.check(top.care.animationRunning, "the Care tab sprite animates by default")
                panel.reducedMotion = true
                top.check(top.care.reducedMotion && !top.care.animationRunning,
                          "the host's less-motion preference reaches the Care tab through Panel")
                panel.reducedMotion = false
                top.check(top.care.animationRunning, "and clearing it resumes the sprite")
                top.care.reducedMotion = true
                top.check(!top.care.animationRunning, "reduced motion disables pose ticks")
                top.care.reducedMotion = false
                panel.visible = false
                top.savedFrame = top.care.frame
                top.check(!top.care.animationRunning, "hidden panel disables pose ticks")
                break
            case 13:
                top.check(top.care.frame === top.savedFrame, "hidden animation actually stays paused")
                panel.visible = true
                top.care.quietMode = true
                top.check(!top.care.animationRunning, "quiet mode keeps a static scene")
                top.care.quietMode = false
                top.check(top.care.animationRunning, "visible motion resumes without RPC")
                console.log("DONE failures=" + top.failures)
                Qt.exit(top.failures === 0 ? 0 : 3)
            }
        }
    }
}
