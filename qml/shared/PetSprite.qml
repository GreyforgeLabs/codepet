import QtQuick
import QtQuick.Window
import "PetCatalog.js" as PetCatalog

// All animation timing belongs to the scene's single scheduler. This component is
// deliberately a passive renderer: no timers, animations, or backend calls. It sets
// only the current frame's URL, so only the selected species pack and stage load.
// A missing or broken file shows the neutral placeholder; nothing else happens.
Item {
    id: root
    implicitWidth: 112
    implicitHeight: 112
    property string petId: ""
    property int stage: 1
    // Semantic state from PetStateMachine ("idle", "locomotion", "food", …, "memorial").
    property string semanticState: "idle"
    // Legacy v0.1 name for semanticState ("walk" maps to locomotion).
    property alias pose: root.semanticState
    // Index into the state's explicit frame list (wraps).
    property int frame: 0
    property bool facingLeft: false
    property bool reducedMotion: false

    readonly property var resolved: PetCatalog.resolve(root.petId, root.stage, root.semanticState, root.frame, root.reducedMotion)
    readonly property string targetUrl: resolved.url
    // True after the current target failed to load (missing file, bad data).
    property bool broken: false
    readonly property bool showingPlaceholder: resolved.placeholder || broken
    readonly property bool mirrored: artwork.mirror
    readonly property url currentSource: artwork.source
    readonly property int loadStatus: artwork.status
    // Exposed for tests: the artwork never uses Qt's shared pixmap cache.
    readonly property bool pixmapCached: artwork.cache
    readonly property real canvasSize: 192

    onTargetUrlChanged: root.broken = false

    Image {
        id: artwork
        objectName: "petSpriteArtwork"
        anchors.fill: parent
        source: root.broken ? PetCatalog.placeholderUrl() : root.targetUrl
        sourceSize: Qt.size(root.decodeSize(), root.decodeSize())
        fillMode: Image.PreserveAspectFit
        mirror: root.facingLeft && root.resolved.mirror && !root.broken
        smooth: true
        mipmap: false
        asynchronous: false
        // No pixmap cache: only the frame on screen stays decoded, so at most the current
        // pack is resident however often looks change (spec §1 / issue #5 bounded cache).
        // Re-decoding one small WebP per ≤ 5 Hz frame change is cheap.
        cache: false
        // Deferred so the fallback never re-enters the synchronous load of `source`.
        // The placeholder itself failing leaves an empty frame; it never loops.
        onStatusChanged: if (status === Image.Error && !root.broken) Qt.callLater(root.fallBack, String(source))
    }

    function fallBack(failedUrl) {
        var target = String(root.targetUrl)
        if (!root.broken && artwork.status === Image.Error
                && (failedUrl === target || failedUrl === String(Qt.resolvedUrl(target))))
            root.broken = true
    }

    // Decode no larger than the canvas, and no larger than twice the displayed size.
    function decodeSize() {
        var dpr = Number(root.Screen.devicePixelRatio)
        if (!isFinite(dpr) || dpr <= 0) dpr = 1
        var shown = Math.max(root.width, root.height, 1) * Math.max(1, dpr) * 2
        return Math.max(16, Math.min(root.canvasSize, Math.ceil(shown)))
    }
}
