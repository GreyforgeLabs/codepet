import QtQuick
import QtQuick.Window
import "../shared"

Window {
    id: win
    width: 640; height: 480; visible: true
    SmokeKit { id: kit }
    property var careRequests: []
    DesktopPet {
        id: pet; anchors.fill: parent; fixedNowMs: 1700000000000
        onCareRequested: action => win.careRequests = win.careRequests.concat([action])
    }
    Component.onCompleted: kit.run([
        function () { pet.barState = kit.fixture("idle-happy"); pet.pinned = true },
        function () {
            kit.check(typeof pet.toggleWheel === "function", "desktop exposes care wheel")
            if (typeof pet.toggleWheel !== "function") return
            pet.toggleWheel()
            kit.check(pet.wheelOpen && pet.pinned, "opening wheel does not silently unpin")
            var wheel = kit.find(pet, "desktopPet.wheel")
            kit.check(wheel && wheel.visible, "wheel is visible")
            var actions = ["pet", "food", "water", "play", "wash", "sleep", "pin", "close"]
            var boxes = []
            for (var action of actions) {
                var control = kit.find(pet, "desktopPet.wheel." + action)
                kit.check(!!control, action + " has a visible wheel control")
                if (!control) continue
                if (action === "wash") kit.check(control.modelData.label === "Bath" && control.modelData.icon === "care-floor/wash", "Bath uses the existing wash icon and a clear label")
                for (var other of boxes)
                    kit.check(control.x >= other.x + other.width || control.x + control.width <= other.x
                        || control.y >= other.y + other.height || control.y + control.height <= other.y, action + " target does not overlap " + other.objectName)
                boxes.push(control)
            }
            for (var pos of [[0,0],[528,0],[0,368],[528,368]]) {
                pet.petX = pos[0]; pet.petY = pos[1]
                var hit = pet.wheelHit
                kit.check(hit.x >= 0 && hit.y >= 0 && hit.x + hit.width <= pet.width && hit.y + hit.height <= pet.height, "wheel stays inside each screen edge")
            }
            pet.toggleWheel()
            kit.check(!pet.wheelOpen && pet.wheelHit.width === 0, "second right-click closes exact input region")
            var x = pet.petX, y = pet.petY
            pet.idleTicks = 0; pet.resting = true; pet.cosmeticTick()
            kit.check(pet.expressionCount > 0 && pet.petX === x && pet.petY === y, "pinned healthy idle has an expressive beat without roaming")
            pet.clearTransients()
            pet.careAnswered("play", { ok: true, result: { changed: false } })
            kit.check(pet.transientState === "play" && pet.careMessage === "Already feeling good", "confirmed full Play animates without claiming a refill")
            pet.clearTransients(); pet.reducedMotion = true
            var count = pet.expressionCount; pet.idleTicks = 0; pet.resting = true; pet.cosmeticTick()
            kit.check(pet.expressionCount === count, "Calm suppresses idle expression motion")
        },
        function () {
            for (var action of ["pet", "pin", "food", "water", "play", "wash", "sleep"]) {
                pet.careBusy = false; pet.clearTransients(); pet.wheelOpen = true
                var before = win.careRequests.length, pinned = pet.pinned
                pet.chooseCare(action)
                kit.check(!pet.wheelOpen && pet.wheelHit.width === 0, action + " immediately dismisses the wheel and its input region")
                kit.check(win.careRequests.length === before + (["pet", "pin"].indexOf(action) >= 0 ? 0 : 1), action + " sends exactly the expected request count")
                if (action === "pin") kit.check(pet.pinned !== pinned, "closing wheel retains the explicit pin change")
                if (["pet", "pin"].indexOf(action) < 0) {
                    kit.check(pet.careBusy && pet.transientState === "", action + " stays pending without optimistic animation")
                    pet.careAnswered(action, { ok: true, result: { changed: true } })
                    kit.check(!pet.wheelOpen, action + " confirmation does not reopen the wheel")
                }
            }
            pet.clearTransients(); pet.chooseCare("food")
            var count = win.careRequests.length
            pet.wheelOpen = true; pet.chooseCare("water")
            kit.check(!pet.wheelOpen && pet.careBusy && win.careRequests.length === count, "busy care selection closes without a duplicate request")
            pet.careAnswered("food", { ok: false, error: { code: "FIXTURE_ERROR" } })
            kit.check(!pet.wheelOpen && pet.transientState === "" && !pet.careBusy, "failed response stays closed without celebrating")
            var feedback = kit.find(pet, "desktopPet.careFeedback")
            kit.check(feedback && feedback.visible, "closed wheel keeps failed care feedback visible beside the pet")
            kit.check(kit.find(pet, "desktopPet.careFeedbackText").text === "Try again from Care", "feedback explains the failed action")
            pet.careAnswered("sleep", { ok: false, error: { message: "Stop the current job before taking a nap." } })
            kit.check(pet.careMessage === "Stop the current job before taking a nap.", "care failure gives the actionable backend reason")
            var pinned = pet.pinned
            pet.barState = kit.fixture("off")
            pet.chooseCare("pin"); pet.chooseCare("food")
            kit.check(pet.pinned === pinned && win.careRequests.length === count, "inactive actions still do nothing")
        }
    ])
}
