pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "LifeText.js" as LifeText

// Life panel (SPEC-v0.2 §2, §6, §12): status, per-need rows with active-time estimates, the
// critical card, why the clock is paused, Pause/Travel, the mode switch, leniency-only
// settings, two-step retirement by typed name, and the paged memorial. Every value comes
// from the Rust snapshot; every change is one explicit request followed by a state.get.
// Copy is calm and plain text (LifeText.js); durations always say "active time".
ColumnLayout {
    id: root
    required property var host
    property var registry: null
    property string notice: ""
    // Two-step retirement: 0 = closed, 1 = typed-name confirmation open.
    property int retireStep: 0
    property bool modeConfirmOpen: false
    property var retryKeys: ({})
    property int seqNo: 0

    readonly property var snap: host && host.snapshot ? host.snapshot : null
    readonly property var life: snap && snap.life ? snap.life : null
    readonly property var lifecycle: snap && snap.lifecycle ? snap.lifecycle : null
    readonly property var critical: lifecycle && lifecycle.critical ? lifecycle.critical : null
    readonly property var needs: lifecycle && Array.isArray(lifecycle.needs) ? lifecycle.needs : []
    readonly property var rules: LifeText.rulesOf(snap, life ? life.pace : "steady")
    readonly property bool hostBusy: !!(host && host.busy)
    readonly property string name: LifeText.cleanName(life ? life.name : "")
    readonly property bool pending: !!(life && life.death_pending)
    readonly property bool paused: !!(life && life.paused) || !!(snap && snap.decay && snap.decay.paused)
    readonly property var slotStates: ["preparing", "running", "waiting_for_user", "verifying", "review_ready", "cancelling", "recovery_required"]
    readonly property bool slotBusy: !!(snap && snap.active_task && slotStates.indexOf(String(snap.active_task.state)) >= 0)
    readonly property string frozenReason: lifecycle && typeof lifecycle.frozen_reason === "string" ? lifecycle.frozen_reason : ""
    readonly property string targetMode: life && life.mode === "gentle" ? "permadeath" : "gentle"
    readonly property string modeBlock: !life ? "No companion yet."
        : pending ? "The mode stays as it is while the current task finishes."
        : !life.armed ? "Set up life rules first."
        : targetMode === "permadeath" && life.status !== "healthy"
            ? "Switching to permadeath needs " + name + " to be healthy first. Care for every need, then try again."
        : ""
    readonly property string retireBlock: !life ? "No companion yet."
        : pending ? "Retirement isn't available while the current task finishes."
        : life.status === "critical" ? "Care for " + name + " first; retirement isn't available while a need is urgent."
        : slotBusy ? "Finish, review or discard the current task first."
        : ""
    readonly property bool canPause: life !== null && !pending && !hostBusy
    readonly property bool canConfigure: life !== null && life.armed && !pending && !hostBusy

    spacing: 10

    function keyFor(kind, params) {
        var signature = kind + JSON.stringify(params)
        var entry = root.retryKeys[kind]
        if (!entry || entry.signature !== signature) {
            root.seqNo += 1
            entry = { "key": String(root.host.clientTag || "ui") + "-" + kind + "-" + Date.now().toString(36) + "-" + root.seqNo,
                      "signature": signature }
            var next = Object.assign({}, root.retryKeys)
            next[kind] = entry
            root.retryKeys = next
        }
        return entry.key
    }
    function clearKey(kind) {
        var next = Object.assign({}, root.retryKeys)
        delete next[kind]
        root.retryKeys = next
    }
    // One explicit mutation, then one refresh. `kind` scopes the retry key.
    function send(kind, method, params, okText, done) {
        if (root.hostBusy)
            return
        var key = root.keyFor(kind, params)
        root.notice = ""
        root.host.request(method, params, function (resp) {
            if (resp && resp.ok) {
                root.clearKey(kind)
                root.notice = okText
            } else {
                var e = resp && resp.error ? resp.error : {}
                root.notice = "Nothing changed: " + String(e.message || e.code || "unknown error")
            }
            if (done)
                done(resp)
            if (!root.host.busy)
                root.host.request("state.get", {}, function () {})
        }, key)
    }
    function careNow() {
        if (!root.critical || root.pending)
            return
        var action = root.critical.action || LifeText.NEED_ACTION[root.critical.need]
        root.send("care", "pet.care", { "action": action }, LifeText.ACTION_HELP[action] + " given. Thank you for looking after " + root.name + ".")
    }
    function togglePause() {
        if (!root.canPause)
            return
        var next = !root.paused
        root.send("pause", "pet.pause", { "paused": next },
                  next ? "Pause/Travel is on. Time isn't counting for " + root.name + "."
                       : "Welcome back. Active time counts again; nothing was charged while paused.")
    }
    function switchMode() {
        if (root.modeBlock !== "" || root.hostBusy || !modeAck.checked)
            return
        var target = root.targetMode
        root.send("mode", "pet.rules", { "life_id": root.life.life_id, "mode": target, "acknowledge_rules": true },
                  root.name + " now lives in " + LifeText.modeLabel(target).toLowerCase() + " mode.", function (resp) {
                      if (resp && resp.ok) {
                          modeAck.checked = false
                          root.modeConfirmOpen = false
                      }
                  })
    }
    // Leniency-only settings: only longer windows or slower decay are ever offered.
    function leniencyParams() {
        var p = { "life_id": root.life ? root.life.life_id : "", "expected_revision": root.life ? root.life.revision : 0 }
        var any = false
        if (graceBox.currentIndex > 0) { p.grace_hours = graceBox.model[graceBox.currentIndex].value; any = true }
        if (criticalBox.currentIndex > 0) { p.critical_after_hours = criticalBox.model[criticalBox.currentIndex].value; any = true }
        if (decayBox.currentIndex > 0) { p.decay_percent = decayBox.model[decayBox.currentIndex].value; any = true }
        return any ? p : null
    }
    function applyLeniency() {
        var p = root.leniencyParams()
        if (!p || !root.canConfigure)
            return
        root.send("configure", "pet.configure_life", p, "Settings are more lenient now. Any running countdown can only move later.",
                  function (resp) {
                      if (resp && resp.ok) {
                          graceBox.currentIndex = 0
                          criticalBox.currentIndex = 0
                          decayBox.currentIndex = 0
                      }
                  })
    }
    function choiceModel(values, label) {
        var out = [{ "text": "No change", "value": 0 }]
        for (var i = 0; i < values.length; i++)
            out.push({ "text": label(values[i]), "value": values[i] })
        return out
    }
    function openRetire() {
        if (root.retireBlock !== "")
            return
        retireName.text = ""
        root.retireStep = 1
    }
    function cancelRetire() {
        retireName.text = ""
        root.retireStep = 0
    }
    readonly property bool retireNameMatches: life !== null && LifeText.normalizeName(retireName.text) === String(life.name)
    function confirmRetire() {
        if (root.retireStep !== 1 || root.retireBlock !== "" || !root.retireNameMatches || root.hostBusy)
            return
        var retiring = root.name
        root.send("retire", "pet.retire", { "life_id": root.life.life_id, "confirm_name": LifeText.normalizeName(retireName.text) },
                  retiring + " has retired. Their story is in the memorial; your work and history are unchanged.",
                  function (resp) {
                      if (resp && resp.ok)
                          root.cancelRetire()
                  })
    }

    // ---------------------------------------------------------------- status
    Label {
        objectName: "life.headline"
        Layout.fillWidth: true
        visible: root.life !== null
        text: root.life ? LifeText.lifeHeadline(root.life, root.registry) : ""
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        font.bold: true
        font.pixelSize: 18
    }
    Label {
        objectName: "life.badges"
        Layout.fillWidth: true
        visible: root.life !== null
        text: root.life ? [LifeText.modeLabel(root.life.mode), root.life.pace === "playtest" ? "Playtest" : "Steady pace",
                           root.life.armed ? "Rules set" : "Rules not set up", root.paused ? "Pause/Travel on" : ""]
                          .filter(function (s) { return s !== "" }).join(" · ") : ""
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
    }
    Label {
        objectName: "life.status"
        Layout.fillWidth: true
        visible: root.life !== null
        text: LifeText.statusLine(root.snap, root.registry)
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        Accessible.role: Accessible.StaticText
        Accessible.name: text
    }
    Label {
        objectName: "life.frozen"
        Layout.fillWidth: true
        visible: root.life !== null
        text: LifeText.frozenText(root.frozenReason) !== "" ? LifeText.frozenText(root.frozenReason) : LifeText.COUNTING_TEXT
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
    }

    RulesCard {
        objectName: "life.rulesCard"
        Layout.fillWidth: true
        visible: root.life !== null && !root.life.armed && !root.pending
        host: root.host
        registry: root.registry
    }

    // ---------------------------------------------------------------- critical card
    GroupBox {
        objectName: "life.critical"
        Layout.fillWidth: true
        visible: root.critical !== null && root.life !== null
        title: root.life && root.life.fainted ? "Fainted" : "Needs you now"
        ColumnLayout {
            anchors.fill: parent
            spacing: 8
            Label {
                Layout.fillWidth: true
                text: !root.critical ? ""
                    : root.pending ? LifeText.pendingText(root.name)
                    : root.life && root.life.fainted ? LifeText.faintedText(root.name, root.critical.need)
                    : LifeText.criticalText(root.name, root.critical.need, root.critical.remaining_ms, root.life ? root.life.mode : "")
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                Accessible.name: text
            }
            Label {
                Layout.fillWidth: true
                visible: root.critical !== null && !root.pending && !(root.life && root.life.fainted)
                    && typeof root.critical.remaining_ms === "number"
                text: root.critical && typeof root.critical.remaining_ms === "number"
                    ? "Grace left: " + LifeText.formatActive(root.critical.remaining_ms) + ". The grace was fixed when this started and can only get longer."
                    : ""
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }
            Flow {
                Layout.fillWidth: true
                spacing: 8
                Button {
                    objectName: "life.careNow"
                    visible: !root.pending
                    text: root.critical ? "Give " + LifeText.careLabel(root.life ? root.life.appearance : "", root.critical.action || LifeText.NEED_ACTION[root.critical.need], root.registry).toLowerCase() : "Care"
                    enabled: !root.hostBusy && !root.pending
                    Accessible.description: "Fills this need right away"
                    onClicked: root.careNow()
                }
                Button {
                    objectName: "life.criticalPause"
                    visible: !root.pending && !root.paused
                    text: "Pause/Travel"
                    enabled: root.canPause
                    Accessible.description: "Stops the active-time clock until you resume"
                    onClicked: root.togglePause()
                }
            }
        }
    }

    // ---------------------------------------------------------------- needs
    GroupBox {
        objectName: "life.needs"
        Layout.fillWidth: true
        visible: root.life !== null && root.needs.length > 0
        title: "Needs · estimates in active time, as of the last refresh"
        ColumnLayout {
            anchors.fill: parent
            spacing: 4
            Repeater {
                model: root.needs
                delegate: Label {
                    required property var modelData
                    objectName: "life.need." + modelData.need
                    Layout.fillWidth: true
                    text: LifeText.needLine(modelData) + (modelData.lethal ? "" : " (warning only)")
                    textFormat: Text.PlainText
                    wrapMode: Text.WordWrap
                    Accessible.role: Accessible.StaticText
                    Accessible.name: text
                }
            }
        }
    }

    // ---------------------------------------------------------------- pause/travel
    GroupBox {
        Layout.fillWidth: true
        visible: root.life !== null
        title: "Pause/Travel"
        ColumnLayout {
            anchors.fill: parent
            spacing: 6
            Label {
                Layout.fillWidth: true
                text: "Pause/Travel stops the clock for breaks and trips, even when a need is urgent. It changes no needs and grants nothing; care and tasks still work."
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }
            Button {
                objectName: "life.pause"
                text: root.paused ? "Resume" : "Pause/Travel"
                enabled: root.canPause
                Accessible.description: root.pending ? "Not available while the current task finishes" : ""
                onClicked: root.togglePause()
            }
        }
    }

    // ---------------------------------------------------------------- mode
    GroupBox {
        objectName: "life.modeBox"
        Layout.fillWidth: true
        visible: root.life !== null && root.life.armed
        title: "Mode · " + (root.life ? LifeText.modeLabel(root.life.mode) : "")
        ColumnLayout {
            anchors.fill: parent
            spacing: 6
            Label {
                Layout.fillWidth: true
                text: root.life ? LifeText.modeExplain(root.life.mode, root.rules) : ""
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }
            Button {
                objectName: "life.modeOpen"
                visible: !root.modeConfirmOpen
                text: "Switch to " + LifeText.modeLabel(root.targetMode).toLowerCase() + "…"
                enabled: root.modeBlock === "" && !root.hostBusy
                onClicked: root.modeConfirmOpen = true
            }
            Label {
                objectName: "life.modeBlock"
                Layout.fillWidth: true
                visible: root.modeBlock !== ""
                text: root.modeBlock
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }
            ColumnLayout {
                visible: root.modeConfirmOpen
                Layout.fillWidth: true
                Label {
                    Layout.fillWidth: true
                    text: LifeText.modeExplain(root.targetMode, root.rules)
                        + (root.targetMode === "gentle" ? " Time already counted stays counted; switching back later needs " + root.name + " to be healthy." : "")
                    textFormat: Text.PlainText
                    wrapMode: Text.WordWrap
                }
                CheckBox {
                    id: modeAck
                    objectName: "life.modeAck"
                    text: "I understand the " + LifeText.modeLabel(root.targetMode).toLowerCase() + " rules"
                }
                RowLayout {
                    Button {
                        objectName: "life.modeConfirm"
                        text: "Switch to " + LifeText.modeLabel(root.targetMode).toLowerCase()
                        enabled: modeAck.checked && root.modeBlock === "" && !root.hostBusy
                        onClicked: root.switchMode()
                    }
                    Button {
                        text: "Keep " + (root.life ? LifeText.modeLabel(root.life.mode).toLowerCase() : "")
                        onClicked: { modeAck.checked = false; root.modeConfirmOpen = false }
                    }
                }
            }
        }
    }

    // ---------------------------------------------------------------- leniency
    GroupBox {
        objectName: "life.leniency"
        Layout.fillWidth: true
        visible: root.life !== null && root.life.armed
        title: "More lenient settings"
        GridLayout {
            anchors.fill: parent
            columns: 2
            Label {
                Layout.columnSpan: 2
                Layout.fillWidth: true
                text: "Only longer windows or slower needs are offered; nothing here can make "
                    + root.name + "'s life harder. Now: urgent after " + LifeText.formatSpan(root.rules.critical_after_ms)
                    + " empty, grace " + LifeText.formatSpan(root.rules.grace_ms) + " (active time)."
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }
            Label { text: "Grace"; textFormat: Text.PlainText }
            ComboBox {
                id: graceBox
                objectName: "life.graceBox"
                Layout.fillWidth: true
                textRole: "text"
                model: root.choiceModel(LifeText.graceHourChoices(root.rules.grace_ms), function (h) { return h + " h of active time" })
                Accessible.name: "Longer grace"
            }
            Label { text: "Urgent after"; textFormat: Text.PlainText }
            ComboBox {
                id: criticalBox
                objectName: "life.criticalBox"
                Layout.fillWidth: true
                textRole: "text"
                model: root.choiceModel(LifeText.criticalAfterHourChoices(root.rules.critical_after_ms), function (h) { return h + " h empty" })
                Accessible.name: "Longer time before a need becomes urgent"
            }
            Label { text: "Needs change"; textFormat: Text.PlainText }
            ComboBox {
                id: decayBox
                objectName: "life.decayBox"
                Layout.fillWidth: true
                textRole: "text"
                model: root.choiceModel(LifeText.decayPercentChoices(), function (p) { return p + "% of the current speed" })
                Accessible.name: "Slower needs"
            }
            Button {
                objectName: "life.leniencyApply"
                Layout.columnSpan: 2
                text: "Apply"
                enabled: root.canConfigure && (graceBox.currentIndex > 0 || criticalBox.currentIndex > 0 || decayBox.currentIndex > 0)
                onClicked: root.applyLeniency()
            }
        }
    }

    // ---------------------------------------------------------------- retire
    GroupBox {
        objectName: "life.retireBox"
        Layout.fillWidth: true
        visible: root.life !== null
        title: "Retire"
        ColumnLayout {
            anchors.fill: parent
            spacing: 6
            Label {
                Layout.fillWidth: true
                text: "Retiring ends " + root.name + "'s life peacefully and keeps their story in the memorial. It can't be undone. Your work, tasks and history are unchanged."
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }
            Button {
                objectName: "life.retireOpen"
                visible: root.retireStep === 0
                text: "Retire…"
                enabled: root.retireBlock === "" && !root.hostBusy
                onClicked: root.openRetire()
            }
            Label {
                objectName: "life.retireBlock"
                Layout.fillWidth: true
                visible: root.retireBlock !== ""
                text: root.retireBlock
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }
            ColumnLayout {
                visible: root.retireStep === 1
                Layout.fillWidth: true
                Label {
                    Layout.fillWidth: true
                    text: "Type " + root.name + " to confirm."
                    textFormat: Text.PlainText
                    wrapMode: Text.WordWrap
                }
                TextField {
                    id: retireName
                    objectName: "life.retireName"
                    Layout.fillWidth: true
                    maximumLength: 24
                    placeholderText: root.name
                    Accessible.name: "Type the companion's name to confirm retirement"
                }
                RowLayout {
                    Button {
                        objectName: "life.retireConfirm"
                        text: "Retire " + root.name
                        enabled: root.retireNameMatches && root.retireBlock === "" && !root.hostBusy
                        onClicked: root.confirmRetire()
                    }
                    Button {
                        objectName: "life.retireCancel"
                        text: "Keep " + root.name
                        onClicked: root.cancelRetire()
                    }
                }
            }
        }
    }

    Label {
        objectName: "life.notice"
        Layout.fillWidth: true
        visible: root.notice !== ""
        text: root.notice
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        Accessible.name: text
    }

    MemorialPanel {
        id: memorial
        objectName: "life.memorial"
        Layout.fillWidth: true
        host: root.host
        registry: root.registry
    }
}
