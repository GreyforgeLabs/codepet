pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "LifeText.js" as LifeText

// Explicit adoption (docs/decisions/0001-v0.2-lifecycle.md D8). Shown only when there is no living companion.
// Species pick, a name (max 24, defaulting to the species' default name), Mode with
// Permadeath preselected, Pace (Steady default / Playtest) and a required acknowledgement.
// "Adopt <Name> (starts at level 1)" sends pet.adopt and reuses one retry key while the
// form is unchanged. Adoption is never automatic; the panel issues no request on its own
// except `lifecycle.rules` when the host calls loadRules() after an explicit open.
GroupBox {
    id: root
    required property var host
    property var registry: null
    property string species: "cat"
    property bool nameEdited: false
    property string mode: "permadeath"
    property string pace: "steady"
    property string notice: ""
    property string retryKey: ""
    property string retrySignature: ""
    property int seqNo: 0
    // Steady/playtest presets from `lifecycle.rules`; built-in RULES_V1 until loaded.
    property var presets: LifeText.presetsFrom(null)
    property bool rulesLoaded: false
    signal adopted()

    readonly property var snap: host && host.snapshot ? host.snapshot : null
    readonly property bool hostBusy: !!(host && host.busy)
    readonly property var latest: snap && snap.memorial && snap.memorial.latest ? snap.memorial.latest : null
    readonly property var slotStates: ["preparing", "running", "waiting_for_user", "verifying", "review_ready", "cancelling", "recovery_required"]
    readonly property bool slotBusy: !!(snap && snap.active_task && slotStates.indexOf(String(snap.active_task.state)) >= 0)
    readonly property bool lifeExists: !!(snap && snap.life)
    readonly property var rules: pace === "playtest" ? presets.playtest : presets.steady
    readonly property string name: LifeText.normalizeName(nameField.text)
    readonly property string blockReason: snap === null ? "Refresh to load CodePet first."
        : lifeExists ? "You already have a companion. CodePet keeps one companion at a time."
        : slotBusy ? "Finish, review or discard the current task first; adoption waits for a free task slot."
        : name === "" ? "Choose a name of 1 to 24 characters."
        : !ackBox.checked ? "Tick \"I've read how this works\" to adopt."
        : ""
    readonly property bool canAdopt: blockReason === "" && !hostBusy

    title: "Adopt a companion"

    onSpeciesChanged: {
        picker.highlightedId = root.species
        if (!root.nameEdited)
            nameField.text = LifeText.defaultName(root.species, root.registry)
    }

    function params() {
        return { "species": root.species, "name": root.name, "mode": root.mode, "pace": root.pace,
                 "acknowledge_rules": true }
    }
    function chooseSpecies(id) {
        if (LifeText.isSpecies(id))
            root.species = id
    }
    function reset() {
        root.species = "cat"
        root.nameEdited = false
        nameField.text = LifeText.defaultName("cat", root.registry)
        root.mode = "permadeath"
        root.pace = "steady"
        ackBox.checked = false
        root.notice = ""
    }
    // Explicit read of the canonical presets, for the exact numbers. Called by the host view
    // after its own refresh finished, so it never collides with another request.
    function loadRules(done) {
        if (root.hostBusy) {
            if (done) done(false)
            return
        }
        root.host.request("lifecycle.rules", {}, function (resp) {
            if (resp && resp.ok) {
                root.presets = LifeText.presetsFrom(resp.result)
                root.rulesLoaded = true
            }
            if (done) done(!!(resp && resp.ok))
        })
    }
    function adopt() {
        if (!root.canAdopt)
            return
        var p = root.params()
        var signature = JSON.stringify(p)
        if (root.retryKey === "" || root.retrySignature !== signature) {
            root.seqNo += 1
            root.retryKey = String(root.host.clientTag || "ui") + "-adopt-" + Date.now().toString(36) + "-" + root.seqNo
            root.retrySignature = signature
        }
        root.notice = ""
        var adoptedName = p.name
        root.host.request("pet.adopt", p, function (resp) {
            if (resp && resp.ok) {
                root.retryKey = ""
                ackBox.checked = false
                root.notice = "Welcome, " + adoptedName + ". They start at level 1 with one free care routine."
                root.adopted()
                if (!root.host.busy)
                    root.host.request("state.get", {}, function () {})
            } else {
                var e = resp && resp.error ? resp.error : {}
                root.notice = "No companion was adopted: " + String(e.message || e.code || "unknown error")
            }
        }, root.retryKey)
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 8

        Label {
            objectName: "adoption.memorial"
            Layout.fillWidth: true
            visible: root.latest !== null
            text: root.latest ? LifeText.endText(root.latest, root.registry) : ""
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            Accessible.name: text
        }
        Label {
            Layout.fillWidth: true
            text: "A new companion starts at level 1 in its young stage, with needs at 70 and one free care routine. Your tasks, results and lifetime progress stay exactly as they are."
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
        }

        PetChooser {
            id: picker
            objectName: "adoption.chooser"
            Layout.fillWidth: true
            host: root.host
            registry: root.registry
            mode: "pick"
            highlightedId: root.species
            onPicked: id => root.chooseSpecies(id)
        }

        RowLayout {
            Layout.fillWidth: true
            Label { text: "Name"; textFormat: Text.PlainText }
            TextField {
                id: nameField
                objectName: "adoption.name"
                Layout.fillWidth: true
                maximumLength: 24
                text: LifeText.defaultName(root.species, root.registry)
                placeholderText: LifeText.defaultName(root.species, root.registry)
                Accessible.name: "Companion name, up to 24 characters"
                onTextEdited: root.nameEdited = true
            }
        }

        Label { text: "Mode"; font.bold: true; textFormat: Text.PlainText }
        ButtonGroup { id: modeGroup }
        RadioButton {
            objectName: "adoption.permadeath"
            text: "Permadeath"
            checked: root.mode === "permadeath"
            ButtonGroup.group: modeGroup
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
            objectName: "adoption.gentle"
            text: "Gentle"
            checked: root.mode === "gentle"
            ButtonGroup.group: modeGroup
            onClicked: root.mode = "gentle"
        }
        Label {
            Layout.fillWidth: true
            leftPadding: 28
            text: LifeText.modeExplain("gentle", root.rules)
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
        }

        Label { text: "Pace (chosen once, at adoption)"; font.bold: true; textFormat: Text.PlainText }
        ButtonGroup { id: paceGroup }
        RadioButton {
            objectName: "adoption.steady"
            text: "Steady"
            checked: root.pace === "steady"
            ButtonGroup.group: paceGroup
            onClicked: root.pace = "steady"
        }
        Label {
            Layout.fillWidth: true
            leftPadding: 28
            text: LifeText.paceExplain("steady", root.presets.steady)
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
        }
        RadioButton {
            objectName: "adoption.playtest"
            text: "Playtest"
            checked: root.pace === "playtest"
            ButtonGroup.group: paceGroup
            onClicked: root.pace = "playtest"
        }
        Label {
            Layout.fillWidth: true
            leftPadding: 28
            text: LifeText.paceExplain("playtest", root.presets.playtest)
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
        }

        Label { text: "How this works"; font.bold: true; textFormat: Text.PlainText }
        Repeater {
            model: LifeText.rulesLines(root.rules, root.mode, root.name !== "" ? root.name : LifeText.defaultName(root.species, root.registry))
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
            visible: !root.rulesLoaded
            text: "Numbers shown are the lifecycle-v1 defaults."
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            font.italic: true
        }
        CheckBox {
            id: ackBox
            objectName: "adoption.ack"
            text: "I've read how this works"
        }
        Label {
            objectName: "adoption.blockReason"
            Layout.fillWidth: true
            visible: root.blockReason !== ""
            text: root.blockReason
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
        }
        Button {
            id: adoptButton
            objectName: "adoption.adopt"
            text: "Adopt " + (root.name !== "" ? root.name : "a companion") + " (starts at level 1)"
            enabled: root.canAdopt
            Accessible.description: root.blockReason !== "" ? root.blockReason
                : LifeText.modeLabel(root.mode) + ", " + LifeText.paceLabel(root.pace) + " pace"
            onClicked: root.adopt()
            // Button labels default to AutoText and the name is user text: force plain text.
            contentItem: Text {
                text: adoptButton.text
                textFormat: Text.PlainText
                font: adoptButton.font
                color: adoptButton.palette.buttonText
                opacity: adoptButton.enabled ? 1 : 0.5
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
        }
        Label {
            objectName: "adoption.notice"
            Layout.fillWidth: true
            visible: root.notice !== ""
            text: root.notice
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            Accessible.name: text
        }
    }
}
