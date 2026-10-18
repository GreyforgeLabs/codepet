pragma ComponentBehavior: Bound
import QtQuick
import "PetCatalog.js" as PetCatalog
import "PetStateMachine.js" as PetStateMachine
import "petstate.js" as PetState
import "OverlayFace.js" as OverlayFace

// The desktop pet (SPEC-v0.3 §3.8). Presentation only: it draws the parsed
// bar-state.json it is given (`barState`) and never sends a request. petstate.js picks the
// face in the one shared order (the bar cell and Home use the same order); this scene
// turns it into an animation, a picture-only thought bubble and a roaming pet.
//
// A click always reacts (K5): a short happy hop (in Calm mode only the bubble) with a
// heart bubble, then `homeRequested()`, which the host turns into "open the bar popup at
// Home". Drag to move; right-click for care and an explicit Pin/Roam choice.
//
// Timers: one cosmetic scheduler (<= 5 updates per second, a single long wake during idle
// pauses, stopped while hidden, asleep or in Calm mode except to finish a short
// transient) and one single-shot `wake` timer armed for the face's `wake_at_ms` (the next
// moment the face can change without a new export, SPEC-v0.3 §8 rule 4). Neither runs
// while the pet is not drawn. A separate single-shot timeout expires feedback only
// after an explicit care interaction; it never polls state.
Item {
    id: root
    clip: true
    // The last valid bar-state.json (qml/omarchy/BarState.js parse), or null.
    property var barState: null
    property var workState: null
    property var workController: null
    property bool cardOpen: false
    property bool wheelOpen: false
    property bool careBusy: false
    property bool careResuming: false
    property string careMessage: ""
    onCareMessageChanged: if (careMessage !== "") careFeedbackTimer.restart()
    property int expressionCount: 0
    property bool cosmeticExpression: false
    property real wiggleAngle: 0
    readonly property rect cardHit: card.visible ? Qt.rect(card.x, card.y, card.width, card.height) : Qt.rect(0, 0, 0, 0)
    readonly property rect wheelHit: wheel.visible ? Qt.rect(wheel.x, wheel.y, wheel.width, wheel.height) : Qt.rect(0, 0, 0, 0)
    readonly property bool interactiveOpen: active && (card.visible || wheel.visible)
    property bool surfaceActive: true
    property bool reducedMotion: false
    // Test hook: a fixed clock (ms since the epoch); 0 = the real clock.
    property real fixedNowMs: 0
    property real nowMs: 0
    // What this scene already celebrated (petstate.js `seen`), and the growth it played.
    // Seeded from the first export drawn (OverlayFace.seenAtStart): a new overlay process
    // never replays a celebration it was started after.
    property var seen: ({ finished_seq: 0, hello_seq: 0, test_at_ms: 0 })
    property real grewSeq: -1
    property bool seeded: false
    property string playedCelebration: ""
    property var celebrationBubble: null
    property bool initialized: false
    property bool updatingFace: false
    property real petX: 40
    // -1 follows the floor until the user chooses a position. Pin stops wandering,
    // never manual dragging; placement belongs to the host, not the pet database.
    property real petY: -1
    property bool pinned: false
    property bool dragging: false
    property bool dragMoved: false
    property point pressPoint: Qt.point(0, 0)
    property point pressPosition: Qt.point(0, 0)
    property bool facingLeft: false
    property real destination: 40
    // 200 ms ticks of stillness left before the next walk (or in-tank swim).
    property int idleTicks: 60
    property bool walking: false
    property int swimTicksLeft: 0
    // Idle beats: a loop plays once, then the timer sleeps for the pause.
    property bool resting: false
    property int baseElapsedMs: 0
    // A one-shot care/expression state over the face's animation.
    property string transientState: ""
    property int transientElapsedMs: 0
    property int transientDurationMs: 0
    property var queuedTransient: null
    property int timerInterval: 200
    property bool sleepObserved: false
    property bool lastSleeping: false
    // Click feedback: the heart bubble's remaining time and the hop offset (px, <= 0).
    property int heartMsLeft: 0
    property real hopOffset: 0
    readonly property bool hopping: hop.running
    property int clickCount: 0

    signal homeRequested()
    signal updatesRequested()
    signal reviewRequested()
    signal placementRequested(real x, real y, bool pinned)
    signal careRequested(string action)

    // Set from `barState` by updateFace() (not bindings: a handler of barState must not
    // read values whose bindings have not caught up yet).
    property var life: null
    property string petId: ""
    property int stage: 1
    property string petName: ""
    readonly property bool roamsSpecies: PetCatalog.roams(petId)
    readonly property var gait: PetCatalog.locomotion(petId)
    readonly property string careSet: OverlayFace.careIconSet(PetCatalog.species(petId))

    // The face (petstate.js), recomputed by updateFace() whenever its inputs change.
    property var face: PetState.emptyFace()
    readonly property var anim: OverlayFace.animation(face.overlay)
    // Nothing is drawn while Off, before a pet exists, or for an unknown species.
    readonly property bool shown: face.overlay !== "none" && PetCatalog.isSpecies(petId)
    readonly property bool active: visible && surfaceActive && width > 0 && height > 0 && shown
    readonly property real petSize: Math.min(112, width, height)
    readonly property real floorY: Math.max(0, height - petSize - 12)
    readonly property bool sleeping: anim.state === "sleep"
    // Paused and fainted pets share resting art, but have different care actions.
    readonly property bool careSleeping: !!(barState && barState.care && barState.care.sleeping)
    readonly property bool carePaused: !!(life && life.paused)
    readonly property bool careFainted: !!(life && life.fainted)
    readonly property bool canRoam: anim.roam && !pinned && !dragging && !cardOpen && !wheelOpen
    readonly property string baseState: walking && canRoam ? "locomotion" : (anim.state === "" ? "idle" : anim.state)
    readonly property var baseMeta: PetCatalog.stateMeta(petId, baseState)
    readonly property var transientMeta: transientState !== "" ? PetCatalog.stateMeta(petId, transientState) : null
    readonly property string displayState: transientMeta && PetStateMachine.display(baseMeta, transientMeta) === "transient"
        ? transientState : baseState
    readonly property var displayMeta: PetCatalog.stateMeta(petId, displayState)
    readonly property int frame: displayState === transientState && transientState !== ""
        ? PetStateMachine.frameAt(displayMeta, Math.floor(transientElapsedMs / 200), reducedMotion)
        : PetStateMachine.frameAt(displayMeta, Math.floor(baseElapsedMs / 200), reducedMotion)
    readonly property bool displayMirrored: facingLeft && displayMeta.mirror === true
    // A gentle bob while an app needs you (none in Calm mode).
    readonly property real bobOffset: anim.bob && !reducedMotion && displayState === "waiting" && frame % 2 === 1 ? -3 : 0
    readonly property bool motionRunning: active && !reducedMotion && !sleeping
    readonly property bool schedulerRunning: active && (transientState !== "" || motionRunning || heartMsLeft > 0)

    // The picture bubble: the heart after a click, the celebration while it plays, else the face's.
    readonly property var bubbleIcons: !active ? []
        : heartMsLeft > 0 ? ["care-floor/heart.webp"]
        : transientState === "celebrate" && celebrationBubble !== null
            ? OverlayFace.bubbleIcons(celebrationBubble, careSet, false)
            : OverlayFace.bubbleIcons(face.bubble, careSet, face.state === "waiting")

    readonly property string observerLabel: heartMsLeft > 0 || transientState === "celebrate" ? ""
        : OverlayFace.observerLabel(face, PetState.CLIENT_NAMES)

    readonly property var hitBox: PetCatalog.hitRect(petId, petSize, displayMirrored, stage)
    readonly property rect petHit: active ? Qt.rect(pet.x + hitBox.x, pet.y + hitBox.y, hitBox.width, hitBox.height) : Qt.rect(0, 0, 0, 0)

    Component.onCompleted: {
        initialized = true
        clampPosition()
        refreshClock()
    }
    onWidthChanged: if (initialized) clampPosition()
    onHeightChanged: if (initialized) clampPosition()
    onBarStateChanged: refreshClock()
    onWorkStateChanged: refreshClock()
    onFixedNowMsChanged: refreshClock()
    onActiveChanged: {
        if (!active) {
            cardOpen = false
            wheelOpen = false
            careMessage = ""
            careFeedbackTimer.stop()
            stopMoving()
            clearTransients()
            heartMsLeft = 0
            hopOffset = 0
            hop.stop()
            wiggle.stop()
            wiggleAngle = 0
            updateInterval()
            armWake()
        } else {
            // Recomputes the face, plays what became due while hidden, arms the wake.
            // Deferred: `active` can flip while a new document is being bound.
            Qt.callLater(root.refreshClock)
        }
    }
    onReducedMotionChanged: {
        if (reducedMotion) {
            stopMoving()
            hop.stop()
            hopOffset = 0
            wiggle.stop()
            wiggleAngle = 0
        }
        updateInterval()
    }
    onPetIdChanged: {
        if (!roamsSpecies) stopMoving()
        hop.stop(); hopOffset = 0
        wiggle.stop(); wiggleAngle = 0
        updateInterval()
    }
    onCanRoamChanged: if (!canRoam) stopMoving()

    function savePlacement() {
        placementRequested(petX / Math.max(1, width - petSize),
                           (petY < 0 ? floorY : petY) / Math.max(1, height - petSize), pinned)
    }
    function restorePlacement(position) {
        if (dragging || !position) return
        pinned = position.pinned === true
        if (typeof position.x === "number" && typeof position.y === "number"
                && isFinite(position.x) && isFinite(position.y)
                && position.x >= 0 && position.x <= 1 && position.y >= 0 && position.y <= 1) {
            petX = position.x * Math.max(0, width - petSize)
            petY = position.y * Math.max(0, height - petSize)
            stopMoving()
            clampPosition()
        }
    }
    function togglePinned() {
        pinned = !pinned
        stopMoving()
        idleTicks = 60
        savePlacement()
    }
    function toggleWheel() {
        if (!active) return
        cardOpen = false
        wheelOpen = !wheelOpen
        if (wheelOpen && !careBusy) careMessage = ""
    }
    function express(petted) {
        if (!active || sleeping) return
        expressionCount += 1
        clearTransients()
        startTransient("celebrate", 1600)
        cosmeticExpression = true
        if (petted) heartMsLeft = 1600
        if (!reducedMotion && roamsSpecies) {
            wiggle.restart()
            if (petted) hop.restart()
        }
        updateInterval()
    }
    function chooseCare(action) {
        if (!active) return
        wheelOpen = false
        if (action === "pin") { togglePinned(); careMessage = ""; return }
        if (action === "pet") { express(true); return }
        if (careBusy) { careMessage = "One moment…"; return }
        if (action === "sleep" && careFainted && !careSleeping && !carePaused) {
            careMessage = "Give Snack and Drink to recover"
            return
        }
        careBusy = true
        careResuming = action === "sleep" && carePaused
        careMessage = "One moment…"
        careRequested(action === "sleep" && (careSleeping || carePaused) ? "wake" : action)
    }
    function careAnswered(action, response) {
        careBusy = false
        var resumed = careResuming
        careResuming = false
        if (!active) { careMessage = ""; return }
        if (!response || response.ok !== true) {
            careMessage = response && response.error && response.error.message
                ? String(response.error.message).slice(0, 180) : "Try again from Care"
            return
        }
        var changed = response.result && response.result.changed === true
        if (["food", "water", "play", "wash"].indexOf(action) >= 0) {
            careMessage = changed ? ({ food: "Yum!", water: "Refreshing!", play: "Wheee!", wash: "All clean!" })[action] : "Already feeling good"
            if (changed || action === "play") {
                if (transientState === action) clearTransients()
                startTransient(action, 0)
            }
        } else careMessage = action === "wake" ? (resumed ? "Resumed" : "Awake") : "Nap time"
    }
    function beginDrag(x, y) {
        wheelOpen = false
        dragging = true
        dragMoved = false
        stopMoving()
        hop.stop()
        hopOffset = 0
        pressPoint = Qt.point(x, y)
        pressPosition = Qt.point(petX, petY < 0 ? floorY : petY)
    }
    function moveDrag(x, y) {
        if (!dragging) return
        var dx = x - pressPoint.x, dy = y - pressPoint.y
        if (!dragMoved && dx * dx + dy * dy < 64) return
        dragMoved = true
        petX = pressPosition.x + dx
        petY = Math.max(0, pressPosition.y + dy)
        clampPosition()
    }
    function endDrag() {
        dragging = false
        idleTicks = 60
        if (dragMoved) savePlacement()
    }

    // Reads the clock and recomputes the face (also on the wake timer).
    function refreshClock() {
        root.nowMs = root.fixedNowMs > 0 ? root.fixedNowMs : Date.now()
        updateFace()
    }
    function updateFace() {
        // Re-entry (a new face can flip `active`, whose handler refreshes) is folded
        // into the outer call, which observes the final face.
        if (!initialized || updatingFace) return
        updatingFace = true
        if (!root.seeded && root.barState) {
            var start = OverlayFace.seenAtStart(root.barState)
            root.seen = start.seen
            root.grewSeq = start.grewSeq
            root.seeded = true
        }
        var l = root.barState && root.barState.life ? root.barState.life : null
        root.life = l
        root.petId = l ? String(l.appearance || "") : ""
        root.stage = l && typeof l.stage === "number" ? l.stage : 1
        root.petName = l && l.name ? String(l.name) : ""
        root.face = OverlayFace.withWork(PetState.currentPriority(OverlayFace.withoutGrowth(root.barState, root.grewSeq), root.nowMs, root.seen), root.workState)
        updatingFace = false
        observeFace()
    }
    function clampPosition() {
        if (width <= 0 || height <= 0) return
        petX = Math.max(0, Math.min(petX, Math.max(0, width - petSize)))
        if (petY >= 0) petY = Math.max(0, Math.min(petY, Math.max(0, height - petSize)))
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
    // Plays sleep transitions and new celebrations once, and arms the wake timer. Called on
    // every face change; the first face after startup plays no sleep transition.
    function observeFace() {
        if (!initialized) return
        if (sleepObserved && sleeping !== lastSleeping && active)
            startTransient(sleeping ? "sleep_enter" : "sleep_exit", 0)
        if (sleeping) stopMoving()
        lastSleeping = sleeping
        sleepObserved = shown
        var key = OverlayFace.celebrationKey(face.celebrate)
        if (face.state === "celebrate" && key !== "" && key !== playedCelebration && active) {
            playedCelebration = key
            cosmeticExpression = false
            celebrationBubble = face.bubble
            var c = face.celebrate
            startTransient("celebrate", 0)
            // Recorded once shown: the face moves on to the next row (growth stays
            // unseen in Rust until Home shows it; only this scene skips it now).
            if (c.kind === "grew") grewSeq = c.seq
            else seen = OverlayFace.markSeen(seen, c)
            updateFace()
            return
        }
        updateInterval()
        armWake()
    }
    function armWake() {
        var w = face.wake_at_ms
        if (!active || w === null || fixedNowMs > 0) {
            wakeTimer.stop()
            return
        }
        // A few ms past the edge: the face changes at wake_at_ms (half-open windows).
        wakeTimer.interval = Math.max(1, Math.min(2147483647, Math.ceil(w - Date.now()) + 5))
        wakeTimer.restart()
    }
    function clearTransients() {
        cosmeticExpression = false
        transientState = ""
        transientElapsedMs = 0
        transientDurationMs = 0
        queuedTransient = null
    }
    // Starts (or queues behind a non-interruptible one) a one-shot state.
    function startTransient(state, durationOverrideMs) {
        if (!active) return false
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
        celebrationBubble = null
        baseElapsedMs = 0
        resting = false
        if (next) startTransient(next.state, next.durationMs)
    }
    function startMoving() {
        if (!canRoam) return
        if (roamsSpecies) {
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
        if (!roamsSpecies) {
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
        if (heartMsLeft > 0) heartMsLeft = Math.max(0, heartMsLeft - dt)
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
            baseElapsedMs = 0
            idleTicks -= Math.round(dt / 200)
            if (idleTicks <= 0) {
                if (pinned && face.state === "idle" && !dragging && !cardOpen && !wheelOpen && !petMouse.containsMouse) {
                    idleTicks = randomPause()
                    express(false)
                    return
                }
                // The pause is over: walk, or (held by hover or a state that cannot
                // roam) one more beat followed by a whole new pause.
                if (petMouse.containsMouse || !canRoam) idleTicks = randomPause()
                else { resting = false; startMoving(); updateInterval(); return }
            }
            // A single-frame hold (needs_care, error) has no beat to play: keep resting.
            resting = PetStateMachine.frameCount(displayMeta) <= 1
            updateInterval()
            return
        }
        baseElapsedMs += dt
        if (baseElapsedMs >= PetStateMachine.cycleMs(displayMeta) || PetStateMachine.frameCount(displayMeta) <= 1) {
            // Beat done: hold the first frame and sleep until the next beat or walk.
            baseElapsedMs = 0
            resting = !anim.bob
        }
        updateInterval()
    }
    function updateInterval() {
        var ms = 200
        if (transientState !== "") {
            ms = reducedMotion ? transientDurationMs - transientElapsedMs : PetStateMachine.frameMsOf(transientMeta)
        } else if (heartMsLeft > 0 && !motionRunning) {
            ms = heartMsLeft
        } else if (walking) {
            ms = 200
        } else if (resting) {
            // One wake for the whole remaining pause, bounded so a malformed pause range
            // cannot park the scene for long; a heart bubble still ends on time.
            ms = Math.min(60000, Math.max(200, idleTicks * 200))
            if (heartMsLeft > 0) ms = Math.min(ms, heartMsLeft)
        } else {
            ms = PetStateMachine.frameMsOf(displayMeta)
        }
        timerInterval = PetStateMachine.snapInterval(ms)
    }
    // A click on the pet: hop (bubble only in Calm mode), heart bubble, open Home. Never a
    // silent no-op while the pet is drawn (K5).
    function petClicked() {
        if (!active) return false
        wheelOpen = false
        clickCount += 1
        stopMoving()
        resting = false
        idleTicks = 60
        heartMsLeft = 1600
        if (!reducedMotion && roamsSpecies) hop.restart()
        updateInterval()
        if (workController) cardOpen = !cardOpen
        else homeRequested()
        return true
    }

    Timer {
        id: careFeedbackTimer
        objectName: "desktopPet.careFeedbackTimer"
        interval: 6000
        repeat: false
        onTriggered: root.careMessage = ""
    }
    Timer {
        id: cosmeticTimer
        objectName: "desktopPet.cosmeticTimer"
        interval: root.timerInterval; repeat: true; running: root.schedulerRunning
        onTriggered: root.cosmeticTick()
    }
    Timer {
        id: wakeTimer
        objectName: "desktopPet.wakeTimer"
        repeat: false
        onTriggered: root.refreshClock()
    }
    SequentialAnimation {
        id: hop
        NumberAnimation { target: root; property: "hopOffset"; to: -18; duration: 150; easing.type: Easing.OutQuad }
        NumberAnimation { target: root; property: "hopOffset"; to: 0; duration: 250; easing.type: Easing.OutBounce }
    }
    SequentialAnimation {
        id: wiggle
        NumberAnimation { target: root; property: "wiggleAngle"; to: -6; duration: 150; easing.type: Easing.InOutSine }
        NumberAnimation { target: root; property: "wiggleAngle"; to: 6; duration: 220; easing.type: Easing.InOutSine }
        NumberAnimation { target: root; property: "wiggleAngle"; to: -3; duration: 180; easing.type: Easing.InOutSine }
        NumberAnimation { target: root; property: "wiggleAngle"; to: 0; duration: 180; easing.type: Easing.InOutSine }
    }
    Item {
        id: pet
        objectName: "desktopPet.pet"
        x: root.petX; y: (root.petY < 0 ? root.floorY : root.petY) + root.hopOffset + root.bobOffset
        width: root.petSize; height: root.petSize
        visible: root.active
        PetSprite {
            id: sprite
            objectName: "desktopPet.sprite"
            anchors.fill: parent
            petId: root.petId
            stage: root.stage
            semanticState: root.displayState
            // The happy face is shared art; the party frame belongs to real growth.
            frame: root.cosmeticExpression && root.displayState === "celebrate" ? 1 : root.frame
            facingLeft: root.facingLeft
            reducedMotion: root.reducedMotion
            rotation: root.wiggleAngle
            opacity: root.face.state === "paused" ? 0.85 : 1
        }
        // Input covers the manifest hit box only, matching the host's input mask.
        MouseArea {
            id: petMouse
            objectName: "desktopPet.mouse"
            x: root.hitBox.x; y: root.hitBox.y
            width: root.hitBox.width; height: root.hitBox.height
            hoverEnabled: true
            enabled: root.active
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            cursorShape: root.dragging ? Qt.ClosedHandCursor : Qt.OpenHandCursor
            onPressed: mouse => {
                if (mouse.button !== Qt.LeftButton) return
                var p = mapToItem(root, mouse.x, mouse.y)
                root.beginDrag(p.x, p.y)
            }
            onPositionChanged: mouse => {
                if (!root.dragging) return
                var p = mapToItem(root, mouse.x, mouse.y)
                root.moveDrag(p.x, p.y)
            }
            onReleased: mouse => {
                if (mouse.button === Qt.LeftButton) root.endDrag()
            }
            onCanceled: root.endDrag()
            onClicked: mouse => {
                if (mouse.button === Qt.RightButton) root.toggleWheel()
                else if (!root.dragMoved) root.petClicked()
            }
        }
        Accessible.role: Accessible.Button
        Accessible.name: root.petName + ". Click to open, drag to move, right-click for care"
        Accessible.onPressAction: root.petClicked()
    }
    Rectangle {
        visible: root.active && root.pinned
        x: Math.max(0, Math.min(root.width - width, pet.x + (pet.width - width) / 2))
        y: Math.min(root.height - height, pet.y + pet.height - 6)
        width: pinLabel.implicitWidth + 14; height: 22; radius: 8
        color: "#ee22303b"
        Text { id: pinLabel; anchors.centerIn: parent; text: "Pinned"; font.pixelSize: 12; color: "white" }
    }
    // Decorative, outside the input region; observed client activity names its source.
    Rectangle {
        id: bubble
        objectName: "desktopPet.bubble"
        readonly property int icon: root.observerLabel !== "" ? 24 : 40
        visible: root.active && !root.cardOpen && !careFeedback.visible && root.bubbleIcons.length > 0
        width: Math.min(root.width, icons.implicitWidth + (observerText.visible ? observerText.width + 6 : 0) + 16)
        height: icon + 12; radius: height / 2
        x: Math.max(0, Math.min(root.width - width, pet.x + root.hitBox.x + root.hitBox.width - width / 3))
        y: root.observerLabel !== "" ? Math.max(0, pet.y >= height + 6 ? pet.y - height - 6
            : Math.min(root.height - height, pet.y + pet.height + 6))
            : Math.max(0, pet.y + root.hitBox.y - height - 6)
        Accessible.role: Accessible.StaticText
        Accessible.name: root.observerLabel
        Accessible.description: root.observerLabel !== "" ? "Watching a separate coding session." : ""
        color: "#f2ffffff"; border.color: "#33000000"; border.width: 1
        Row {
            id: icons
            anchors.verticalCenter: parent.verticalCenter
            x: 8
            spacing: 4
            Repeater {
                model: root.bubbleIcons
                Image {
                    required property string modelData
                    width: bubble.icon; height: bubble.icon
                    source: Qt.resolvedUrl("../assets/icons/" + modelData)
                    sourceSize: Qt.size(bubble.icon * 2, bubble.icon * 2)
                    fillMode: Image.PreserveAspectFit
                    asynchronous: false
                    smooth: true
                }
            }
        }
        Text {
            id: observerText
            objectName: "desktopPet.observerLabel"
            visible: root.observerLabel !== ""
            x: icons.x + icons.width + 6
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, Math.max(0, Math.min(260, root.width) - icons.width - 22))
            text: root.observerLabel
            textFormat: Text.PlainText
            font.pixelSize: 12; color: "#24303c"
            elide: Text.ElideRight
        }
    }
    // Feedback remains visible after quick care closes its wheel. This is decorative:
    // it never expands the input region or takes focus from the user's work.
    Rectangle {
        id: careFeedback
        objectName: "desktopPet.careFeedback"
        visible: root.active && !root.cardOpen && !root.wheelOpen && root.careMessage !== ""
        width: Math.min(root.width, 230)
        height: careFeedbackText.implicitHeight + 16
        x: Math.max(0, Math.min(root.width - width, pet.x + (pet.width - width) / 2))
        y: Math.max(0, Math.min(root.height - height, pet.y >= height + 8
            ? pet.y - height - 8 : pet.y + root.petSize + 8))
        radius: 12
        color: "#f2232b36"
        border.color: "#897d8b9b"
        Accessible.role: Accessible.StaticText
        Accessible.name: root.careMessage
        Accessible.description: "Care status"
        Accessible.ignored: !visible
        readonly property string announcement: visible ? root.careMessage : ""
        onAnnouncementChanged: if (announcement !== "") Accessible.announce(announcement, Accessible.Polite)
        Text {
            id: careFeedbackText
            objectName: "desktopPet.careFeedbackText"
            x: 10; y: 8; width: parent.width - 20
            text: root.careMessage
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            font.pixelSize: 12; color: "#f0f4fa"
            horizontalAlignment: Text.AlignHCenter
            Accessible.ignored: true
        }
    }
    Rectangle {
        objectName: "desktopPet.workBadge"
        visible: root.active && !root.cardOpen && !!(root.workState && root.workState.task
            && (root.workState.task.status === "review" || root.workState.task.status === "applying"
                || (root.workState.task.status === "scouted" && root.workState.task.has_finding)))
        x: Math.max(0, Math.min(root.width - width, pet.x + root.hitBox.x + root.hitBox.width - width))
        y: Math.max(0, pet.y + root.hitBox.y)
        width: 18; height: 18; radius: 9; color: "#eab276"; border.color: "#2b303a"; border.width: 2
        Text { anchors.centerIn: parent; text: "!"; font.bold: true; font.pixelSize: 12; color: "#302719" }
    }
    WorkCard {
        id: card
        objectName: "desktopPet.workCard"
        visible: root.active && root.cardOpen && !!root.workController
        onVisibleChanged: if (visible) Qt.callLater(() => { if (card.visible) card.focusInitial() })
        controller: root.workController
        meters: root.barState ? root.barState.meters : null
        width: Math.min(implicitWidth, root.width)
        height: Math.min(implicitHeight, root.height)
        x: Math.max(0, Math.min(root.width - width, pet.x + root.petSize / 2 - width / 2))
        y: Math.max(0, Math.min(root.height - height, pet.y >= height + 8 ? pet.y - height - 8 : pet.y + root.petSize + 8))
        onCloseRequested: root.cardOpen = false
        onHomeRequested: { root.cardOpen = false; root.homeRequested() }
        onUpdatesRequested: { root.cardOpen = false; root.updatesRequested() }
        onReviewRequested: root.reviewRequested()
    }
    CareWheel {
        id: wheel
        objectName: "desktopPet.wheel"
        visible: root.active && root.wheelOpen
        onVisibleChanged: if (visible) Qt.callLater(() => { if (wheel.visible) wheel.focusInitial() })
        width: Math.min(implicitWidth, root.width)
        height: Math.min(implicitHeight, root.height)
        x: Math.max(0, Math.min(root.width - width, pet.x + root.petSize / 2 - width / 2))
        y: Math.max(0, Math.min(root.height - height, pet.y >= height + 8 ? pet.y - height - 8 : pet.y + root.petSize + 8))
        pinned: root.pinned
        sleeping: root.careSleeping
        paused: root.carePaused
        fainted: root.careFainted
        careSet: root.careSet
        message: root.careMessage
        onChosen: action => root.chooseCare(action)
        onCloseRequested: root.wheelOpen = false
    }
}
