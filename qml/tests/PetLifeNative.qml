import QtQuick
import QtQuick.Window
import qs.Commons
import "../shared"
import "../../omarchy-plugin"
import "../../omarchy-plugin/petstate.js" as PetState

Window {
    id: win
    width: 1000; height: 720; visible: true; color: "#202733"
    property var registryInput: null
    property string runtimeBaseInput: ""
    property var doc: null
    property bool homeOpen: false
    property bool holdReply: false
    property bool failReply: false
    property var held: null
    property var calls: []
    property var workState: null
    property real clock: 1700007200000
    property bool calm: false
    function care(action) {
        var method = action === "sleep" ? "pet.sleep" : action === "wake" ? "pet.wake" : "pet.care"
        var params = method === "pet.care" ? { action: action } : {}
        if (action === "wake" && win.doc.life.paused) { method = "pet.pause"; params = { paused: false } }
        ctl.request(method, params, function (r) { pet.careAnswered(action, r) })
    }
    function setSnapshot(serialized) { win.doc = JSON.parse(serialized) }
    function imageReady(item) { return item.status === Image.Ready }
    function release() { var job = held; held = null; if (job) respond(job) }
    function respond(job) {
        if (win.failReply) { job.callback({ ok: false, error: { code: "FIXTURE_ERROR" } }); return }
        var next = JSON.parse(JSON.stringify(win.doc)), changed = false
        if (job.method === "pet.care") {
            var meter = { food: "hunger", water: "thirst", play: "mental_health", wash: "hygiene" }[job.params.action]
            changed = next.meters[meter] < 10000; next.meters[meter] = 10000
            next.needs = next.needs.filter(function (n) { return n.action !== job.params.action })
        } else if (job.method === "treats.give") {
            if (next.treats.jar <= 0) { job.callback({ ok: false, error: { code: "NO_TREATS" } }); return }
            next.treats.jar -= 1; next.life.xp += 5; changed = true
            if (next.life.xp >= 80 && next.life.stage === 1) {
                next.life.stage = 2; next.life.level = 2; next.life.next_stage_xp = 400
                next.unseen_events = [{ seq: 2, kind: "evolved" }]
            }
        } else if (job.method === "pet.sleep" || job.method === "pet.wake") {
            next.care.sleeping = job.method === "pet.sleep"; changed = true
        } else if (job.method === "pet.pause") {
            next.life.paused = job.params.paused; changed = true
        }
        win.doc = next
        job.callback({ ok: true, result: job.method === "treats.give" ? next : { changed: changed } })
    }
    function reset() {
        win.doc = { schema: "codepet.bar.v2", state_version: 1, enabled: true,
            life: { life_id: "life-native-fixture", name: "Pip", appearance: "penguin", stage: 1,
                level: 1, xp: 75, next_stage_xp: 80, armed: true, adopted_at_ms: 1700000000000,
                mode: "gentle", pace: "steady", care_tempo: "steady", revision: 1 },
            care: { sleeping: false }, needs: [],
            meters: { hunger: 10000, thirst: 10000, mental_health: 10000, hygiene: 10000 },
            treats: { jar: 1, full: false }, agent: { apps: [], finished_seq: 0 }, unseen_events: [] }
        win.calls = []; win.workState = { readiness: { ready: true, energy: 100 }, task: null }; win.homeOpen = false
        pet.clearTransients(); pet.heartMsLeft = 0; pet.wheelOpen = false; pet.cardOpen = false; pet.careBusy = false; pet.careMessage = ""
        pet.petX = 200; pet.petY = 570; pet.pinned = true
    }
    QtObject {
        id: ctl
        readonly property var barState: win.doc
        readonly property var life: win.doc ? win.doc.life : null
        readonly property var face: PetState.currentPriority(win.doc, win.clock, {})
        readonly property var registry: win.registryInput
        readonly property var workState: win.workState
        readonly property string runtimeBase: win.runtimeBaseInput
        readonly property bool popupVisible: win.homeOpen
        readonly property real now: win.clock
        readonly property var cfg: ({ calm: win.calm })
        readonly property bool testState: true
        readonly property string stateHome: "synthetic fixture"
        property var snapshot: null
        function request(method, params, callback) {
            win.calls = win.calls.concat([{ method: method, params: params }])
            var job = { method: method, params: params, callback: callback }
            if (win.holdReply) win.held = job
            else Qt.callLater(function () { win.respond(job) })
            return true
        }
        function openWork(kind) {}
        function closePopup() { win.homeOpen = false }
    }
    QtObject {
        id: workCtl
        readonly property var state: win.workState
        readonly property var pet: win.doc ? win.doc.life : null
        readonly property bool busy: false
        readonly property string error: ""
        readonly property string selectingClient: ""
    }
    DesktopPet {
        id: pet
        objectName: "native.pet"
        anchors.fill: parent
        barState: win.doc
        workState: win.workState
        workController: workCtl
        fixedNowMs: win.clock
        reducedMotion: win.calm
        onCareRequested: action => win.care(action)
        onHomeRequested: win.homeOpen = true
    }
    Rectangle {
        visible: win.homeOpen
        x: win.width - width - 20; y: 20
        width: popup.width + 24; height: popup.height + 24
        radius: 16; color: "#202733"; border.color: "#586679"
        PopupApp { id: popup; objectName: "native.popup"; x: 12; y: 12; ctl: ctl; width: implicitWidth }
    }
    Component.onCompleted: { Style.fontBaseSize = 16; win.reset() }
}
