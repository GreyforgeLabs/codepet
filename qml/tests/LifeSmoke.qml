// Life UI smoke test (SPEC-v0.2 §17.7, T6): rules card, adoption, retirement, memorial
// paging, leniency, mode switch, growth acknowledgement and calm copy over every fixture.
// A recording fake host answers from qml/standalone/Fixtures.js; nothing reaches Rust.
// Run: QT_QPA_PLATFORM=offscreen python3 qml/tests/run_qml.py qml/tests/LifeSmoke.qml
import QtQuick
import "../shared"
import "../shared/LifeText.js" as LifeText
import "../standalone/Fixtures.js" as Fixtures

// A real (offscreen) Window, so the keyboard focus chain can be checked.
Window {
    id: top
    width: 760; height: 1400
    visible: true
    property int step: 0
    property int failures: 0
    property int checks: 0
    property string firstKey: ""

    QtObject {
        id: host
        property var snapshot: null
        property bool busy: false
        property string lastError: ""
        property string clientTag: "life-smoke"
        property int presentationLocks: 0
        property string fixture: "unarmed"
        // [{method, params, id}] in order.
        property var requests: []
        // method → number of upcoming failures to return for it.
        property var failNext: ({})
        function load(name) {
            host.fixture = name
            host.snapshot = Fixtures.snapshot(name, 1760400000000)
        }
        function methods() { return host.requests.map(function (r) { return r.method }) }
        function count(method) { return host.requests.filter(function (r) { return r.method === method }).length }
        function last(method) {
            for (var i = host.requests.length - 1; i >= 0; i--)
                if (host.requests[i].method === method)
                    return host.requests[i]
            return null
        }
        function request(method, params, callback, requestId) {
            if (host.busy) {
                if (callback) callback({ v: 1, id: null, ok: false, error: { code: "CLIENT_BUSY", message: "busy" } })
                return
            }
            var id = requestId || ("life-smoke-" + (host.requests.length + 1))
            host.requests = host.requests.concat([{ method: method, params: JSON.parse(JSON.stringify(params || {})), id: id }])
            host.busy = true
            Qt.callLater(function () {
                host.busy = false
                var resp
                if ((host.failNext[method] || 0) > 0) {
                    var f = Object.assign({}, host.failNext)
                    f[method] -= 1
                    host.failNext = f
                    resp = { v: 1, id: id, ok: false, error: { code: "TRANSPORT", message: "simulated lost response" } }
                } else if (method === "state.get") {
                    resp = { v: 1, id: id, ok: true, result: Fixtures.snapshot(host.fixture, 1760400000000) }
                    host.snapshot = resp.result
                } else if (method === "lifecycle.rules") {
                    resp = { v: 1, id: id, ok: true, result: Fixtures.rulesResult() }
                } else if (method === "pet.memorial") {
                    resp = { v: 1, id: id, ok: true, result: Fixtures.memorialPage(host.fixture, params) }
                } else {
                    resp = { v: 1, id: id, ok: true, result: { request_id: id, changed: true } }
                }
                if (!resp.ok)
                    host.lastError = resp.error.code + ": " + resp.error.message
                if (callback) callback(resp)
            })
        }
    }

    Panel { id: panel; width: 740; host: host }

    function find(obj, name) {
        if (!obj) return null
        if (obj.objectName === name) return obj
        var kids = obj.children || []
        for (var i = 0; i < kids.length; i++) {
            var r = find(kids[i], name)
            if (r) return r
        }
        if (obj.contentItem && kids.indexOf(obj.contentItem) === -1)
            return find(obj.contentItem, name)
        return null
    }
    // Every need row is an accessible static text named exactly by its plain-text line.
    function checkNeedRows(label) {
        var needs = ["hunger", "thirst", "mental_health", "hygiene"]
        var ok = true, seen = 0
        for (var i = 0; i < needs.length; i++) {
            var row = find(panel, "life.need." + needs[i])
            if (!row) continue
            seen += 1
            var expected = LifeText.needLine(row.modelData) + (row.modelData.lethal ? "" : " (warning only)")
            if (row.Accessible.role !== Accessible.StaticText || row.Accessible.name !== row.text
                    || row.text !== expected || row.text === "" || row.textFormat !== Text.PlainText) {
                console.log("need row " + needs[i] + ": role=" + row.Accessible.role + " name=" + row.Accessible.name)
                ok = false
            }
        }
        check(ok && seen === 4, label + ": every need row is accessible static text named by its line (" + seen + " rows)")
    }
    function w(name) {
        var o = find(panel, name)
        if (!o) console.log("MISSING " + name)
        return o
    }
    function check(cond, label) {
        top.checks += 1
        console.log((cond ? "PASS " : "FAIL ") + label)
        if (!cond) top.failures += 1
    }
    function same(a, b) { return JSON.stringify(a) === JSON.stringify(b) }
    function calm(text, label) {
        var bad = LifeText.bannedIn(String(text))
        top.check(bad === "", "calm copy (" + label + ")" + (bad ? ": found " + bad : ""))
    }

    Timer {
        interval: 60; repeat: true; running: true
        onTriggered: {
            top.step += 1
            var r, rules, adoption, life
            switch (top.step) {
            // ------------------------------------------------ rules card
            case 1:
                host.load("unarmed")
                panel.view = "life"
                break
            case 2:
                rules = top.w("life.rulesCard")
                top.check(rules && rules.visible, "unarmed life shows the rules card")
                top.check(top.w("rulesCard.permadeath").checked && !top.w("rulesCard.gentle").checked, "permadeath is preselected")
                top.check(!top.w("rulesCard.start").enabled, "start disabled before the acknowledgement")
                top.check(host.requests.length === 0, "showing the rules card sends nothing")
                top.check(top.w("life.frozen").text.indexOf("not set up") >= 0, "unarmed frozen reason shown")
                panel.view = "home"
                break
            case 3:
                top.check(top.w("panel.rulesBanner").visible, "home shows the set-up banner for an unarmed life")
                top.w("panel.setUpRules").clicked()
                top.check(panel.view === "life", "banner opens the Life view")
                top.w("rulesCard.ack").checked = true
                top.check(top.w("rulesCard.start").enabled, "start enabled once acknowledged")
                host.failNext = { "pet.rules": 1 }
                top.w("rulesCard.start").clicked()
                break
            case 5:
                r = host.last("pet.rules")
                top.check(r && top.same(r.params, { life_id: Fixtures.LIFE_ID, mode: "permadeath", acknowledge_rules: true }),
                          "pet.rules params: " + (r ? JSON.stringify(r.params) : "none"))
                top.check(r && r.id.indexOf("life-smoke-rules-") === 0, "pet.rules uses the host clientTag in its retry key")
                top.firstKey = r ? r.id : ""
                top.check(top.w("rulesCard.notice").text.indexOf("Nothing changed") === 0, "a failed pet.rules is reported plainly")
                top.check(top.w("rulesCard.ack").checked, "acknowledgement survives a failed request")
                top.w("rulesCard.start").clicked()
                break
            case 7:
                r = host.last("pet.rules")
                top.check(r && r.id === top.firstKey, "retrying the same rules reuses the retry key")
                top.check(host.methods()[host.methods().length - 1] === "state.get", "a confirmed change is followed by state.get")
                top.check(!top.w("rulesCard.ack").checked, "acknowledgement clears after success")
                top.w("rulesCard.gentle").clicked()
                top.w("rulesCard.ack").checked = true
                top.w("rulesCard.start").clicked()
                break
            case 9:
                r = host.last("pet.rules")
                top.check(r && r.params.mode === "gentle" && r.id !== top.firstKey, "a different choice gets a new key")
                // ------------------------------------------------ adoption
                host.requests = []
                host.load("no_life_memorial")
                panel.view = "home"
                panel.refresh()
                break
            case 11:
                adoption = top.w("panel.adoption")
                top.check(adoption && adoption.visible, "no living companion: home shows adoption")
                top.check(host.count("lifecycle.rules") === 1 && host.methods()[1] === "lifecycle.rules",
                          "exact rule numbers are read once, after the explicit refresh: " + host.methods().join(","))
                top.check(adoption.rulesLoaded, "lifecycle.rules result applied")
                top.check(top.w("adoption.memorial").text
                          === "Biscuit has passed away. They reached level 2 as a Rambler and helped with 6 accepted tasks. Your work, tasks and history are unchanged. You can adopt a new companion whenever you're ready.",
                          "latest memorial card uses the death template: " + top.w("adoption.memorial").text)
                top.check(top.w("adoption.permadeath").checked && top.w("adoption.steady").checked, "permadeath and steady preselected")
                top.check(top.w("adoption.name").text === "Ginger", "name defaults to the cat's default name")
                top.check(!top.w("adoption.adopt").enabled, "adopt disabled before the acknowledgement")
                top.check(top.w("adoption.adopt").text === "Adopt Ginger (starts at level 1)", "adopt button names the companion and level 1")
                adoption.chooseSpecies("dog")
                top.check(top.w("adoption.name").text === "Biscuit", "picking a species updates the default name")
                top.w("adoption.ack").checked = true
                top.check(top.w("adoption.adopt").enabled, "adopt enabled with the acknowledgement")
                var order = []
                var cur = top.w("adoption.chooser")
                for (var k = 0; k < 7 && cur; k++) {
                    cur = cur.nextItemInFocusChain(true)
                    order.push(cur ? cur.objectName : "")
                }
                top.check(top.same(order, ["adoption.name", "adoption.permadeath", "adoption.gentle", "adoption.steady",
                                           "adoption.playtest", "adoption.ack", "adoption.adopt"]),
                          "keyboard order: chooser, name, mode, pace, acknowledgement, adopt: " + JSON.stringify(order))
                host.failNext = { "pet.adopt": 1 }
                top.w("adoption.adopt").clicked()
                break
            case 13:
                r = host.last("pet.adopt")
                top.check(r && top.same(r.params, { species: "dog", name: "Biscuit", mode: "permadeath", pace: "steady", acknowledge_rules: true }),
                          "pet.adopt params: " + (r ? JSON.stringify(r.params) : "none"))
                top.firstKey = r ? r.id : ""
                top.w("adoption.adopt").clicked()
                break
            case 15:
                r = host.last("pet.adopt")
                top.check(r && r.id === top.firstKey && host.count("pet.adopt") === 2, "adoption retry reuses one key")
                adoption = top.w("panel.adoption")
                adoption.chooseSpecies("goldfish")
                adoption.pace = "playtest"
                adoption.mode = "gentle"
                top.check(top.w("adoption.name").text === "Bubbles", "the default name follows the species until edited")
                top.w("adoption.name").text = "  "
                top.check(!top.w("adoption.adopt").enabled && top.w("adoption.blockReason").text.indexOf("name") >= 0, "a blank name blocks adoption")
                top.w("adoption.name").text = "x".repeat(24)
                top.check(top.w("adoption.name").text.length === 24, "name field allows 24 characters")
                top.w("adoption.name").text = "Nemo"
                top.w("adoption.ack").checked = true
                top.w("adoption.adopt").clicked()
                break
            case 17:
                r = host.last("pet.adopt")
                top.check(r && top.same(r.params, { species: "goldfish", name: "Nemo", mode: "gentle", pace: "playtest", acknowledge_rules: true })
                          && r.id !== top.firstKey, "edited adoption sends the new params with a new key")
                // A busy task slot or an existing life blocks adoption.
                var withLife = Fixtures.snapshot("warning_thirst", 0)
                host.snapshot = withLife
                top.check(!top.w("panel.adoption").visible, "adoption hidden while a companion lives")
                var slot = Fixtures.snapshot("no_life_memorial", 0)
                slot.active_task = { task_id: "t", state: "review_ready" }
                host.snapshot = slot
                adoption = top.w("panel.adoption")
                top.check(!adoption.canAdopt && adoption.blockReason.indexOf("task") >= 0, "adoption waits for a free task slot")
                // ------------------------------------------------ memorial paging
                host.requests = []
                host.load("no_life_memorial")
                panel.view = "life"
                break
            case 18:
                var mem = top.w("life.memorial")
                top.check(mem && mem.visible && mem.count === 3, "memorial count from the snapshot")
                top.check(host.count("pet.memorial") === 0, "the memorial does not load on its own")
                mem.pageSize = 2
                top.w("memorial.show").clicked()
                break
            case 20:
                r = host.last("pet.memorial")
                top.check(host.count("pet.memorial") === 1 && r && top.same(r.params, { limit: 2 }), "first page only on Show memorial")
                top.check(top.w("life.memorial").entries.length === 2 && top.w("memorial.more").visible, "two entries and a More button")
                top.w("memorial.more").clicked()
                break
            case 22:
                r = host.last("pet.memorial")
                top.check(host.count("pet.memorial") === 2 && r && top.same(r.params, { limit: 2, before_ordinal: 2 }), "More asks for the next page")
                top.check(top.w("life.memorial").entries.length === 3 && !top.w("memorial.more").visible, "all three entries; no further pages")
                top.calm(top.w("life.memorial").entries.map(function (e) { return top.w("life.memorial").entryTitle(e) + " " + LifeText.memorialStory(e) }).join(" "), "memorial")
                // ------------------------------------------------ retirement (two steps, typed name)
                host.requests = []
                host.load("warning_thirst")
                panel.view = "life"
                break
            case 23:
                life = top.w("panel.life")
                top.check(top.w("life.retireOpen").enabled && !top.w("life.retireConfirm").visible, "retire starts closed (step one)")
                top.w("life.retireOpen").clicked()
                top.check(life.retireStep === 1 && top.w("life.retireConfirm").visible, "Retire… opens the typed-name step")
                top.check(!top.w("life.retireConfirm").enabled, "confirm disabled before typing the name")
                top.w("life.retireName").text = "ginger"
                top.check(!top.w("life.retireConfirm").enabled, "a wrong-case name does not confirm")
                top.w("life.retireName").text = "Ginger"
                top.check(top.w("life.retireConfirm").enabled, "the exact name enables confirm")
                top.check(host.count("pet.retire") === 0, "nothing sent before confirm")
                top.w("life.retireConfirm").clicked()
                break
            case 25:
                r = host.last("pet.retire")
                top.check(r && top.same(r.params, { life_id: Fixtures.LIFE_ID, confirm_name: "Ginger" }), "pet.retire params")
                top.check(top.w("panel.life").retireStep === 0, "retire step closes after success")
                host.load("critical_hunger")
                top.check(!top.w("life.retireOpen").enabled && top.w("life.retireBlock").text.indexOf("urgent") >= 0, "no retirement while critical")
                host.load("death_pending_review")
                top.check(!top.w("life.retireOpen").enabled, "no retirement while death is pending")
                host.load("robot_working")
                top.check(!top.w("life.retireOpen").enabled && top.w("life.retireBlock").text.indexOf("task") >= 0, "no retirement with a busy slot")
                // ------------------------------------------------ leniency (only longer or slower)
                host.requests = []
                host.load("warning_thirst")
                break
            case 26:
                var grace = top.w("life.graceBox"), crit = top.w("life.criticalBox"), decay = top.w("life.decayBox")
                var gm = grace.model, cm = crit.model, dm = decay.model
                top.check(gm[0].value === 0 && cm[0].value === 0 && dm[0].value === 0, "each control starts at No change")
                top.check(gm.slice(1).every(function (o) { return o.value * 3600000 > 21600000 && o.value <= 48 }) && gm.length === 43,
                          "grace offers only 7..48 h (longer than 6 h)")
                top.check(cm.slice(1).every(function (o) { return o.value * 3600000 > 7200000 && o.value <= 24 }) && cm.length === 23,
                          "urgent-after offers only 3..24 h (longer than 2 h)")
                top.check(dm.slice(1).every(function (o) { return o.value >= 25 && o.value < 100 }), "decay offers only slower speeds")
                top.check(!top.w("life.leniencyApply").enabled, "apply disabled with no change")
                grace.currentIndex = 1
                decay.currentIndex = 2
                top.check(top.w("life.leniencyApply").enabled, "apply enabled with a lenient choice")
                top.w("life.leniencyApply").clicked()
                break
            case 28:
                r = host.last("pet.configure_life")
                top.check(r && top.same(r.params, { life_id: Fixtures.LIFE_ID, expected_revision: 3, grace_hours: 7, decay_percent: 75 }),
                          "pet.configure_life params: " + (r ? JSON.stringify(r.params) : "none"))
                top.check(top.w("life.graceBox").currentIndex === 0, "choices reset after success")
                // ------------------------------------------------ mode switch
                top.check(top.w("life.modeOpen").enabled, "permadeath → gentle is allowed")
                top.w("life.modeOpen").clicked()
                top.check(!top.w("life.modeConfirm").enabled, "mode switch needs its own acknowledgement")
                top.w("life.modeAck").checked = true
                top.w("life.modeConfirm").clicked()
                break
            case 30:
                r = host.last("pet.rules")
                top.check(r && top.same(r.params, { life_id: Fixtures.LIFE_ID, mode: "gentle", acknowledge_rules: true }), "mode switch sends pet.rules")
                host.load("gentle_fainted")
                top.check(!top.w("life.modeOpen").enabled && top.w("life.modeBlock").text.indexOf("healthy") >= 0,
                          "gentle → permadeath refused unless healthy, with the reason shown")
                // ------------------------------------------------ growth acknowledgement
                host.requests = []
                host.load("evolved_unseen")
                panel.view = "companion"
                break
            case 31:
                top.check(top.w("panel.growth").visible && panel.growthText() === "★ Ginger grew into a Desk Prowler!",
                          "unseen growth shows a static line: " + panel.growthText())
                top.check(host.requests.length === 0, "showing growth sends nothing")
                top.w("panel.growthSeen").clicked()
                break
            case 33:
                r = host.last("pet.events_seen")
                top.check(r && top.same(r.params, { life_id: Fixtures.LIFE_ID, through_seq: 88 }), "acknowledged growth sends pet.events_seen once")
                top.check(host.count("pet.events_seen") === 1, "exactly one events_seen")
                // ------------------------------------------------ every fixture: honest, calm copy
                host.requests = []
                panel.view = "life"
                var names = Fixtures.NAMES
                for (var i = 0; i < names.length; i++) {
                    host.load(names[i])
                    var status = top.w("life.status").text + " " + top.w("panel.adoption").visible
                    top.calm(top.w("life.status").text, names[i] + " status")
                    top.calm(top.w("life.frozen").text, names[i] + " frozen")
                    if (top.w("life.critical").visible)
                        top.calm(top.w("life.critical").title, names[i] + " critical title")
                    var lp = top.w("panel.life")
                    for (var n = 0; n < lp.needs.length; n++) {
                        var line = LifeText.needLine(lp.needs[n])
                        top.calm(line, names[i] + " need " + lp.needs[n].need)
                        if (lp.needs[n].until_critical_ms > 0 && lp.needs[n].state !== "critical")
                            top.check(line.indexOf("active time") >= 0, names[i] + " need durations say active time")
                    }
                }
                top.check(host.requests.length === 0, "rendering every fixture sends nothing")
                host.load("warning_thirst")
                var warnText = top.w("life.status").text
                top.check(/^Ginger is getting thirsty\. Water will help\. About .+ of active time before it becomes serious\.$/.test(warnText),
                          "warning names need, action and active-time duration: " + warnText)
                top.check(top.w("life.need.thirst").text.indexOf("Hydration 28% · low · water") === 0, "thirst need row")
                top.checkNeedRows("warning")
                top.check(!top.w("life.critical").visible, "no critical card while only warning")
                host.load("critical_hunger")
                top.checkNeedRows("critical")
                top.check(top.w("life.critical").visible && top.w("life.careNow").visible && top.w("life.criticalPause").visible,
                          "critical offers care and pause")
                top.check(top.w("life.status").text.indexOf("Ginger urgently needs food. Care now keeps them safe. Without food, Ginger may pass away after about 5 h of active time.") === 0,
                          "critical copy: " + top.w("life.status").text)
                top.w("life.careNow").clicked()
                break
            case 35:
                r = host.last("pet.care")
                top.check(r && top.same(r.params, { action: "food" }), "care now sends the matching care action")
                host.load("gentle_fainted")
                top.check(top.w("life.status").text === "Ginger fainted from thirst. Water will revive them; work waits until then.", "fainted copy")
                top.check(top.w("life.status").text.indexOf("pass away") < 0, "gentle copy never mentions passing")
                host.load("death_pending_review")
                top.check(top.w("life.status").text.indexOf("kept safe") >= 0 && !top.w("life.careNow").visible && !top.w("life.pause").enabled,
                          "death pending keeps the job safe and offers no care or pause")
                host.load("paused")
                top.check(top.w("life.frozen").text === "Paused: time isn't counting (on a break)." && top.w("life.pause").text === "Resume",
                          "paused shows the frozen reason and a Resume toggle")
                top.w("life.pause").clicked()
                break
            case 37:
                r = host.last("pet.pause")
                top.check(r && top.same(r.params, { paused: false }), "resume sends pet.pause {paused: false}")
                host.load("robot_working")
                top.check(top.w("life.frozen").text === "Paused: time isn't counting (task running).", "task frozen reason")
                host.load("no_life_memorial")
                top.check(top.w("panel.adoption").visible && !top.w("life.status").visible, "life view with no companion shows adoption")
                console.log("DONE checks=" + top.checks + " failures=" + top.failures)
                Qt.exit(top.failures === 0 ? 0 : 3)
                break
            }
        }
    }
}
