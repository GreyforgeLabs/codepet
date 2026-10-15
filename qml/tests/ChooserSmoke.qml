// PetChooser smoke test (SPEC-v0.2 §12, §17.7 T6): grid, keyboard, accessibility, lazy
// thumbnails, pet.select with one retry key per choice, and every disabled state
// (busy, presentation lock, no life, death pending). Pick mode never sends anything.
// Run: QT_QPA_PLATFORM=offscreen python3 qml/tests/run_qml.py qml/tests/ChooserSmoke.qml
import QtQuick
import "../shared"
import "../shared/LifeText.js" as LifeText
import "../standalone/Fixtures.js" as Fixtures

Item {
    id: top
    width: 700; height: 900
    property int step: 0
    property int failures: 0
    property int checks: 0
    property string firstKey: ""
    property int closedCount: 0
    property var pickedIds: []

    QtObject {
        id: host
        property var snapshot: Fixtures.snapshot("warning_thirst", 0)
        property bool busy: false
        property string lastError: ""
        property string clientTag: "chooser-smoke"
        property int presentationLocks: 0
        property var requests: []
        property int failSelect: 0
        function count(method) { return host.requests.filter(function (r) { return r.method === method }).length }
        function last(method) {
            for (var i = host.requests.length - 1; i >= 0; i--)
                if (host.requests[i].method === method) return host.requests[i]
            return null
        }
        function request(method, params, callback, requestId) {
            if (host.busy) {
                if (callback) callback({ v: 1, id: null, ok: false, error: { code: "CLIENT_BUSY", message: "busy" } })
                return
            }
            var id = requestId || ("chooser-smoke-" + (host.requests.length + 1))
            host.requests = host.requests.concat([{ method: method, params: JSON.parse(JSON.stringify(params || {})), id: id }])
            host.busy = true
            Qt.callLater(function () {
                host.busy = false
                var resp
                if (method === "pet.select" && host.failSelect > 0) {
                    host.failSelect -= 1
                    resp = { v: 1, id: id, ok: false, error: { code: "TRANSPORT", message: "simulated lost response" } }
                } else if (method === "state.get") {
                    resp = { v: 1, id: id, ok: true, result: host.snapshot }
                } else {
                    resp = { v: 1, id: id, ok: true, result: { request_id: id, changed: true } }
                }
                if (callback) callback(resp)
            })
        }
    }

    PetChooser {
        id: chooser
        width: 680
        host: host
        mode: "look"
        onClosed: top.closedCount += 1
    }
    PetChooser {
        id: picker
        y: 400
        width: 680
        host: host
        mode: "pick"
        visible: false
        onPicked: id => top.pickedIds = top.pickedIds.concat([id])
    }

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
    function check(cond, label) {
        top.checks += 1
        console.log((cond ? "PASS " : "FAIL ") + label)
        if (!cond) top.failures += 1
    }
    function same(a, b) { return JSON.stringify(a) === JSON.stringify(b) }
    function cells(root) {
        return LifeText.speciesIds().map(function (id) { return top.find(root, "chooser.cell." + id) }).filter(function (c) { return c !== null })
    }

    Timer {
        interval: 60; repeat: true; running: true
        onTriggered: {
            top.step += 1
            var r, use = top.find(chooser, "chooser.use")
            switch (top.step) {
            case 1:
                var all = top.cells(chooser)
                top.check(all.length === 15, "look mode shows all 15 companions (5 × 3)")
                top.check(top.find(chooser, "chooser.grid").columns === 5, "grid has 5 columns")
                top.check(all[0].Accessible.name === "Cat: the curious desk inspector (current look)", "cell accessible name: " + all[0].Accessible.name)
                top.check(all[1].Accessible.name === "Dog: the enthusiastic teammate", "dog cell accessible name")
                top.check(all.every(function (c) { return c.Accessible.role === Accessible.Button }), "every cell is an accessible button")
                top.check(chooser.highlightedId === "cat" && chooser.stage === 1, "starts on the current look and stage")
                top.check(chooser.thumbUrl("cat", 2).toString().endsWith("/assets/pets/cat/thumb-s2.webp"), "thumbnail URL pattern")
                top.check(chooser.thumbUrl("../x", 1) === "" && chooser.thumbUrl("cat", 4) === "", "unsafe ids and stages give no URL")
                top.check(top.find(chooser, "chooser.stagePreview").visible, "look mode shows the three-stage preview row")
                top.check(top.cells(picker).length === 0, "a hidden chooser creates no thumbnails")
                top.check(!use.enabled, "the current look cannot be re-selected")
                top.check(chooser.cosmeticText === "Changing looks is cosmetic: Ginger's needs, rest, level and countdowns stay exactly the same.", "cosmetic copy")
                top.check(host.requests.length === 0, "showing the chooser sends nothing")
                // Keyboard: arrows, Enter, Esc.
                chooser.handleKey(Qt.Key_Left)
                top.check(chooser.highlightedId === "cat", "Left at the first cell stays put")
                chooser.handleKey(Qt.Key_Down)
                top.check(chooser.highlightedId === "iguana", "Down moves one row (5 cells)")
                chooser.handleKey(Qt.Key_Down)
                chooser.handleKey(Qt.Key_Down)
                top.check(chooser.highlightedId === "turtle", "Down at the last row stays put")
                chooser.handleKey(Qt.Key_Up)
                chooser.handleKey(Qt.Key_Up)
                chooser.handleKey(Qt.Key_Right)
                top.check(chooser.highlightedId === "dog", "Up and Right move the highlight")
                top.check(top.find(chooser, "chooser.highlightText").text === "Dog: the enthusiastic teammate", "highlight label")
                chooser.handleKey(Qt.Key_Escape)
                top.check(top.closedCount === 1, "Esc closes")
                top.check(use.enabled, "a different look can be used")
                host.failSelect = 1
                chooser.handleKey(Qt.Key_Return)
                break
            case 3:
                r = host.last("pet.select")
                top.check(r && top.same(r.params, { life_id: Fixtures.LIFE_ID, pet_id: "dog" }), "Enter sends pet.select for the highlighted look")
                top.check(r && r.id.indexOf("chooser-smoke-select-") === 0, "retry key carries the clientTag")
                top.firstKey = r ? r.id : ""
                top.check(top.find(chooser, "chooser.notice").text.indexOf("did not change") >= 0, "failure is shown as text")
                use.clicked()
                break
            case 5:
                r = host.last("pet.select")
                top.check(r && r.id === top.firstKey && host.count("pet.select") === 2, "retrying the same choice reuses its key")
                top.check(host.requests[host.requests.length - 1].method === "state.get", "a confirmed look is followed by state.get")
                top.check(host.requests.every(function (q) { return q.method === "pet.select" || q.method === "state.get" }),
                          "selecting a look sends only pet.select (and the refresh)")
                top.check(top.find(chooser, "chooser.notice").text === "Ginger now looks like a dog.", "success notice")
                chooser.highlight(2)
                use.clicked()
                break
            case 7:
                r = host.last("pet.select")
                top.check(r && r.params.pet_id === "capybara" && r.id !== top.firstKey, "a new choice gets a new key")
                // Disabled states, each with its reason as text.
                var before = host.requests.length
                chooser.highlight(4)
                host.busy = true
                top.check(!chooser.canSelect && !use.enabled && chooser.lockReason.indexOf("Waiting") === 0, "disabled while busy")
                host.busy = false
                host.presentationLocks = 1
                top.check(!chooser.canSelect && top.find(chooser, "chooser.lockReason").text.indexOf("care routine") >= 0, "disabled while a routine runs")
                chooser.useLook()
                top.check(host.requests.length === before, "a locked chooser sends nothing")
                host.presentationLocks = 0
                top.check(chooser.canSelect, "enabled again once unlocked")
                host.snapshot = Fixtures.snapshot("no_life_memorial", 0)
                top.check(!chooser.canSelect && chooser.lockReason.indexOf("Adopt") === 0, "disabled with no living companion")
                host.snapshot = Fixtures.snapshot("death_pending_review", 0)
                top.check(!chooser.canSelect && chooser.lockReason.indexOf("task finishes") >= 0, "disabled while death is pending")
                chooser.useLook()
                top.check(host.requests.length === before, "a pending life sends nothing")
                host.snapshot = Fixtures.snapshot("evolved_unseen", 0)
                top.check(chooser.stage === 2, "look mode follows the current stage")
                top.check(top.cells(chooser)[0].children[0].source.toString().endsWith("/cat/thumb-s2.webp"), "grid shows thumb-s<current stage>")
                // Pick mode: no life needed, emits picked, never requests.
                host.snapshot = Fixtures.snapshot("no_life_memorial", 0)
                picker.visible = true
                break
            case 8:
                var count = host.requests.length
                top.check(top.cells(picker).length === 15, "pick mode builds its grid once visible")
                top.check(top.cells(picker)[0].children[0].source.toString().endsWith("/cat/thumb-s1.webp"), "pick mode uses thumb-s1")
                top.check(!top.find(picker, "chooser.stagePreview").visible, "pick mode has no stage row")
                top.check(picker.lockReason === "", "pick mode works without a living companion")
                picker.handleKey(Qt.Key_Right)
                picker.handleKey(Qt.Key_Return)
                top.check(top.same(top.pickedIds, ["dog", "dog"]), "pick mode emits picked on move and Enter: " + JSON.stringify(top.pickedIds))
                top.check(host.requests.length === count, "pick mode never sends a request")
                picker.visible = false
                break
            case 9:
                top.check(top.cells(picker).length === 0, "hiding the chooser unloads its thumbnails")
                console.log("DONE checks=" + top.checks + " failures=" + top.failures)
                Qt.exit(top.failures === 0 ? 0 : 3)
                break
            }
        }
    }
}
