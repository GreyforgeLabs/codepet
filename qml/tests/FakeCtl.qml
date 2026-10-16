import QtQuick
import "../../omarchy-plugin/CodePetBar.js" as Model
import "../../omarchy-plugin/DesktopPetFlow.js" as DesktopFlow
import "../../omarchy-plugin/petstate.js" as PetState

// Test-only controller for the bar plugin's PopupApp (the role BarWidget plays in the
// shell): it serves a bar-v2 fixture, records requests and answers them from `responses`
// on the next event-loop turn (or holds them while `hold` is set). Like BarWidget.request it
// applies the widget's own follow-ups (DesktopPetFlow.responseEffects) before the callback.
QtObject {
    id: ctl
    property var barState: null
    property var snapshot: null
    property var registry: null
    property var cfg: ({ label: "name", presence: true, overlayOnLoad: false, calm: false })
    property string runtimeBase: ""
    property bool popupVisible: true
    property bool testState: false
    property string stateHome: ""
    property real now: 1792123900000
    property var seen: ({ finished_seq: 0, hello_seq: 0, test_at_ms: 0 })
    readonly property var face: barState ? PetState.sharedPriority(barState, now, seen) : null

    property var calls: []
    property var settings: []
    property var launches: []
    property var pages: []
    property int closes: 0
    // method -> result object, or function(params) returning {ok, result|error}.
    property var responses: ({})
    property bool hold: false
    property var held: []

    function methods() { return ctl.calls.map(function (c) { return c.method }) }
    function count(method) { return ctl.calls.filter(function (c) { return c.method === method }).length }
    function last(method) {
        for (var i = ctl.calls.length - 1; i >= 0; i--) if (ctl.calls[i].method === method) return ctl.calls[i]
        return null
    }
    function answer(method, params) {
        var r = ctl.responses[method]
        if (typeof r === "function") return r(params)
        if (r && typeof r === "object" && (r.ok === true || r.ok === false)) return r
        if (r === undefined && method === "clients.status") return { ok: false, error: { code: "NOT_IMPLEMENTED" } }
        if (r === undefined && method === "pet.memorial") return { ok: true, result: { entries: [], next_before_ordinal: null } }
        if (r === undefined && method === "pet.memories") return { ok: true, result: {
            entries: [{ seq: 1, kind: "born", at_ms: ctl.now, name: "Ginger", appearance: "cat", stage: 1, past: false }], next_before_seq: null } }
        return { ok: true, result: r !== undefined ? r : { changed: true } }
    }
    function request(method, params, callback) {
        if (!Model.allowedMethod(method)) {
            if (callback) callback({ ok: false, error: { code: "CLIENT_REFUSED" } })
            return false
        }
        ctl.calls = ctl.calls.concat([{ method: method, params: params }])
        var resp = ctl.answer(method, params)
        var job = { method: method, params: params, resp: resp, callback: callback }
        if (ctl.hold) { ctl.held = ctl.held.concat([job]); return true }
        Qt.callLater(function () { ctl.deliver(job) })
        return true
    }
    function deliver(job) {
        var fx = DesktopFlow.responseEffects(job.method, job.params, job.resp, ctl.cfg)
        for (var i = 0; i < fx.length; i++) {
            if (fx[i].setting !== undefined) ctl.setSetting(fx[i].setting, fx[i].value)
            if (fx[i].launch !== undefined) ctl.launch(fx[i].launch)
        }
        if (job.callback) job.callback(job.resp)
    }
    function release() {
        var h = ctl.held
        ctl.held = []
        for (var i = 0; i < h.length; i++) ctl.deliver(h[i])
    }
    function setSetting(key, value) {
        ctl.settings = ctl.settings.concat([{ key: key, value: value }])
        var next = {}
        for (var k in ctl.cfg) next[k] = ctl.cfg[k]
        next[key] = value
        ctl.cfg = next
        return true
    }
    function launch(verb) { ctl.launches = ctl.launches.concat([verb]); return true }
    function openPage(page) { ctl.pages = ctl.pages.concat([page]) }
    function closePopup() { ctl.closes += 1 }
}
