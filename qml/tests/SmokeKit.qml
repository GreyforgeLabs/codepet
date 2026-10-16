import QtQuick
import qs.Commons
import "../../omarchy-plugin/CodePetBar.js" as Model

// Shared checks for the bar plugin's popup smokes (SPEC-v0.3 §1.2, §9.3): fixtures, the
// item-tree walks behind K1/K2/K8 (kid copy), K3 (sizes), K6 (nothing disabled), K9 (timers)
// and K13 (no scrolling), and PASS/FAIL bookkeeping. Sizes are compared in px at base-size
// 16 (SPEC: "all pixel sizes are at the owner's font base-size 16").
Item {
    id: kit
    property int failures: 0
    property int checks: 0
    readonly property string repo: String(Qt.resolvedUrl("../../")).replace(/\/$/, "")
    readonly property string contracts: repo + "/docs/v0.3/contracts/"
    // Set autoLoad false where the repository is not next to this file (the Quickshell
    // harness of test_bar_plugin.py stages a copy).
    property bool autoLoad: true
    property var registry: kit.autoLoad ? kit.json(repo + "/qml/assets/pets/registry.json") : null
    // Every FAIL line, for harnesses that read results over IPC instead of the log.
    property var failed: []
    readonly property var styleFont: Style.font
    // Exemptions: adult pages (K1/K2) and the controls they hold (K3 full width).
    readonly property var adultNames: ["cp.helpFlick", "cp.codexPicture"]

    function json(url) {
        var x = new XMLHttpRequest()
        x.open("GET", url, false)
        x.send()
        return JSON.parse(x.responseText)
    }
    function fixture(name) { return Model.normalize(kit.json(kit.contracts + "fixtures/bar-v2/" + name + ".json")) }
    function status(name) { return kit.json(kit.contracts + "fixtures/clients-status/" + name + ".json") }

    function check(condition, label) {
        kit.checks += 1
        if (!condition) {
            kit.failures += 1
            kit.failed = kit.failed.concat([label])
        }
        console.log((condition ? "PASS " : "FAIL ") + label)
        return condition
    }
    function finish() {
        console.log("DONE checks=" + kit.checks + " failures=" + kit.failures)
        Qt.exit(kit.failures === 0 ? 0 : 3)
    }

    // ---------------------------------------------------------------- tree walks

    function kids(o) {
        var out = []
        var lists = []
        try { if (o.children) lists.push(o.children) } catch (e) {}
        try { if (o.contentItem && o.contentItem !== o) lists.push([o.contentItem]) } catch (e) {}
        for (var l = 0; l < lists.length; l++)
            for (var i = 0; i < lists[l].length; i++) if (out.indexOf(lists[l][i]) < 0) out.push(lists[l][i])
        return out
    }
    function walk(o, fn, seen, adult) {
        seen = seen || []
        if (!o || seen.indexOf(o) >= 0) return
        seen.push(o)
        var isAdult = adult === true || kit.adultNames.indexOf(String(o.objectName)) >= 0
        if (fn(o, isAdult) === false) return
        var ks = kit.kids(o)
        for (var i = 0; i < ks.length; i++) kit.walk(ks[i], fn, seen, isAdult)
    }
    function find(root, name) {
        var hit = null
        kit.walk(root, function (o) { if (!hit && String(o.objectName) === name) hit = o; return hit === null })
        return hit
    }
    function isText(o) { return o && o.text !== undefined && o.lineCount !== undefined && o.elide !== undefined }
    // Shown on screen: visible, opaque enough and inside its clipping ancestors' bounds.
    function shown(o, root) {
        if (!o || !o.visible || o.opacity === 0 || o.width <= 0 || o.height <= 0) return false
        var p = o.parent
        while (p && p !== root) {
            if (p.clip) {
                var r = o.mapToItem(p, 0, 0)
                if (r.x + o.width <= 0 || r.y + o.height <= 0 || r.x >= p.width || r.y >= p.height) return false
            }
            p = p.parent
        }
        return true
    }

    // Visible texts as {text, adult, item}.
    function texts(root) {
        var out = []
        kit.walk(root, function (o, adult) {
            if (!o.visible) return false
            if (kit.isText(o) && String(o.text) !== "" && kit.shown(o, root)) out.push({ text: String(o.text), adult: adult, item: o })
            return true
        })
        return out
    }
    function controls(root) {
        var out = []
        kit.walk(root, function (o, adult) {
            if (!o.visible) return false
            if (o.cpControl === true && kit.shown(o, root)) out.push({ item: o, adult: adult })
            return true
        })
        return out
    }
    // Every running Timer under root (Timers live in `data`/`resources`, not `children`).
    function runningTimers(root) {
        var out = []
        var seen = []
        function visit(o, depth) {
            if (!o || depth > 40 || seen.indexOf(o) >= 0) return
            seen.push(o)
            if (o.interval !== undefined && o.repeat !== undefined && o.running === true && o.triggeredOnStart !== undefined)
                out.push(String(o.objectName || "timer"))
            var lists = []
            try { if (o.data) lists.push(o.data) } catch (e) {}
            try { if (o.children) lists.push(o.children) } catch (e) {}
            try { if (o.resources) lists.push(o.resources) } catch (e) {}
            try { if (o.contentItem) lists.push([o.contentItem]) } catch (e) {}
            try { if (o.item) lists.push([o.item]) } catch (e) {}
            for (var l = 0; l < lists.length; l++) for (var i = 0; i < lists[l].length; i++) visit(lists[l][i], depth + 1)
        }
        visit(root, 0)
        return out
    }

    // ---------------------------------------------------------------- K checks

    function px16(v) { return v / Math.max(0.01, Style.effectiveSpacingScale * 0.75) }
    function near(a, b) { return a + 0.75 >= b }

    // K1, K2, K8 over every visible kid-facing text (adult pages exempt).
    function kidCopy(root, label) {
        var bad = []
        var all = kit.texts(root)
        for (var i = 0; i < all.length; i++) {
            if (all[i].adult) continue
            var p = Model.kidTextProblems(all[i].text)
            if (p.length) bad.push(all[i].text + " (" + p.join(", ") + ")")
        }
        return kit.check(bad.length === 0, label + ": kid copy K1/K2/K8" + (bad.length ? " " + JSON.stringify(bad) : ""))
    }

    // K3 sizes and K6 (no disabled control) for every visible control.
    function sizes(root, label) {
        var bad = []
        var list = kit.controls(root)
        for (var i = 0; i < list.length; i++) {
            var o = list[i].item
            var w = kit.px16(o.width), h = kit.px16(o.height)
            var kind = String(o.cpKind || "control")
            var name = String(o.objectName || kind)
            if (!kit.near(w, 48) || !kit.near(h, 48)) bad.push(name + " " + Math.round(w) + "x" + Math.round(h) + " < 48 (px " + o.width + "x" + o.height + " scale " + Style.effectiveSpacingScale + ")")
            if (kind === "primary" && !kit.near(h, 64)) bad.push(name + " primary height " + Math.round(h))
            if (kind === "primary" && !list[i].adult && !kit.near(w, 448)) bad.push(name + " primary width " + Math.round(w))
            if (kind === "secondary" && !kit.near(h, 48)) bad.push(name + " secondary height " + Math.round(h))
            if (kind === "tile" && (!kit.near(h, 104) || !kit.near(w, 104))) bad.push(name + " tile " + Math.round(w) + "x" + Math.round(h))
            if (kind === "round" && !kit.near(Math.min(w, h), 72)) bad.push(name + " round " + Math.round(w) + "x" + Math.round(h))
            if (kind === "treats" && !kit.near(h, 72)) bad.push(name + " treats " + Math.round(h))
            if (kind === "switch" && (!kit.near(w, 96) || !kit.near(h, 48))) bad.push(name + " switch " + Math.round(w) + "x" + Math.round(h))
            if (o.enabled === false) bad.push(name + " is disabled (K6)")
            if (["primary", "secondary", "tile", "round", "treats", "switch"].indexOf(kind) >= 0) {
                kit.walk(o, function (c) {
                    if (c !== o && kit.isText(c) && c.visible && /[A-Za-z]/.test(String(c.text)) && c.font.pixelSize < kit.styleFont.title)
                        bad.push(name + " label font " + c.font.pixelSize + " < title " + kit.styleFont.title)
                    return true
                })
            }
        }
        return kit.check(bad.length === 0, label + ": K3/K6 sizes" + (bad.length ? " " + JSON.stringify(bad) : ""))
    }

    // K13: the content fits the card cap (min(640 px at base 16, 90% of a 1080 output)) and
    // every primary button lies inside it.
    function fits(app, label) {
        var pad = Style.space(12)
        var cap = Math.min(Style.space(480), Math.floor(1080 * 0.9))
        var total = app.implicitHeight + 2 * pad
        var bad = []
        if (total > cap) bad.push("height " + Math.round(kit.px16(total)) + " > " + Math.round(kit.px16(cap)))
        var list = kit.controls(app)
        for (var i = 0; i < list.length; i++) {
            var o = list[i].item
            if (o.cpKind !== "primary" || list[i].adult) continue
            var p = o.mapToItem(app, 0, 0)
            if (p.y < 0 || p.y + o.height + 2 * pad > cap + 0.5 || p.x < -0.5 || p.x + o.width > app.width + 0.5)
                bad.push(String(o.objectName) + " outside the card")
        }
        return kit.check(bad.length === 0, label + ": K13 fits (" + Math.round(kit.px16(total)) + " px16)" + (bad.length ? " " + JSON.stringify(bad) : ""))
    }

    // The Home status line keeps to one line: at kit.styleFont.heading, or (a long app pair in a
    // monospace theme font, notes.md §8) stepped down to no less than kit.styleFont.body.
    TextMetrics { id: metrics }
    property var shrunk: []
    function oneLine(app, label) {
        var line = kit.find(app, "cp.statusLine")
        if (!line || !line.visible) return kit.check(true, label + ": no status line")
        metrics.font.family = line.font.family
        metrics.font.bold = true
        metrics.font.pixelSize = kit.styleFont.heading
        metrics.text = line.text
        var atHeading = metrics.advanceWidth <= line.width
        return kit.check(line.lineCount <= 1 && atHeading && !line.truncated,
                         label + ": status line at heading size (" + line.text + ")")
    }

    // No visible kid-facing text is cut off: an elided or line-capped Text reports
    // `truncated` when what it renders is shorter than its text (adult pages exempt).
    function notCut(root, label) {
        var bad = []
        var all = kit.texts(root)
        for (var i = 0; i < all.length; i++)
            if (!all[i].adult && all[i].item.truncated === true) bad.push(all[i].text)
        return kit.check(bad.length === 0, label + ": no kid text cut off" + (bad.length ? " " + JSON.stringify(bad) : ""))
    }

    // ---------------------------------------------------------------- sequencing

    // Runs steps one after another; a step may return the ms to wait before the next one
    // (default 80, enough for the window to lay items out). Then finish().
    property var steps: []
    property int stepIndex: 0
    function run(list) {
        kit.steps = list
        kit.stepIndex = 0
        sequencer.interval = 80
        sequencer.start()
    }
    Timer {
        id: sequencer
        repeat: false
        onTriggered: {
            if (kit.stepIndex >= kit.steps.length) { kit.finish(); return }
            var step = kit.steps[kit.stepIndex]
            kit.stepIndex += 1
            var wait = 80
            try {
                var w = step()
                if (typeof w === "number") wait = w
            } catch (e) {
                kit.check(false, "step " + kit.stepIndex + " threw: " + e + "\n" + e.stack)
            }
            sequencer.interval = Math.max(1, wait)
            sequencer.start()
        }
    }
    // Two steps per (base size x item): setup(size, item) after the base size is set, then,
    // once the window has laid it out, verify(size, item).
    function each(sizes, items, setup, verify) {
        var out = []
        for (var s = 0; s < sizes.length; s++)
            for (var i = 0; i < items.length; i++)
                (function (size, item) {
                    out.push(function () { Style.fontBaseSize = size; return setup(size, item) })
                    out.push(function () { return verify(size, item) })
                })(sizes[s], items[i])
        return out
    }

    function all(app, label) {
        kit.kidCopy(app, label)
        kit.notCut(app, label)
        kit.sizes(app, label)
        kit.fits(app, label)
    }
}
