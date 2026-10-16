pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "LifeText.js" as LifeText

// One-time life-rules card for an unarmed life (docs/decisions/0001-v0.2-lifecycle.md D1). It shows the full
// rules with the life's exact numbers, a mode choice with Permadeath preselected, and a
// required acknowledgement. Only "Start life rules" sends anything: pet.rules
// {life_id, mode, acknowledge_rules: true}, reusing one retry key per choice. Until then the
// clock stays unarmed: no decay, no warnings, no countdown.
GroupBox {
    id: root
    required property var host
    property var registry: null
    property string mode: "permadeath"
    property string notice: ""
    property string retryKey: ""
    property string retrySignature: ""
    property int seqNo: 0
    signal armed()

    readonly property var snap: host && host.snapshot ? host.snapshot : null
    readonly property var life: snap && snap.life ? snap.life : null
    readonly property bool hostBusy: !!(host && host.busy)
    readonly property var rules: LifeText.rulesOf(snap, life ? life.pace : "steady")
    readonly property bool acknowledged: ackBox.checked
    readonly property bool canSend: life !== null && !life.armed && !life.death_pending && acknowledged && !hostBusy
    readonly property string name: LifeText.cleanName(life ? life.name : "")

    title: "Set up life rules"

    function params() {
        return { "life_id": root.life ? root.life.life_id : "", "mode": root.mode, "acknowledge_rules": true }
    }
    function reset() {
        root.mode = "permadeath"
        ackBox.checked = false
        root.notice = ""
    }
    function send() {
        if (!root.canSend)
            return
        var p = root.params()
        var signature = JSON.stringify(p)
        if (root.retryKey === "" || root.retrySignature !== signature) {
            root.seqNo += 1
            root.retryKey = String(root.host.clientTag || "ui") + "-rules-" + Date.now().toString(36) + "-" + root.seqNo
            root.retrySignature = signature
        }
        root.notice = ""
        root.host.request("pet.rules", p, function (resp) {
            if (resp && resp.ok) {
                root.retryKey = ""
                ackBox.checked = false
                root.notice = "Life rules are set. Active time now counts for " + root.name + "."
                root.armed()
                if (!root.host.busy)
                    root.host.request("state.get", {}, function () {})
            } else {
                var e = resp && resp.error ? resp.error : {}
                root.notice = "Nothing changed: " + String(e.message || e.code || "unknown error")
            }
        }, root.retryKey)
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 8

        Label {
            Layout.fillWidth: true
            text: root.name + " is here with you, and nothing changes on its own until you choose how their life works. Read the rules, pick a mode, then start."
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
        }
        Repeater {
            objectName: "rulesCard.lines"
            model: LifeText.rulesLines(root.rules, root.mode, root.name)
            delegate: Label {
                required property string modelData
                Layout.fillWidth: true
                text: "• " + modelData
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }
        }
        Label {
            Layout.fillWidth: true
            visible: root.life !== null && root.life.pace === "playtest"
            text: "Playtest pace: about 20 times faster than steady."
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            font.bold: true
        }

        ButtonGroup { id: modeGroup }
        RadioButton {
            objectName: "rulesCard.permadeath"
            Layout.fillWidth: true
            text: "Permadeath"
            checked: root.mode === "permadeath"
            ButtonGroup.group: modeGroup
            Accessible.description: LifeText.modeExplain("permadeath", root.rules)
            onClicked: root.mode = "permadeath"
        }
        Label {
            Layout.fillWidth: true
            leftPadding: 28
            text: LifeText.modeExplain("permadeath", root.rules)
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
        }
        RadioButton {
            objectName: "rulesCard.gentle"
            Layout.fillWidth: true
            text: "Gentle"
            checked: root.mode === "gentle"
            ButtonGroup.group: modeGroup
            Accessible.description: LifeText.modeExplain("gentle", root.rules)
            onClicked: root.mode = "gentle"
        }
        Label {
            Layout.fillWidth: true
            leftPadding: 28
            text: LifeText.modeExplain("gentle", root.rules) + " You can switch to gentle later at any time; switching back to permadeath needs " + root.name + " to be healthy."
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
        }
        CheckBox {
            id: ackBox
            objectName: "rulesCard.ack"
            Layout.fillWidth: true
            text: "I've read how this works"
        }
        Button {
            objectName: "rulesCard.start"
            text: "Start life rules (" + LifeText.modeLabel(root.mode) + ")"
            enabled: root.canSend
            Accessible.description: root.acknowledged ? "Starts the active-time clock for " + root.name
                                                      : "Tick \"I've read how this works\" first"
            onClicked: root.send()
        }
        Label {
            objectName: "rulesCard.notice"
            Layout.fillWidth: true
            visible: root.notice !== ""
            text: root.notice
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            Accessible.name: text
        }
    }
}
