// PetSprite renderer smoke (T5, SPEC §17.6) against the generated fixture pack.
// Run: QT_QPA_PLATFORM=offscreen python3 qml/tests/run_qml.py qml/tests/PetSpriteSmoke.qml
import QtQuick
import "../shared"
import "../shared/PetCatalog.js" as PetCatalog
import "fixtures/pets/PetRegistry.js" as FixtureRegistry

Item {
    id: top
    width: 400; height: 300
    property int step: 0
    property int failures: 0
    property int checks: 0
    readonly property string base: String(Qt.resolvedUrl("fixtures/pets/"))
    // A host the renderer could misuse; it must never be called.
    QtObject {
        id: host
        property int requests: 0
        function request(method, params, callback, id) { requests += 1 }
    }
    PetSprite { id: sprite; width: 112; height: 112; petId: "cat"; stage: 1 }
    function check(condition, label) {
        checks += 1
        console.log((condition ? "PASS " : "FAIL ") + label)
        if (!condition) failures += 1
    }
    function src() { return String(sprite.currentSource) }
    function ends(suffix) { return src().slice(-suffix.length) === suffix }
    Component.onCompleted: PetCatalog.install(FixtureRegistry.REGISTRY, top.base)
    Timer {
        interval: 50; repeat: true; running: true
        onTriggered: {
            top.step += 1
            switch (top.step) {
            case 1:
                sprite.petId = ""; sprite.petId = "cat" // re-resolve after install()
                sprite.semanticState = "idle"; sprite.frame = 2
                break
            case 2:
                top.check(top.ends("/cat/s1/b01.webp") && sprite.loadStatus === Image.Ready, "cat idle frame 2 loads b01 from the s1 pack: " + top.src())
                top.check(!sprite.showingPlaceholder && !sprite.mirrored, "real frame, not mirrored")
                sprite.semanticState = "dance"
                top.check(sprite.resolved.state === "idle" && top.ends("/cat/s1/b01.webp"), "unknown state renders idle")
                sprite.frame = 0
                sprite.pose = "walk"
                top.check(sprite.semanticState === "walk" && sprite.resolved.state === "locomotion" && top.ends("/cat/s1/b08.webp"),
                          "legacy pose alias walk maps to locomotion")
                sprite.facingLeft = true
                top.check(sprite.mirrored, "floor species mirrors when facing left")
                sprite.stage = 3; sprite.frame = 2
                top.check(top.ends("/cat/s3/b10.webp"), "stage 3 loads only the s3 frame")
                sprite.petId = "goldfish"
                top.check(!sprite.mirrored && top.ends("/goldfish/s3/b10.webp"), "aquatic species never mirrors")
                sprite.facingLeft = false
                sprite.petId = "dragon"
                top.check(sprite.showingPlaceholder && top.ends("fixtures/pets/_placeholder.webp") && sprite.loadStatus === Image.Ready,
                          "unknown species shows the placeholder")
                sprite.petId = "cat"; sprite.stage = 2
                sprite.semanticState = "memorial"
                top.check(top.ends("fixtures/pets/_memorial.webp") && sprite.loadStatus === Image.Ready, "memorial renders _memorial.webp")
                sprite.semanticState = "water"; sprite.frame = 4; sprite.reducedMotion = true
                top.check(top.ends("/cat/s2/c02.webp"), "reduced motion shows the state's reducedFrame")
                sprite.reducedMotion = false
                top.check(top.ends("/cat/s2/c03.webp"), "water frame 4 is c03 (explicit list)")
                sprite.frame = 3
                top.check(top.ends("/cat/s2/c02.webp"), "water frame 3 reuses c02 (shared frame)")
                sprite.frame = 5
                top.check(top.ends("/cat/s2/b04.webp"), "confirmed-result frame is b04")
                // A registry entry whose pack is missing on disk.
                sprite.petId = "ghost"; sprite.semanticState = "idle"; sprite.frame = 0; sprite.facingLeft = true
                break
            case 3:
                top.check(sprite.broken && sprite.showingPlaceholder && top.ends("fixtures/pets/_placeholder.webp"),
                          "missing file falls back to the placeholder: " + top.src())
                top.check(!sprite.mirrored, "the placeholder is never mirrored")
                top.check(host.requests === 0, "asset failure issues no request")
                sprite.petId = "cat"; sprite.facingLeft = false
                top.check(!sprite.broken && top.ends("/cat/s2/b00.webp") && sprite.loadStatus === Image.Ready,
                          "a later valid frame clears the fallback")
                // 200 look switches: only the current pack (and stage) is ever referenced.
                var ids = ["cat", "goldfish", "ghost", "cat", "goldfish"]
                var foreign = 0, held = 0
                for (var i = 0; i < 200; i++) {
                    var id = ids[i % ids.length]
                    sprite.petId = id
                    sprite.stage = 1 + (i % 3)
                    sprite.frame = i
                    var target = String(sprite.targetUrl)
                    if (target.indexOf(top.base + id + "/s" + sprite.stage + "/") !== 0) foreign += 1
                    // The one Image only ever holds the current target (or the placeholder).
                    var shown = String(sprite.currentSource)
                    if (shown !== target && !sprite.showingPlaceholder) held += 1
                }
                top.check(foreign === 0, "every switch targets only the current pack and stage")
                top.check(held === 0 && !sprite.pixmapCached,
                          "no earlier pack stays referenced or cached: only the shown frame is decoded")
                break
            case 5:
                top.check(sprite.petId === "goldfish" && sprite.loadStatus === Image.Ready && !sprite.showingPlaceholder,
                          "after the switch storm the final frame is loaded")
                top.check(host.requests === 0, "the renderer never requested anything")
                PetCatalog.install()
                console.log("PetSprite smoke: " + top.checks + " checks, " + top.failures + " failures")
                Qt.exit(top.failures === 0 ? 0 : 3)
            }
        }
    }
}
