pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "LifeText.js" as LifeText

// Shared CodePet panel. Renders the Rust-owned snapshot only; it never changes
// meters or rewards itself. Every action goes through `host.request` and is
// followed by an explicit `state.get` refresh.
//
// Host contract (see qml/README.md):
//   property var snapshot          last `state.get` result, set by the host
//   property bool busy             a request is awaiting its response
//   property string lastError      "CODE: message" of the last failed request, else ""
//   function request(method, params, callback[, requestId])
//                                  callback receives the parsed response envelope
// Optional: property bool supervisorActive (a started job's process is still alive).
// v0.2 optional: clientTag (request-id prefix), presentationLocks (care routines running),
//   openCompanion() / openLife() (host-level view requests; this panel also has its own).
//
// Views (SPEC-v0.2 §12): home (everything, the standalone default), care, tasks, companion
// and life, plus the internal form and review views. With no living companion, home and life
// show the adoption form; with an unarmed life, a small "Set up life rules" banner links to Life.
ColumnLayout {
    id: root
    required property var host
    property string view: "home"
    property bool showPetCare: true
    property string notice: ""
    // Optional generated pet registry (PetRegistry.js REGISTRY); LifeText falls back to the roster.
    property var registry: null
    readonly property var snap: host && host.snapshot ? host.snapshot : null
    readonly property var life: snap && snap.life ? snap.life : null
    // A v0.2 snapshot that reports no living companion (a v0.1 snapshot has no `life` key).
    readonly property bool noLife: LifeText.hasNoLife(snap)
    readonly property bool lifeAware: !!(snap && ("life" in snap))
    readonly property var unseenEvents: snap && Array.isArray(snap.unseen_events) ? snap.unseen_events : []
    readonly property bool inView: view === "home" || view === "care" || view === "tasks" || view === "companion" || view === "life"
    function shows(names) { return names.indexOf(root.view) >= 0 }
    readonly property var activeTask: snap && snap.active_task ? snap.active_task : null
    readonly property var meters: snap && snap.meters ? snap.meters : null
    readonly property bool paused: !!(snap && snap.decay && snap.decay.paused) || !!(life && life.paused)
    readonly property bool decayEnabled: !!(snap && snap.decay && snap.decay.enabled)
    readonly property bool hostBusy: !!(host && host.busy)
    readonly property var cancellable: ["preparing", "running", "waiting_for_user", "verifying"]
    readonly property var categoryNames: ({
        "hunger": "Bugfix",
        "thirst": "Quality & Performance",
        "mental_health": "Learning & Specification",
        "hygiene": "Cleanup"
    })
    spacing: 14

    function refresh() {
        root.host.request("state.get", {}, function (resp) {
            // The adoption form's exact numbers: one read, chained after this explicit refresh.
            if (resp && resp.ok && root.noLife && adoption.visible && !adoption.rulesLoaded && !root.host.busy)
                adoption.loadRules()
        })
    }
    function openCompanion() {
        root.view = "companion"
    }
    function openLife() {
        root.view = "life"
    }
    // Marks the growth celebration as seen, after the user acknowledged the line.
    function acknowledgeGrowth() {
        if (!root.life || root.unseenEvents.length === 0 || root.hostBusy)
            return
        var through = 0
        for (var i = 0; i < root.unseenEvents.length; i++)
            through = Math.max(through, Number(root.unseenEvents[i].seq) || 0)
        root.act("pet.events_seen", { "life_id": root.life.life_id, "through_seq": through })
    }
    function growthText() {
        if (!root.life || root.unseenEvents.length === 0)
            return ""
        var top = 0
        for (var i = 0; i < root.unseenEvents.length; i++)
            top = Math.max(top, Number(root.unseenEvents[i].stage) || 0)
        return "★ " + LifeText.cleanName(root.life.name) + " grew into a "
            + LifeText.stageName(root.life.appearance || root.life.species, top || root.life.stage, root.registry) + "!"
    }
    // Run one action, then refresh from Rust whatever the outcome.
    function act(method, params, done) {
        root.notice = ""
        root.host.request(method, params, function (resp) {
            if (done)
                done(resp)
            root.refresh()
        })
    }
    function meterValue(key) {
        return root.meters && typeof root.meters[key] === "number" ? root.meters[key] : 0
    }
    function categoryName(key) {
        return root.categoryNames[key] || String(key)
    }
    function openNewTask() {
        taskForm.reset()
        root.view = "form"
    }
    // Plain summary of a task.recover result. A metadata repair can leave the state
    // unchanged, so it is reported separately rather than as "nothing found".
    function recoverNotice(result) {
        var changed = Array.isArray(result.changed) ? result.changed.length : 0
        var repaired = Array.isArray(result.repaired) ? result.repaired.length : 0
        var parts = []
        if (changed > 0)
            parts.push(changed + " task(s) marked after recovery. Nothing was resumed.")
        if (repaired > 0)
            parts.push("Recovery metadata restored for " + repaired + " task(s); use Resolve recovery to end it without a reward.")
        return parts.length > 0 ? parts.join(" ") : "No interrupted tasks found."
    }
    // True when the active task's review cannot be echoed back to task.discard: no
    // review, or a digest an older build left empty or over the protocol's 128-byte
    // limit (task.discard would fail with INVALID_PARAMS). "Check interrupted tasks"
    // replaces such digests; this mirrors the protocol bound only, never decides.
    function reviewDigestUnusable(task) {
        if (!task || !task.review)
            return true
        var digest = task.review.digest
        if (typeof digest !== "string" || digest.length === 0)
            return true
        var bytes = 0
        for (var i = 0; i < digest.length; i++) {
            var unit = digest.charCodeAt(i)
            if (unit < 0x80)
                bytes += 1
            else if (unit < 0x800)
                bytes += 2
            else if (unit >= 0xD800 && unit <= 0xDBFF && i + 1 < digest.length
                     && digest.charCodeAt(i + 1) >= 0xDC00 && digest.charCodeAt(i + 1) <= 0xDFFF) {
                bytes += 4
                i += 1
            } else
                bytes += 3
        }
        return bytes > 128
    }
    function openReview(taskId) {
        root.view = "review"
        taskReview.load(taskId)
    }

    Label {
        Layout.fillWidth: true
        visible: root.snap === null
        text: root.hostBusy ? "Loading CodePet state…" : "No state loaded yet. Use Refresh."
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
    }

    Label {
        Layout.fillWidth: true
        visible: root.host && root.host.lastError !== ""
        text: "Error: " + (root.host ? root.host.lastError : "")
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        font.bold: true
        color: "#9b1c1c"
        Accessible.name: text
    }
    Label {
        Layout.fillWidth: true
        visible: root.notice !== ""
        text: root.notice
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
    }

    ColumnLayout {
        Layout.fillWidth: true
        visible: root.inView
        spacing: 14

        Label {
            Layout.fillWidth: true
            text: "SIMULATION ONLY · No real provider, commands or project changes"
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            font.bold: true
        }

        // Growth celebration from another UI: a static line until the user acknowledges it.
        RowLayout {
            objectName: "panel.growth"
            Layout.fillWidth: true
            visible: root.growthText() !== ""
            Label {
                Layout.fillWidth: true
                text: root.growthText()
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                font.bold: true
                Accessible.name: text
            }
            Button {
                objectName: "panel.growthSeen"
                text: "Yay"
                Accessible.name: "Mark the growth news as seen"
                enabled: !root.hostBusy
                onClicked: root.acknowledgeGrowth()
            }
        }

        // An unarmed life: nothing changes on its own until the rules are set up.
        RowLayout {
            objectName: "panel.rulesBanner"
            Layout.fillWidth: true
            visible: root.life !== null && root.life.armed === false && root.view !== "life"
            Label {
                Layout.fillWidth: true
                text: LifeText.cleanName(root.life ? root.life.name : "") + "'s life rules aren't set up yet, so the active-time clock is off."
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }
            Button {
                objectName: "panel.setUpRules"
                text: "Set up life rules"
                onClicked: root.openLife()
            }
        }

        // Life status at a glance (home and care).
        RowLayout {
            objectName: "panel.lifeSummary"
            Layout.fillWidth: true
            visible: root.life !== null && root.shows(["home", "care"])
            Label {
                objectName: "panel.lifeStatus"
                Layout.fillWidth: true
                text: root.life ? LifeText.lifeHeadline(root.life, root.registry) + ". " + LifeText.statusLine(root.snap, root.registry) : ""
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                Accessible.name: text
            }
            Button {
                text: "Life…"
                Accessible.name: "Open life and memorial"
                onClicked: root.openLife()
            }
        }

        // No living companion: adoption on home and life; a pointer elsewhere.
        AdoptionPanel {
            id: adoption
            objectName: "panel.adoption"
            Layout.fillWidth: true
            visible: root.noLife && root.shows(["home", "life"])
            host: root.host
            registry: root.registry
        }
        RowLayout {
            objectName: "panel.noLifeHint"
            Layout.fillWidth: true
            visible: root.noLife && root.shows(["care", "companion"])
            Label {
                Layout.fillWidth: true
                text: "No companion yet. Adopt one whenever you're ready; your tasks and history are unchanged."
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }
            Button {
                text: "Adopt…"
                onClicked: root.openLife()
            }
        }

        PetCare {
            objectName: "panel.petCare"
            visible: root.showPetCare && root.shows(["home", "care"]) && !root.noLife
            Layout.fillWidth: true
            host: root.host
        }
        GroupBox {
            objectName: "panel.careMeters"
            visible: root.shows(["home", "care"]) && !root.noLife
            title: "Care · all four bars must reach 100 before Start"
            Layout.fillWidth: true
            ColumnLayout {
                anchors.fill: parent
                spacing: 10
                Label {
                    Layout.fillWidth: true
                    text: "All four meters are well-being from 0 to 100: higher is better. Values come from CodePet's saved state."
                    textFormat: Text.PlainText
                    wrapMode: Text.WordWrap
                }
                WellbeingMeter { Layout.fillWidth: true; label: "Nourishment"; hundredths: root.meterValue("hunger") }
                WellbeingMeter { Layout.fillWidth: true; label: "Hydration"; hundredths: root.meterValue("thirst") }
                WellbeingMeter { Layout.fillWidth: true; label: "Mood"; hundredths: root.meterValue("mental_health") }
                WellbeingMeter { Layout.fillWidth: true; label: "Hygiene"; hundredths: root.meterValue("hygiene") }
                Label {
                    objectName: "panel.clockText"
                    Layout.fillWidth: true
                    // v0.2: the active-time clock (lifecycle.frozen_reason); v0.1 snapshots keep the legacy decay line.
                    text: root.snap && root.snap.lifecycle
                        ? (LifeText.frozenText(root.snap.lifecycle.frozen_reason) || LifeText.COUNTING_TEXT)
                        : (!root.decayEnabled ? "Decay: off (default)" : (root.paused ? "Decay: paused" : "Decay: gentle decay while open"))
                    textFormat: Text.PlainText
                    wrapMode: Text.WordWrap
                }
            }
        }

        GroupBox {
            objectName: "panel.taskBox"
            visible: root.shows(["home", "tasks"])
            title: "Task"
            Layout.fillWidth: true
            ColumnLayout {
                anchors.fill: parent
                spacing: 8
                Label {
                    visible: root.activeTask === null
                    text: "[–] No active task"
                    textFormat: Text.PlainText
                    font.bold: true
                }
                StatusBadge {
                    visible: root.activeTask !== null
                    taskState: root.activeTask ? root.activeTask.state : ""
                }
                Label {
                    Layout.fillWidth: true
                    visible: root.activeTask !== null
                    text: root.activeTask
                        ? "Category: " + root.categoryName(root.activeTask.category) + " · Provider: " + root.activeTask.provider
                            + (root.activeTask.provider === "mock" ? " (SIMULATED)" : "")
                        : ""
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }
                Label {
                    objectName: "panel.recoveryMetadataHint"
                    Layout.fillWidth: true
                    visible: root.activeTask !== null && root.activeTask.state === "recovery_required"
                        && root.reviewDigestUnusable(root.activeTask)
                    text: "The saved review metadata is missing or damaged, so this task cannot be ended yet. Use \"Check interrupted tasks\" to restore it; nothing is resumed."
                    textFormat: Text.PlainText
                    wrapMode: Text.WordWrap
                }
                Label {
                    Layout.fillWidth: true
                    visible: root.activeTask !== null && root.cancellable.indexOf(root.activeTask.state) !== -1
                    text: root.host && root.host.supervisorActive === true
                        ? "The job process is still running. Refresh to see its latest saved state."
                        : "Nothing updates on its own. Refresh to see the latest saved state."
                    textFormat: Text.PlainText
                    wrapMode: Text.WordWrap
                }
                Flow {
                    Layout.fillWidth: true
                    spacing: 8
                    Button {
                        text: "New task"
                        enabled: !root.hostBusy && root.snap !== null && root.activeTask === null
                        onClicked: root.openNewTask()
                    }
                    Button {
                        text: "Review result"
                        visible: root.activeTask !== null && root.activeTask.state === "review_ready"
                        enabled: !root.hostBusy
                        onClicked: root.openReview(root.activeTask.task_id)
                    }
                    Button {
                        objectName: "panel.resolveRecovery"
                        text: "Resolve recovery"
                        visible: root.activeTask !== null && root.activeTask.state === "recovery_required"
                        enabled: !root.hostBusy
                        onClicked: root.openReview(root.activeTask.task_id)
                    }
                    Button {
                        text: "Cancel task"
                        visible: root.activeTask !== null && root.cancellable.indexOf(root.activeTask.state) !== -1
                        enabled: !root.hostBusy
                        onClicked: root.act("task.cancel", { "task_id": root.activeTask.task_id })
                    }
                    Button {
                        objectName: "panel.pause"
                        text: root.paused ? "Resume" : "Pause/Travel"
                        enabled: !root.hostBusy && root.snap !== null && !root.noLife && !(root.life && root.life.death_pending)
                        Accessible.description: "Pause/Travel stops the active-time clock; care and tasks still work"
                        onClicked: root.act("pet.pause", { "paused": !root.paused })
                    }
                    Button {
                        text: "Refresh"
                        enabled: !root.hostBusy
                        onClicked: root.refresh()
                    }
                    Button {
                        text: "Check interrupted tasks"
                        enabled: !root.hostBusy && root.snap !== null
                        onClicked: root.act("task.recover", {}, function (resp) {
                            if (resp.ok && resp.result)
                                root.notice = root.recoverNotice(resp.result)
                        })
                    }
                }
            }
        }

        ProgressionPanel {
            objectName: "panel.progression"
            Layout.fillWidth: true
            visible: root.shows(["home", "companion"])
            progression: root.snap ? root.snap.progression : null
            life: root.life
            registry: root.registry
        }
        GroupBox {
            objectName: "panel.looks"
            Layout.fillWidth: true
            visible: root.view === "companion" && root.lifeAware && !root.noLife
            title: "Looks"
            PetChooser {
                id: chooser
                objectName: "panel.chooser"
                width: parent.width
                host: root.host
                registry: root.registry
                mode: "look"
                onClosed: root.view = "home"
            }
        }
        LifePanel {
            id: lifePanel
            objectName: "panel.life"
            Layout.fillWidth: true
            visible: root.view === "life" && root.lifeAware
            host: root.host
            registry: root.registry
        }
        TaskTimeline {
            objectName: "panel.timeline"
            Layout.fillWidth: true
            visible: root.shows(["home", "tasks"])
            host: root.host
        }
        GroupBox {
            objectName: "panel.recent"
            visible: root.shows(["home", "tasks"])
            title: "Recent results"
            Layout.fillWidth: true
            ColumnLayout {
                anchors.fill: parent
                spacing: 6
                Label {
                    visible: !root.snap || !root.snap.recent || root.snap.recent.length === 0
                    text: "No tasks yet."
                    textFormat: Text.PlainText
                }
                Repeater {
                    model: root.snap && root.snap.recent ? root.snap.recent : []
                    delegate: ColumnLayout {
                        id: recentRow
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: 2
                        StatusBadge { taskState: recentRow.modelData.state }
                        Label {
                            Layout.fillWidth: true
                            text: root.categoryName(recentRow.modelData.category) + " · " + recentRow.modelData.provider
                                + " · " + String(recentRow.modelData.instructions_excerpt || "").split("\n")[0]
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                        }
                        Label {
                            Layout.fillWidth: true
                            visible: !!recentRow.modelData.receipt
                            text: recentRow.modelData.receipt
                                ? "Saved receipt: " + (recentRow.modelData.receipt.demo_xp || 0) + " demo XP · "
                                    + (recentRow.modelData.receipt.real_xp || 0) + " real-work XP"
                                    + (recentRow.modelData.receipt.amount_hundredths > 0 ? " · historical care reward: +" + (recentRow.modelData.receipt.amount_hundredths / 100).toFixed(1) : "")
                                    + " (verification: " + recentRow.modelData.receipt.verification + ")"
                                : ""
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                        }
                        Button {
                            text: "Continue draft"
                            visible: recentRow.modelData.state === "draft"
                            enabled: !root.hostBusy && root.activeTask === null
                            onClicked: {
                                var id = recentRow.modelData.task_id
                                taskForm.loadDraftById(id, function (ok) {
                                    if (ok)
                                        root.view = "form"
                                })
                            }
                        }
                    }
                }
            }
        }
    }

    TaskForm {
        id: taskForm
        objectName: "panel.taskForm"
        Layout.fillWidth: true
        visible: root.view === "form"
        host: root.host
        onClosed: {
            root.view = "home"
            root.refresh()
        }
    }

    TaskReview {
        id: taskReview
        objectName: "panel.taskReview"
        Layout.fillWidth: true
        visible: root.view === "review"
        host: root.host
        onClosed: {
            root.view = "home"
            root.refresh()
        }
    }
}
