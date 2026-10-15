// Care UI policy tests. Fixed fixtures are the only state authority in this test.
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
    property var form: null
    property var timeline: null
    property int requestsBefore: 0
    property int savedFrame: 0
    PreviewHost { id: host }
    Panel { id: panel; width: 700; host: host }
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
                top.form = top.find(panel, "panel.taskForm")
                top.timeline = top.find(panel, "panel.timeline")
                top.check(top.care && top.form && top.timeline, "care, form and timeline components loaded")
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
                top.check(!top.find(top.care, "petCare.seeLife").visible, "See Life is hidden when the host cannot open it")
                top.check(!host.snapshot.care.ready_to_work, "full bars plus bathroom due is not ready")
                top.fixture("sleep_due")
                top.check(!top.form.readyToWork && top.care.canSleep, "sleep due blocks Start and offers bed")
                top.fixture("sleeping")
                top.check(!top.care.canCare && !top.form.readyToWork, "asleep blocks care and Start")
                top.requestsBefore = host.seq
                break
            case 10:
                top.check(host.seq === top.requestsBefore, "sleep countdown launches no RPC process")
                top.check(top.care.remainingMs < 92000 && host.snapshot.care.sleep_remaining_ms === 92000,
                          "countdown is local presentation only")
                top.fixture("ready")
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
                panel.openNewTask()
                top.form.setFields("mental_health", "mock", "/tmp/project", ["docs"], "Explain bounded code", "artifact", 0)
                top.find(top.form, "taskForm.preflight").clicked()
                break
            case 16:
                top.find(top.form, "taskForm.ack").checked = true
                top.check(top.find(top.form, "taskForm.start").enabled, "ready + fresh reviewed preflight permits Start")
                top.fixture("idle")
                var revised = host.buildSnapshot("idle")
                revised.care.revision = 2
                host.snapshot = revised
                top.check(!top.form.preflightFresh && !top.find(top.form, "taskForm.ack").checked,
                          "care revision change clears preflight and acknowledgement")
                top.check(top.form.formError.indexOf("Care or rest changed") >= 0, "stale care token has actionable re-preflight hint")
                top.check(!top.find(top.form, "taskForm.start").enabled, "care change invalidates Start availability even with same preflight")
                top.requestsBefore = host.seq
                top.form.startClicked()
                top.check(host.seq === top.requestsBefore, "programmatic Start handler also enforces care gate")
                panel.view = "home"
                top.timeline.loadMore()
                break
            case 19:
                top.check(top.timeline.entries.length === 3 && top.timeline.cursor === 3, "durable fixture transitions rendered")
                top.check(top.timeline.entries[2].data.state === "running", "timeline uses actual event state")
                top.timeline.loadMore()
                break
            case 21:
                top.check(top.timeline.entries.length === 3, "repeated timeline refresh adds no fake events")
                top.fixture("demo_level_2")
                top.check(host.snapshot.progression.demo.level === 2 && host.snapshot.progression.real.lifetime_xp === 0,
                          "demo level fixture isolated from real-work XP")
                top.check(host.snapshot.providers.filter(function (p) { return p.available }).length === 1,
                          "demo level unlocks no real provider")
                console.log("DONE failures=" + top.failures)
                Qt.exit(top.failures === 0 ? 0 : 3)
            }
        }
    }
}
