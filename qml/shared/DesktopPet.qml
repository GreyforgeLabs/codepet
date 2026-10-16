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
// Home". There is no menu, no meter and no text on the desktop.
//
// Timers: one cosmetic scheduler (<= 5 updates per second, a single long wake during idle
// pauses, stopped while hidden, asleep or in Calm mode except to finish a short
// transient) and one single-shot `wake` timer armed for the face's `wake_at_ms` (the next
// moment the face can change without a new export, SPEC-v0.3 §8 rule 4). Neither runs
// while the pet is not drawn.
Item {
    id: root
    clip: true
    // The last valid bar-state.json (qml/omarchy/BarState.js parse), or null.
    property var barState: null
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
    property bool facingLeft: false
    property real destination: 40
    // 200 ms ticks of stillness left before the next walk (or in-tank swim).
    property int idleTicks: 60
    property bool walking: false
    property int swimTicksLeft: 0
    // Idle beats: a loop plays once, then the timer sleeps for the pause.
    property bool resting: false
    property int baseElapsedMs: 0
    // A one-shot state (sleep_enter/exit, celebrate) over the face's animation.
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
    readonly property bool canRoam: anim.roam
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
    onFixedNowMsChanged: refreshClock()
    onActiveChanged: {
        if (!active) {
            stopMoving()
            clearTransients()
            heartMsLeft = 0
            hopOffset = 0
            hop.stop()
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
        }
        updateInterval()
    }
    onPetIdChanged: { if (!roamsSpecies) stopMoving(); updateInterval() }
    onCanRoamChanged: if (!canRoam) stopMoving()

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
        root.face = PetState.sharedPriority(OverlayFace.withoutGrowth(root.barState, root.grewSeq), root.nowMs, root.seen)
        updatingFace = false
        observeFace()
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
        clickCount += 1
        stopMoving()
        resting = false
        idleTicks = 60
        heartMsLeft = 1600
        if (!reducedMotion) hop.restart()
        updateInterval()
        homeRequested()
        return true
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
    Item {
        id: pet
        objectName: "desktopPet.pet"
        x: root.petX; y: root.floorY + root.hopOffset + root.bobOffset
        width: root.petSize; height: root.petSize
        visible: root.active
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
            acceptedButtons: Qt.LeftButton
            cursorShape: Qt.PointingHandCursor
            onClicked: root.petClicked()
        }
        Accessible.role: Accessible.Button
        Accessible.name: root.petName + ". Open CodePet"
        Accessible.onPressAction: root.petClicked()
    }
    // Picture-only thought bubble; decorative and outside the input region.
    Rectangle {
        id: bubble
        objectName: "desktopPet.bubble"
        readonly property int icon: 40
        visible: root.active && root.bubbleIcons.length > 0
        width: icons.implicitWidth + 16; height: icon + 12; radius: height / 2
        x: Math.max(0, Math.min(root.width - width, pet.x + root.hitBox.x + root.hitBox.width - width / 3))
        y: Math.max(0, pet.y + root.hitBox.y - height - 6)
        color: "#f2ffffff"; border.color: "#33000000"; border.width: 1
        Row {
            id: icons
            anchors.centerIn: parent
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
    }
}
