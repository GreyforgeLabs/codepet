pragma ComponentBehavior: Bound
import QtQuick
import "CodePetBar.js" as Model

// How it works (SPEC-v0.3 §3.6): the grown-up page and the only place for technical words,
// numbers and app details. Rules, grown-up settings (Hardcore behind the typed name, Count
// background helpers, Forget coding history), Apps and Privacy. It may scroll.
Item {
    id: help
    required property var app
    readonly property var t: app.t
    readonly property var life: app.life
    readonly property bool hardcore: !!(life && life.mode === "permadeath")
    readonly property bool lively: !!(life && life.care_tempo === "lively")
    readonly property var careRules: app.doc && app.doc.care_rules ? app.doc.care_rules : null
    readonly property bool helpersOn: !!(app.ctl && app.ctl.snapshot && app.ctl.snapshot.count_helpers === true)
    property bool askHardcore: false
    property bool askForget: false
    property string message: ""

    implicitWidth: t.u(448)
    implicitHeight: t.u(536)

    function setHardcore(on) {
        help.message = ""
        if (!help.life) return
        if (on && help.lively) { help.message = help.app.s("care.steady_first"); return }
        if (on) { help.askHardcore = true; Qt.callLater(function () { hardcoreName.focusField() }); return }
        help.askHardcore = false
        help.app.request("pet.rules", { life_id: help.life.life_id, mode: "gentle", acknowledge_rules: true }, function (resp) {
            if (!resp.ok) help.app.failed(resp)
        })
    }
    function confirmHardcore() {
        if (!help.life) return
        help.app.request("pet.rules", { life_id: help.life.life_id, mode: "permadeath", acknowledge_rules: true,
                                        confirm_name: hardcoreName.text }, function (resp) {
            if (resp.ok) { help.askHardcore = false; hardcoreName.text = ""; return }
            var code = resp.error ? resp.error.code : ""
            help.message = code === "INVALID_STATE" ? help.app.s("help.hardcore_unhealthy") : help.app.s("err.generic")
        })
    }
    function setHelpers(on) {
        help.app.request("app.set_helpers", { enabled: on }, function (resp) { if (!resp.ok) help.app.failed(resp) })
    }
    function setTempo(on) {
        help.message = ""
        if (!help.life) return
        if (on && (help.hardcore || help.life.pace !== "steady")) {
            help.message = help.app.s("care.lively_needs_gentle")
            return
        }
        help.app.request("pet.care_tempo", { life_id: help.life.life_id,
            expected_revision: help.life.revision, tempo: on ? "lively" : "steady" }, function (resp) {
            if (!resp.ok) help.app.failed(resp)
        })
    }
    function forget() {
        help.askForget = false
        help.app.request("agent.forget_history", {}, function (resp) { if (!resp.ok) help.app.failed(resp) })
    }

    component Heading: Text {
        width: parent ? parent.width : 0
        topPadding: help.t.u(10)
        textFormat: Text.PlainText
        font.family: help.t.family
        font.pixelSize: help.t.fontHeading
        font.bold: true
        color: help.t.text
    }
    component Body: Text {
        width: parent ? parent.width : 0
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        font.family: help.t.family
        font.pixelSize: help.t.fontBody
        color: help.t.text
        lineHeight: 1.15
    }

    Flickable {
        id: flick
        objectName: "cp.helpFlick"
        anchors.fill: parent
        contentWidth: width
        contentHeight: column.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds

        Column {
            id: column
            width: flick.width - help.t.u(8)
            spacing: help.t.u(8)

            Body {
                visible: help.app.ctl && help.app.ctl.testState === true
                text: "Test state: " + (help.app.ctl && help.app.ctl.stateHome ? help.app.ctl.stateHome : "")
                color: help.t.urgent
            }

            Heading { text: help.app.s("help.rules") }
            Body { text: help.app.s("help.current_rules_body") }
            Body { text: help.app.s("help.current_pace_body") }
            MenuRow {
                objectName: "cp.help.tempo"
                visible: help.life !== null
                t: help.t
                width: parent.width
                icon: "care-floor/play"
                text: help.app.s("care.tempo")
                kind: "switch"
                checked: help.lively
                textOn: help.app.s("care.lively")
                textOff: help.app.s("care.steady")
                onToggled: on => help.setTempo(on)
            }
            Body { text: help.app.s(help.lively ? "help.current_lively_body" : "help.current_steady_body") }
            Body {
                visible: !!help.careRules
                text: help.careRules ? help.app.s("help.current_effective_rates", {
                    food: help.careRules.decay_per_hour[0] / 100,
                    water: help.careRules.decay_per_hour[1] / 100,
                    mood: help.careRules.decay_per_hour[2] / 100,
                    hygiene: help.careRules.decay_per_hour[3] / 100,
                    minutes: help.careRules.full_hold_ms / 60000
                }) : ""
            }

            Heading { text: help.app.s("help.grownup") }
            MenuRow {
                objectName: "cp.help.hardcore"
                visible: help.life !== null
                t: help.t
                width: parent.width
                icon: "status/flame_hardcore"
                text: help.app.s("help.hardcore")
                kind: "switch"
                checked: help.hardcore || help.askHardcore
                textOn: help.app.s("header.on")
                textOff: help.app.s("header.off")
                onToggled: c => help.setHardcore(c)
            }
            Column {
                objectName: "cp.help.hardcoreAsk"
                visible: help.askHardcore && !help.hardcore
                width: parent.width
                spacing: help.t.u(8)
                Text {
                    width: parent.width
                    text: help.app.s("help.hardcore_warn")
                    textFormat: Text.PlainText
                    wrapMode: Text.WordWrap
                    font.family: help.t.family
                    font.pixelSize: help.t.fontHeading
                    font.bold: true
                    color: help.t.urgent
                }
                TypedField {
                    id: hardcoreName
                    objectName: "cp.help.hardcoreName"
                    t: help.t
                    width: parent.width
                    placeholder: help.app.s("help.hardcore_type")
                    onAccepted: if (help.life && Model.cleanName(text) === help.life.name) help.confirmHardcore()
                }
                BigButton {
                    objectName: "cp.help.hardcoreOn"
                    visible: !!(help.life && Model.cleanName(hardcoreName.text) === help.life.name)
                    t: help.t
                    kind: "danger"
                    width: parent.width
                    icon: "status/flame_hardcore"
                    text: help.app.s("help.btn_hardcore_on")
                    onTapped: help.confirmHardcore()
                }
                BigButton {
                    t: help.t
                    kind: "secondary"
                    width: parent.width
                    text: help.app.s("btn.cancel")
                    onTapped: { help.askHardcore = false; hardcoreName.text = ""; help.message = "" }
                }
            }
            Text {
                visible: help.message !== ""
                width: parent.width
                text: help.message
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                font.family: help.t.family
                font.pixelSize: help.t.fontHeading
                font.bold: true
                color: help.t.urgent
            }
            Body { text: help.app.s("help.hardcore.body") }

            MenuRow {
                objectName: "cp.help.helpers"
                t: help.t
                width: parent.width
                icon: "status/robot_helpers"
                text: help.app.s("help.helpers")
                kind: "switch"
                checked: help.helpersOn
                textOn: help.app.s("header.on")
                textOff: help.app.s("header.off")
                onToggled: c => help.setHelpers(c)
            }
            Body { text: help.app.s("help.helpers.body") }

            BigButton {
                objectName: "cp.help.forget"
                visible: !help.askForget
                t: help.t
                kind: "secondary"
                width: parent.width
                text: help.app.s("help.forget")
                onTapped: help.askForget = true
            }
            Column {
                visible: help.askForget
                width: parent.width
                spacing: help.t.u(8)
                Text {
                    width: parent.width
                    text: help.app.s("help.ask_forget")
                    textFormat: Text.PlainText
                    font.family: help.t.family
                    font.pixelSize: help.t.fontHeading
                    font.bold: true
                    color: help.t.text
                }
                BigButton {
                    objectName: "cp.help.forgetYes"
                    t: help.t
                    kind: "danger"
                    width: parent.width
                    text: help.app.s("help.btn_forget")
                    onTapped: help.forget()
                }
                BigButton {
                    t: help.t
                    kind: "secondary"
                    width: parent.width
                    text: help.app.s("btn.keep")
                    onTapped: help.askForget = false
                }
            }
            Body { text: help.app.s("help.forget.body") }

            Heading { text: help.app.s("help.apps") }
            Body { text: help.app.s("help.current_apps_body") }

            Heading { text: help.app.s("help.privacy") }
            Body { text: help.app.s("help.current_privacy_body") }
            Item { width: 1; height: help.t.u(16) }
        }
    }
}
