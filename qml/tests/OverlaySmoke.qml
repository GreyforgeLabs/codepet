import QtQuick
import "../shared"
import "../standalone"

Item {
    id: test
    width: 1000; height: 700
    property int failures: 0
    property int checks: 0
    property int phase: 0
    property int requestBaseline: 0
    property int savedFrame: 0
    property int openedPanels: 0
    PreviewHost { id: host }
    DesktopPet { id: scene; width: test.width; height: test.height; host: host; onTaskPanelRequested: test.openedPanels += 1 }
    function check(value, label) {
        checks += 1
        if (!value) failures += 1
        console.log((value ? "PASS " : "FAIL ") + label)
    }
    Timer {
        interval: 250; repeat: true; running: true
        onTriggered: {
            test.phase += 1
            switch (test.phase) {
            case 1:
                test.check(host.seq === 0, "scene startup launches no RPC")
                test.check(scene.petHit.width === 110 && scene.menuHit.width === 0, "pet is small and collapsed menu has no input")
                scene.selected = true
                test.check(scene.menuHit.width === 260, "selecting pet exposes menu input")
                scene.width = 180; scene.height = 160
                test.check(scene.menuHit.x >= 0 && scene.menuHit.y >= 0 && scene.menuHit.width <= 180 && scene.menuHit.height <= 160, "short narrow scene clamps menu")
                scene.petX = 999; scene.clampPosition()
                test.check(scene.petX + scene.petSize <= scene.width, "position clamps to screen after resize")
                scene.width = 1000; scene.height = 700; scene.selected = false
                scene.idleTicks = 0; scene.walking = true; scene.destination = 600; scene.petX = 100
                scene.cosmeticTick()
                test.check(scene.petX === 105, "movement is a bounded local floor step")
                test.check(host.seq === 0, "movement never sends RPC")
                scene.reducedMotion = true
                test.savedFrame = scene.frame
                break
            case 4:
                test.check(scene.frame === test.savedFrame && !scene.walking, "reduced motion freezes scheduler and walking")
                scene.reducedMotion = false
                scene.surfaceActive = false
                test.savedFrame = scene.frame
                scene.selected = true
                scene.requestCare("food")
                test.check(host.seq === 0, "inactive control cannot send care")
                test.check(scene.petHit.width === 0 && scene.menuHit.width === 0 && scene.propsHit.width === 0, "inactive surface exposes no input")
                break
            case 7:
                test.check(scene.frame === test.savedFrame, "inactive surface stops cosmetic scheduler")
                scene.surfaceActive = true; scene.selected = true
                host.fixture = "ready"; host.snapshot = host.buildSnapshot("ready")
                host.busy = true
                test.requestBaseline = host.seq
                scene.requestCare("food")
                test.check(host.seq === test.requestBaseline, "busy host rejects care without overlapping RPC")
                host.busy = false
                scene.taskPanelRequested()
                test.check(test.openedPanels === 1 && host.seq === test.requestBaseline, "opening task panel emits signal without starting work")
                scene.requestCare("food")
                break
            case 9:
                test.check(host.lastError.indexOf("PREVIEW_ONLY") === 0, "care remains an explicit host request and preview refuses mutation")
                test.check(host.snapshot.meters.hunger === 10000, "UI does not mutate authoritative meters")
                host.fixture = "sleeping"; host.snapshot = host.buildSnapshot("sleeping")
                test.requestBaseline = host.seq
                scene.requestCare("food")
                test.check(host.seq === test.requestBaseline, "sleeping snapshot disables explicit food")
                test.check(scene.pose === "sleep" && !scene.motionRunning, "sleep snapshot stops roaming with static sleep pose")
                break
            case 13:
                test.check(host.seq === test.requestBaseline, "sleep display never wakes, refreshes or starts work")
                console.log("Overlay smoke: " + test.checks + " checks, " + test.failures + " failures")
                Qt.exit(test.failures > 0 ? 1 : 0)
            }
        }
    }
}
