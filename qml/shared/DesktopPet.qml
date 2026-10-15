pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import "PetCatalog.js" as PetCatalog
import "PetStateMachine.js" as PetStateMachine

// Portable presentation only. The host/Rust snapshot owns all care, task and life
// state. This component never starts tasks and never refreshes from a timer. Its one
// Timer is the scene's cosmetic scheduler: ≤ 5 updates per second, a long single
// wake during idle pauses, and stopped while hidden, under reduced motion or asleep
// (except to finish a short transient such as falling asleep or a care reaction).
Item {
    id: root
    required property var host
    clip: true
    Keys.onEscapePressed: selected = false
    property bool surfaceActive: true
    property bool reducedMotion: false
    property bool selected: false
    property bool initialized: false
    property real petX: 40
    property bool facingLeft: false
    property real destination: 40
    // 200 ms ticks of stillness left before the next walk (or in-tank swim).
    property int idleTicks: 60
    property bool walking: false
    property int swimTicksLeft: 0
    // Idle beats: a loop plays once, then the timer sleeps for the pause.
    property bool resting: false
    property int baseElapsedMs: 0
    // A one-shot overlay state (sleep_enter/exit, confirmed care, celebrate, error).
    property string transientState: ""
    property int transientElapsedMs: 0
    property int transientDurationMs: 0
    property var queuedTransient: null
    property int timerInterval: 200
    // Growth celebrations already shown / acknowledged by this scene (by event seq).
    property real celebratedThroughSeq: -1
    property real seenSentThroughSeq: -1
    property real seenRequestedSeq: -1
    property string seenRetryKey: ""
    property string selectRetryKey: ""
    property string selectRetryPet: ""
    property int requestSequence: 0
    property bool sleepObserved: false
    property bool lastSleeping: false

    readonly property var snap: host && host.snapshot ? host.snapshot : null
    readonly property var life: snap && snap.life ? snap.life : null
    // v0.1 snapshots have no `life` key: that is the one migrated cat.
    readonly property bool legacySnapshot: snap !== null && !Object.prototype.hasOwnProperty.call(snap, "life")
    readonly property bool hasLife: legacySnapshot || life !== null
    readonly property string petId: life ? String(life.appearance || life.species || "") : legacySnapshot ? "cat" : ""
    readonly property int stage: life && typeof life.stage === "number" ? life.stage : 1
    readonly property string petName: life && life.name ? String(life.name) : "CodePet"
    readonly property bool aquatic: PetCatalog.isAquatic(petId)
    readonly property bool roams: PetCatalog.roams(petId)
    readonly property var gait: PetCatalog.locomotion(petId)

    readonly property bool active: visible && surfaceActive && width > 0 && height > 0
    readonly property real petSize: Math.min(112, width, height)
    readonly property real floorY: Math.max(0, height - petSize - 12)
    readonly property bool hovered: petMouse.containsMouse
    readonly property bool sleeping: controller.sleeping
    readonly property string pendingCare: PetStateMachine.isCareAction(controller.pendingAction) ? controller.pendingAction : ""
    readonly property var facts: PetStateMachine.facts(snap, walking)
    readonly property string baseState: snap === null ? "idle" : PetStateMachine.baseState(facts)
    readonly property var baseMeta: PetCatalog.stateMeta(petId, baseState)
    readonly property var transientMeta: transientState !== "" ? PetCatalog.stateMeta(petId, transientState) : null
    readonly property var pendingMeta: pendingCare !== "" ? PetCatalog.stateMeta(petId, pendingCare) : null
    // Shown state: a pending care holds its approach frame; a transient shows unless
    // the base state may interrupt it (SPEC §11.2 priority/interrupt rules).
    readonly property string displayState: baseState === "memorial" ? "memorial"
        : pendingMeta && PetStateMachine.display(baseMeta, pendingMeta) === "transient" ? pendingCare
        : transientMeta && PetStateMachine.display(baseMeta, transientMeta) === "transient" ? transientState
        : baseState
    readonly property var displayMeta: PetCatalog.stateMeta(petId, displayState)
    readonly property int frame: displayState === pendingCare && pendingCare !== "" ? 0
        : displayState === transientState && transientState !== ""
            ? PetStateMachine.frameAt(displayMeta, Math.floor(transientElapsedMs / 200), reducedMotion)
            : PetStateMachine.frameAt(displayMeta, Math.floor(baseElapsedMs / 200), reducedMotion)
    readonly property bool displayMirrored: facingLeft && displayMeta.mirror === true
    // Legacy name kept for hosts/tests written against v0.1.
    readonly property string pose: displayState
    // A memorial or a not-yet-loaded snapshot has nothing to animate.
    readonly property bool motionRunning: active && !reducedMotion && !sleeping && pendingCare === ""
        && snap !== null && baseState !== "memorial"
    readonly property bool schedulerRunning: active && (transientState !== "" || motionRunning)
    readonly property var unseenEvents: snap && Array.isArray(snap.unseen_events)
        ? snap.unseen_events.filter(function (e) { return e && typeof e.seq === "number" && (e.kind === undefined || e.kind === "evolved") })
        : []
    readonly property bool growthBadge: life !== null && unseenEvents.length > 0 && transientState !== "celebrate"
    readonly property bool canSelectLook: life !== null && !life.death_pending && !hostBusy
        && !(host && host.presentationLocks > 0)
    readonly property bool hostBusy: !!(host && host.busy)
    readonly property string statusText: {
        var line = PetStateMachine.statusLine(snap)
        if (line !== "") return line
        return sleeping ? "RESTING · no work starts automatically" : controller.readinessText
    }

    readonly property var hitBox: PetCatalog.hitRect(baseState === "memorial" ? "" : petId, petSize, displayMirrored)
    readonly property rect petHit: active ? Qt.rect(pet.x + hitBox.x, pet.y + hitBox.y, hitBox.width, hitBox.height) : Qt.rect(0, 0, 0, 0)
    readonly property rect menuHit: active && selected ? Qt.rect(menu.x, menu.y, menu.width, menu.height) : Qt.rect(0, 0, 0, 0)
    // Props are drawn inside the frames now; kept so the host mask contract is unchanged.
    readonly property rect propsHit: Qt.rect(0, 0, 0, 0)
    signal reducedMotionRequested(bool enabled)
    signal taskPanelRequested()
    signal hideRequested()
    signal chooserRequested()
    signal adoptRequested()

    Component.onCompleted: { initialized = true; clampPosition(); observeSnapshot() }
    component SceneButton: Button {
        id: control
        background: Rectangle { radius: 8; color: control.down ? "#425271" : control.hovered ? "#34425c" : "#29364b"; opacity: control.enabled ? 1 : 0.4 }
        contentItem: Text { text: control.text; color: control.enabled ? "#edf2fa" : "#8090a4"; font.pixelSize: 11; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter; elide: Text.ElideRight }
    }
    onWidthChanged: if (initialized) clampPosition()
    onHeightChanged: if (initialized) clampPosition()
    onActiveChanged: {
        if (!active) {
            selected = false
            stopMoving()
            clearTransients()
        }
        updateInterval()
    }
    onReducedMotionChanged: { if (reducedMotion) stopMoving(); updateInterval() }
    onSelectedChanged: if (selected) stopMoving()
    onSnapChanged: observeSnapshot()
    onPetIdChanged: { if (!roams) stopMoving(); updateInterval() }
    onPendingCareChanged: { if (pendingCare !== "") stopMoving(); updateInterval() }

    function nextKey(prefix) {
        root.requestSequence += 1
        return prefix + "-" + Date.now().toString(36) + "-" + root.requestSequence
    }
    function clampPosition() {
        if (width <= 0 || height <= 0) return
        petX = Math.max(0, Math.min(petX, Math.max(0, width - petSize)))
        destination = Math.max(0, Math.min(destination, Math.max(0, width - petSize)))
    }
    function stopMoving() {
        if (walking) {
            walking = false
            baseElapsedMs = 0
        }
        swimTicksLeft = 0
    }
    function randomPause() {
        var range = root.gait.pauseTicks
        return range[0] + Math.floor(Math.random() * (range[1] - range[0] + 1))
    }
    // Sleep transitions are compared against the previous snapshot, so the first
    // snapshot after startup never plays a transition.
    function observeSnapshot() {
        if (!snap) return
        var now = !!(snap.care && snap.care.sleeping)
        if (now) stopMoving()
        if (sleepObserved && now !== lastSleeping && active)
            startTransient(now ? "sleep_enter" : "sleep_exit", 0)
        lastSleeping = now
        sleepObserved = true
        updateInterval()
    }
    function clearTransients() {
        transientState = ""
        transientElapsedMs = 0
        transientDurationMs = 0
        queuedTransient = null
    }
    // Starts (or queues behind a non-interruptible one) a one-shot overlay state.
    function startTransient(state, durationOverrideMs) {
        if (!active || baseState === "memorial") return false
        var meta = PetCatalog.stateMeta(petId, state)
        var duration = durationOverrideMs > 0 ? durationOverrideMs : PetStateMachine.durationMs(meta)
        if (!isFinite(duration)) duration = PetStateMachine.cycleMs(meta)
        if (transientState !== "" && PetStateMachine.admit(transientMeta, meta) === "queue") {
            queuedTransient = { state: meta.name, durationMs: duration }
            return true
        }
        stopMoving()
        resting = false
        transientState = meta.name
        transientElapsedMs = 0
        transientDurationMs = duration
        updateInterval()
        return true
    }
    function finishTransient() {
        var next = queuedTransient
        clearTransients()
        baseElapsedMs = 0
        resting = false
        if (next) startTransient(next.state, next.durationMs)
    }
    function startMoving() {
        if (roams) {
            var span = 70 + Math.random() * 150
            destination = Math.max(0, Math.min(width - petSize, petX + (Math.random() < 0.5 ? -1 : 1) * span))
            facingLeft = destination < petX
            walking = Math.abs(destination - petX) > 2
        } else {
            // Aquatic: an in-tank swim beat; the habitat never moves.
            swimTicksLeft = 8
            walking = true
        }
        baseElapsedMs = 0
        if (!walking) idleTicks = randomPause()
    }
    function stepMovement() {
        var meta = PetCatalog.stateMeta(petId, "locomotion")
        var f = PetStateMachine.frameAt(meta, Math.floor(baseElapsedMs / 200), false)
        baseElapsedMs += 200
        if (!roams) {
            swimTicksLeft -= 1
            if (swimTicksLeft <= 0) {
                walking = false
                baseElapsedMs = 0
                idleTicks = randomPause()
            }
            return
        }
        var airborne = gait.airborneFrames
        var px = airborne.length > 0 && airborne.indexOf(f) < 0 ? 0 : Math.max(1, gait.pxPerTick)
        var distance = destination - petX
        petX += Math.max(-px, Math.min(px, distance))
        clampPosition()
        if (Math.abs(destination - petX) < 1) {
            walking = false
            baseElapsedMs = 0
            idleTicks = randomPause()
        }
    }
    // One scheduler step. `cosmeticTimer.interval` ms have elapsed since the last one.
    function cosmeticTick() {
        if (!active) return
        var dt = Math.max(200, root.timerInterval)
        if (transientState !== "") {
            transientElapsedMs += dt
            if (transientElapsedMs >= transientDurationMs) finishTransient()
            updateInterval()
            return
        }
        if (!motionRunning) { updateInterval(); return }
        if (walking) {
            stepMovement()
            updateInterval()
            return
        }
        if (resting) {
            resting = false
            baseElapsedMs = 0
            idleTicks -= Math.round(dt / 200)
            if (idleTicks <= 0) {
                if (selected || hovered || !PetStateMachine.canRoam(facts)) idleTicks = 25
                else startMoving()
            }
            updateInterval()
            return
        }
        baseElapsedMs += dt
        if (baseElapsedMs >= PetStateMachine.cycleMs(displayMeta)) {
            // Beat done: hold the first frame and sleep until the next beat or walk.
            baseElapsedMs = 0
            resting = true
        }
        updateInterval()
    }
    function updateInterval() {
        var ms = 200
        if (transientState !== "") {
            ms = reducedMotion ? transientDurationMs - transientElapsedMs : PetStateMachine.frameMsOf(transientMeta)
        } else if (walking) {
            ms = 200
        } else if (resting) {
            ms = Math.min(8000, Math.max(200, idleTicks * 200))
        } else {
            ms = PetStateMachine.frameMsOf(displayMeta)
        }
        timerInterval = PetStateMachine.snapInterval(ms)
    }
    function requestCare(action) {
        if (!active || !selected || !hasLife) return
        stopMoving()
        idleTicks = 60
        if (action === "sleep" || action === "wake") {
            if (controller.canSleep)
                controller.perform(action === "wake" ? "pet.wake" : "pet.sleep", {}, action)
        } else controller.careClicked(action)
    }
    function eventsMaxSeq(events) {
        var max = -1
        for (var i = 0; i < events.length; i++)
            if (events[i] && typeof events[i].seq === "number" && events[i].seq > max) max = events[i].seq
        return max
    }
    // Celebrates unseen growth once and acknowledges it once. Called on a click on
    // the pet, and after this UI's own task acceptance.
    function acknowledgeGrowth(events) {
        var list = Array.isArray(events) ? events : unseenEvents
        var max = eventsMaxSeq(list)
        if (max < 0 || !life) return false
        if (max > celebratedThroughSeq) {
            celebratedThroughSeq = max
            startTransient("celebrate", 0)
        }
        sendEventsSeen(max)
        return true
    }
    function celebrateFromAcceptance(result) {
        var events = result && Array.isArray(result.unseen_events) ? result.unseen_events
            : result && Array.isArray(result.evolved) ? result.evolved : unseenEvents
        return acknowledgeGrowth(events)
    }
    function sendEventsSeen(seq) {
        if (seq <= seenSentThroughSeq || !life || !host) return
        if (hostBusy) return // retried on the next explicit interaction
        if (seenRequestedSeq !== seq || seenRetryKey === "") {
            seenRequestedSeq = seq
            seenRetryKey = nextKey("seen")
        }
        var key = seenRetryKey
        host.request("pet.events_seen", {life_id: String(life.life_id), through_seq: seq}, function (response) {
            if (response && response.ok) {
                root.seenSentThroughSeq = Math.max(root.seenSentThroughSeq, seq)
                root.seenRetryKey = ""
            }
        }, key)
    }
    // Cosmetic look change: one pet.select request and nothing else. The new look
    // arrives through the snapshot (or the bar-state merge), never through a refresh.
    function selectLook(id) {
        var target = String(id || "")
        if (!canSelectLook || !PetCatalog.isSpecies(target) || target === petId) return false
        if (selectRetryKey === "" || selectRetryPet !== target) {
            selectRetryKey = nextKey("select")
            selectRetryPet = target
        }
        var key = selectRetryKey
        host.request("pet.select", {life_id: String(life.life_id), pet_id: target}, function (response) {
            if (response && response.ok) root.selectRetryKey = ""
        }, key)
        return true
    }
    function petClicked() {
        if (!active) return
        if (unseenEvents.length > 0) acknowledgeGrowth(unseenEvents)
        selected = !selected
    }

    Timer {
        id: cosmeticTimer
        objectName: "desktopPet.cosmeticTimer"
        interval: root.timerInterval; repeat: true; running: root.schedulerRunning
        onTriggered: root.cosmeticTick()
    }
    PetCare {
        id: controller
        objectName: "desktopPet.careController"
        host: root.host
        visible: false
        enabled: false
        reducedMotion: true
        quietMode: true
        showSprite: false
        onReactionSeqChanged: root.startTransient(controller.lastReaction, 0)
        onFailureSeqChanged: root.startTransient("error", 2000)
    }
    // A host may announce this UI's own confirmed acceptance; growth then celebrates
    // from that response (other UIs wait for an explicit click).
    Connections {
        target: root.host
        ignoreUnknownSignals: true
        function onTaskAccepted(result) { root.celebrateFromAcceptance(result) }
    }
    Item {
        id: pet
        objectName: "desktopPet.pet"
        x: root.petX; y: root.floorY
        width: root.petSize; height: root.petSize
        visible: root.active
        activeFocusOnTab: root.active
        Keys.onReturnPressed: root.petClicked()
        Keys.onSpacePressed: root.petClicked()
        PetSprite {
            id: sprite
            objectName: "desktopPet.sprite"
            anchors.fill: parent
            petId: root.petId
            stage: root.stage
            semanticState: root.displayState
            frame: root.frame
            facingLeft: root.facingLeft
            reducedMotion: root.reducedMotion
        }
        // Input covers the manifest hit box only, matching the host's input mask.
        MouseArea {
            id: petMouse
            x: root.hitBox.x; y: root.hitBox.y
            width: root.hitBox.width; height: root.hitBox.height
            hoverEnabled: true
            enabled: root.active
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            cursorShape: Qt.PointingHandCursor
            onClicked: root.petClicked()
        }
        Accessible.role: Accessible.Button
        Accessible.name: (root.hasLife ? root.petName + ". " : "No companion. ") + "Open care menu"
        Accessible.onPressAction: root.petClicked()
    }
    // Static growth badge; decorative and outside the input region.
    Text {
        objectName: "desktopPet.growthBadge"
        visible: root.active && root.growthBadge && !root.selected
        x: Math.max(0, Math.min(root.width - width, pet.x + root.hitBox.x + root.hitBox.width - width / 2))
        y: Math.max(0, pet.y + root.hitBox.y - height)
        text: "★ grew"
        color: "#f2c14e"; font.pixelSize: 11; font.bold: true
        textFormat: Text.PlainText
    }
    // Hover bars are informational and deliberately outside the input region.
    Rectangle {
        id: bars
        x: Math.max(0, Math.min(root.width - width, pet.x + pet.width / 2 - width / 2))
        y: Math.max(0, pet.y + root.hitBox.y - height - 4)
        width: Math.min(136, root.width); height: 31; radius: 10
        visible: root.active && root.hovered && !root.selected && root.hasLife
        color: "#ed171c29"; border.color: "#354157"
        Row {
            anchors.centerIn: parent; spacing: 5
            Repeater {
                model: ["hunger", "thirst", "mental_health", "hygiene"]
                Rectangle {
                    required property string modelData
                    width: 26; height: 5; radius: 2; color: "#364052"
                    Rectangle {
                        width: parent.width * Math.max(0, Math.min(1, Number(controller.meters[parent.modelData] || 0) / 10000))
                        height: parent.height; radius: 2; color: "#99d4ac"
                    }
                }
            }
        }
    }
    Rectangle {
        id: menu
        objectName: "desktopPet.menu"
        x: Math.max(0, Math.min(root.width - width, pet.x + pet.width / 2 - width / 2))
        y: Math.max(0, pet.y - height - 12)
        width: Math.min(260, root.width)
        height: Math.min(content.implicitHeight + 28, root.height)
        radius: 18; color: "#f519202e"; border.color: "#45516a"; border.width: 1
        visible: root.active && root.selected
        enabled: visible
        clip: true
        // Swallow empty menu-space clicks; nothing leaks to the underlying desktop.
        MouseArea { anchors.fill: parent }
        Flickable {
            anchors.fill: parent; anchors.margins: 14
            contentHeight: content.implicitHeight; clip: true
            boundsBehavior: Flickable.StopAtBounds
            Column {
                id: content
                width: parent.width; spacing: 9
                Row {
                    width: parent.width
                    Text {
                        width: parent.width - 36; elide: Text.ElideRight; textFormat: Text.PlainText
                        text: root.life ? root.petName + " · " + PetCatalog.stageName(root.petId, root.stage) : "CodePet"
                        color: "#f0f4fa"; font.pixelSize: 17; font.bold: true
                    }
                    SceneButton {
                        width: 32; height: 26; text: "×"
                        Accessible.name: "Close care menu"
                        onClicked: root.selected = false
                    }
                }
                Text {
                    objectName: "desktopPet.status"
                    width: parent.width; text: root.statusText
                    color: "#a8b9cf"; font.pixelSize: 11; wrapMode: Text.Wrap; textFormat: Text.PlainText
                    Accessible.name: text
                }
                Repeater {
                    model: root.hasLife ? [ { key: "hunger", label: "Nourishment", tint: "#e6bd8a" }, { key: "thirst", label: "Hydration", tint: "#8ecae5" }, { key: "mental_health", label: "Mood", tint: "#c3abe8" }, { key: "hygiene", label: "Hygiene", tint: "#97d4b0" } ] : []
                    Column {
                        id: meterRow
                        required property var modelData
                        width: content.width; spacing: 4
                        property real value: Math.max(0, Math.min(100, Number(controller.meters[modelData.key] || 0) / 100))
                        Row {
                            width: parent.width
                            Text { width: parent.width - 40; text: meterRow.modelData.label; color: "#c4d0e1"; font.pixelSize: 11 }
                            Text { width: 40; horizontalAlignment: Text.AlignRight; text: Math.round(meterRow.value) + "%"; color: "#c4d0e1"; font.pixelSize: 11 }
                        }
                        Rectangle {
                            width: parent.width; height: 4; radius: 2; color: "#354052"
                            Rectangle { width: parent.width * meterRow.value / 100; height: 4; radius: 2; color: meterRow.modelData.tint }
                        }
                    }
                }
                Grid {
                    columns: 2; spacing: 6
                    visible: root.hasLife
                    Repeater {
                        model: ["food", "water", "play", "wash", "bathroom", "sleep"]
                        SceneButton {
                            id: careButton
                            required property string modelData
                            objectName: "desktopPet.care." + modelData
                            width: (content.width - 6) / 2; height: 31
                            text: modelData === "sleep" && root.sleeping ? "Wake / settle" : PetCatalog.careLabel(root.petId, modelData)
                            enabled: menu.visible && (modelData === "sleep" ? controller.canSleep : controller.canCare)
                            Accessible.name: text
                            onClicked: root.requestCare(modelData === "sleep" && root.sleeping ? "wake" : modelData)
                        }
                    }
                }
                Text {
                    width: parent.width; text: controller.feedback; color: "#a8b9cf"; font.pixelSize: 10; wrapMode: Text.Wrap; textFormat: Text.PlainText
                    visible: root.hasLife
                }
                SceneButton {
                    objectName: "desktopPet.chooseLook"
                    width: parent.width; height: 31; text: "Choose look…"
                    visible: root.life !== null
                    enabled: menu.visible && root.canSelectLook
                    Accessible.name: "Choose look. Cosmetic only"
                    onClicked: { root.selected = false; root.chooserRequested() }
                }
                SceneButton {
                    objectName: "desktopPet.adopt"
                    width: parent.width; height: 31; text: "Adopt…"
                    visible: !root.hasLife && root.snap !== null
                    enabled: menu.visible
                    onClicked: {
                        root.selected = false
                        root.adoptRequested()
                        if (root.host && typeof root.host.openLife === "function") root.host.openLife()
                        else root.taskPanelRequested()
                    }
                }
                SceneButton {
                    width: parent.width; height: 33; text: "Open task panel · simulation"
                    enabled: menu.visible
                    onClicked: { root.selected = false; root.taskPanelRequested() }
                }
                Row {
                    width: parent.width; spacing: 4
                    CheckBox {
                        width: parent.width - 50; text: "Less motion"; checked: root.reducedMotion
                        onToggled: root.reducedMotionRequested(checked)
                        palette.windowText: "#bac8dc"
                    }
                    SceneButton {
                        width: 46; height: 32; text: "Hide"
                        onClicked: { root.selected = false; root.hideRequested() }
                    }
                }
            }
        }
    }
}
