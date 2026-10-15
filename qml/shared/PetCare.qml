pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window

// Every stat comes from a Rust snapshot. Timers only paint a pose/countdown or
// finish a user-started care sequence; they never start work or grant readiness.
ColumnLayout {
    id: root
    required property var host
    property bool reducedMotion: false
    property bool quietMode: false
    property string feedback: "Care gets your pet ready. Accepted work earns skills."
    property string reaction: ""
    property var careTrail: []
    property int frame: 0
    property var routineSteps: []
    property bool routineRunning: false
    property int routineGeneration: 0
    property string retryKey: ""
    property string retrySignature: ""
    property int actionSequence: 0
    property double countdownStarted: Date.now()
    property double countdownNow: Date.now()
    readonly property var snap: host && host.snapshot ? host.snapshot : null
    readonly property var care: snap && snap.care ? snap.care : null
    readonly property var meters: snap && snap.meters ? snap.meters : ({})
    readonly property bool hostBusy: !!(host && host.busy)
    readonly property bool sleeping: !!(care && care.sleeping)
    readonly property bool presentationActive: visible && (!Window.window || (Window.window.visible && Window.window.visibility !== Window.Minimized))
    readonly property bool animationRunning: presentationActive && !reducedMotion && !quietMode && !sleeping
    readonly property bool canCare: care !== null && !sleeping && !hostBusy && !routineRunning
    readonly property string taskState: snap && snap.active_task ? String(snap.active_task.state) : ""
    readonly property bool canSleep: care !== null && !hostBusy && !routineRunning
        && ["preparing", "running", "waiting_for_user", "verifying", "cancelling", "recovery_required"].indexOf(taskState) === -1
    readonly property int remainingMs: care ? Math.max(0, care.sleep_remaining_ms - (sleeping ? Math.max(0, countdownNow - countdownStarted) : 0)) : 0
    readonly property string readinessText: !care ? "Refresh to load care and readiness"
        : care.ready_to_work ? "Ready to work"
        : (care.readiness_reasons || []).join(" · ") || "Not ready. Refresh the saved state."
    spacing: 10
    onCareChanged: { countdownStarted = Date.now(); countdownNow = countdownStarted }
    onPresentationActiveChanged: if (!presentationActive) stopRoutine()

    function timeLeft(ms) {
        var seconds = Math.ceil(ms / 1000)
        return Math.floor(seconds / 60) + ":" + (seconds % 60 < 10 ? "0" : "") + seconds % 60
    }
    function actionLabel(action) {
        var labels = {food: "Food offered. Nourishment restored.", water: "Fresh water. Hydration restored.",
            play: "A little play and encouragement. Mood restored.", wash: "Paws washed. Hygiene restored.",
            bathroom: "Bathroom break complete. Paws washed.", sleep: "Tucked into bed for a two-minute nap.",
            wake: "Sleep state checked. Your saved rest credit is preserved."}
        return labels[action] || "Care recorded."
    }
    function perform(method, params, action, continuation) {
        if (root.hostBusy || !root.care)
            return
        var signature = method + JSON.stringify(params)
        if (root.retryKey === "" || root.retrySignature !== signature) {
            root.actionSequence += 1
            root.retryKey = "care-" + Date.now().toString(36) + "-" + root.actionSequence
            root.retrySignature = signature
        }
        var key = root.retryKey
        root.host.request(method, params, function (response) {
            if (response.ok) {
                root.retryKey = ""
                root.reaction = action
                var settled = response.result ? response.result.care : null
                root.feedback = action === "wake"
                    ? (settled && !settled.sleeping && !settled.sleep_due
                        ? "Well rested. Ready for care and another task."
                        : "Awake early. Remaining rest is saved; another nap is still needed.")
                    : action === "sleep" ? "Tucked into bed. Saved rest credit continues toward the two-minute nap."
                    : response.result && response.result.changed === false
                        ? "Already cared for. A little affection is always welcome; no stats or XP changed."
                        : root.actionLabel(action)
                root.careTrail = root.careTrail.concat([root.feedback]).slice(-4)
                reactionEnd.restart()
            } else {
                root.feedback = "Care was not confirmed. " + String(response.error ? response.error.message : "Refresh to check saved state.")
                root.stopRoutine()
            }
            root.host.request("state.get", {}, function (refresh) {
                if (!refresh.ok) {
                    root.stopRoutine()
                    root.feedback = "The action finished, but saved state could not be refreshed. Refresh before continuing care."
                }
                if (continuation)
                    continuation(response.ok && refresh.ok)
            })
        }, key)
    }
    function careClicked(action) {
        if (root.canCare)
            root.perform("pet.care", {action: action}, action)
    }
    function stopRoutine() {
        root.routineGeneration += 1
        root.routineRunning = false
        root.routineSteps = []
        routineNext.stop()
    }
    function startRoutine() {
        if (!root.canCare)
            return
        var steps = []
        if (root.meters.hunger < 10000) steps.push("food")
        if (root.meters.thirst < 10000) steps.push("water")
        if (root.meters.mental_health < 10000) steps.push("play")
        if (root.care.bathroom_due) steps.push("bathroom")
        else if (root.meters.hygiene < 10000) steps.push("wash")
        if (steps.length === 0) {
            root.feedback = "All four bars are full. Care grants no XP."
            return
        }
        root.routineGeneration += 1
        root.routineRunning = true
        root.routineSteps = steps
        root.nextCare()
    }
    function nextCare() {
        if (!root.routineRunning || !root.presentationActive)
            return
        if (root.hostBusy) {
            root.stopRoutine()
            root.feedback = "Routine stopped while another request was busy. Confirmed care is kept; start the routine again for remaining bars."
            return
        }
        if (root.sleeping) {
            root.stopRoutine()
            root.feedback = "Routine stopped because the pet is now asleep."
            return
        }
        if (root.routineSteps.length === 0) {
            root.routineRunning = false
            root.feedback = "Care routine complete. Check readiness before choosing a task."
            return
        }
        var action = root.routineSteps[0]
        root.routineSteps = root.routineSteps.slice(1)
        var generation = root.routineGeneration
        root.perform("pet.care", {action: action}, action, function (ok) {
            if (ok && root.routineRunning && generation === root.routineGeneration)
                routineNext.restart()
        })
    }

    Timer {
        objectName: "petCare.animationTimer"
        interval: 200 // 5 FPS maximum, including care poses; no smooth animations.
        repeat: true
        running: root.animationRunning
        onTriggered: root.frame = (root.frame + 1) % 10
    }
    Timer {
        id: reactionEnd
        interval: 1400
        onTriggered: root.reaction = ""
    }
    Timer {
        id: routineNext
        interval: 1600 // Deliberate pause lets the user stop between interactions.
        onTriggered: root.nextCare()
    }
    Timer {
        objectName: "petCare.countdownTimer"
        interval: 1000
        repeat: true
        running: root.presentationActive && root.sleeping && root.remainingMs > 0
        onTriggered: root.countdownNow = Date.now() // display only; no request or persistence
    }

    Item {
        Layout.fillWidth: true
        Layout.preferredHeight: 120
        PetSprite {
            anchors.centerIn: parent
            width: 112; height: 112
            pose: root.sleeping ? "sleep" : root.reaction || "idle"
            frame: root.frame
        }
        Button {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            text: root.sleeping ? "Wake / settle" : "Bed · 2 min"
            enabled: root.canSleep
            onClicked: root.perform(root.sleeping ? "pet.wake" : "pet.sleep", {}, root.sleeping ? "wake" : "sleep")
        }
    }
    Label {
        objectName: "petCare.readiness"
        Layout.fillWidth: true
        text: root.readinessText
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        font.bold: true
        Accessible.name: text
    }
    Label {
        Layout.fillWidth: true
        visible: root.sleeping || !!(root.care && root.care.sleep_due)
        text: root.sleeping
            ? (root.remainingMs > 0 ? "Resting · about " + root.timeLeft(root.remainingMs) + " left" : "Rest may be complete. Refresh or Wake to confirm saved readiness.")
            : (root.taskState !== "" ? "Nap after this task. " : "Needs a nap. ") + "Put to bed for " + root.timeLeft(root.remainingMs) + "."
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
    }
    Label {
        Layout.fillWidth: true
        visible: root.sleeping
        text: "Early wake keeps the remaining rest requirement. You can still inspect and accept a review. Sleep never starts work."
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
    }
    Label {
        objectName: "petCare.feedback"
        Layout.fillWidth: true
        text: root.feedback
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        Accessible.name: text
    }
    Label {
        Layout.fillWidth: true
        visible: root.careTrail.length > 0
        text: "Recent care in this view:\n" + root.careTrail.join("\n")
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
    }
    Flow {
        Layout.fillWidth: true
        spacing: 8
        Button { objectName: "petCare.food"; text: "Offer food"; enabled: root.canCare; onClicked: root.careClicked("food") }
        Button { objectName: "petCare.water"; text: "Fill water"; enabled: root.canCare; onClicked: root.careClicked("water") }
        Button { objectName: "petCare.play"; text: "Pet / play"; enabled: root.canCare; onClicked: root.careClicked("play") }
        Button { objectName: "petCare.wash"; text: root.care && root.care.bathroom_due ? "Bathroom + wash" : "Wash"; enabled: root.canCare; onClicked: root.careClicked(root.care.bathroom_due ? "bathroom" : "wash") }
        Button { objectName: "petCare.routine"; text: root.care && root.care.welcome_care_available ? "Welcome care routine" : "Care routine"; enabled: root.canCare; onClicked: root.startRoutine() }
        Button { objectName: "petCare.stopRoutine"; text: "Stop routine"; visible: root.routineRunning; onClicked: { root.stopRoutine(); root.feedback = "Routine stopped. Confirmed care is kept." } }
    }
    RowLayout {
        CheckBox { objectName: "petCare.reducedMotion"; text: "Reduced motion"; checked: root.reducedMotion; onToggled: root.reducedMotion = checked }
        CheckBox { text: "Quiet mode"; checked: root.quietMode; onToggled: root.quietMode = checked }
    }
    Label {
        Layout.fillWidth: true
        text: "Care fills bars to 100 and grants no XP. Sound is off. No absence penalties."
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
    }
}
