pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Draft editor. Saves the draft in Rust (task.create / task.update), asks Rust for a
// preflight, shows its summary, and enables Start only for a fresh, acknowledged
// preflight of the current revision. Any edit clears the preflight and acknowledgement.
// Start runs only from the Start button's click handler.
ColumnLayout {
    id: root
    required property var host
    signal closed

    readonly property int maxInstructionBytes: 16384
    // Rust refuses drafts whose folder, scope and instructions exceed 32 KiB as JSON.
    readonly property int maxDraftJsonBytes: 32768
    property string taskId: ""
    property int revision: 0
    property int editGeneration: 0
    property bool dirty: true
    property var preflight: null
    property string providerId: ""
    property string formError: ""
    property bool loading: false

    readonly property var snap: host && host.snapshot ? host.snapshot : null
    readonly property var providers: snap && snap.providers ? snap.providers : []
    readonly property bool hostBusy: !!(host && host.busy)
    readonly property var scopeList: scopeArea.text.split("\n").map(function (s) { return s.trim() }).filter(function (s) { return s.length > 0 })
    readonly property int instructionBytes: utf8Bytes(instructionsArea.text)
    readonly property int draftJsonBytes: utf8Bytes(JSON.stringify([folderField.text.trim(), scopeList, instructionsArea.text]))
    readonly property bool providerAvailable: providerInfo(providerId) !== null && providerInfo(providerId).available === true
    readonly property bool draftValid: providerAvailable && folderField.text.trim().length > 0 && scopeList.length > 0
        && instructionsArea.text.trim().length > 0 && instructionBytes <= maxInstructionBytes
        && draftJsonBytes <= maxDraftJsonBytes
    readonly property bool preflightFresh: preflight !== null && !dirty && taskId !== ""
        && preflight.task_id === taskId && preflight.revision === revision
    spacing: 10

    function utf8Bytes(s) {
        var n = 0
        for (var i = 0; i < s.length; i++) {
            var c = s.charCodeAt(i)
            if (c < 0x80) n += 1
            else if (c < 0x800) n += 2
            else if (c >= 0xd800 && c <= 0xdbff) { n += 4; i++ }
            else n += 3
        }
        return n
    }
    function providerInfo(id) {
        for (var i = 0; i < root.providers.length; i++)
            if (root.providers[i].id === id)
                return root.providers[i]
        return null
    }
    function firstAvailableProvider() {
        for (var i = 0; i < root.providers.length; i++)
            if (root.providers[i].available === true)
                return root.providers[i].id
        return ""
    }
    function edited() {
        if (root.loading)
            return
        root.editGeneration += 1
        root.dirty = true
        root.preflight = null
        root.formError = ""
        ackBox.checked = false
    }
    function setFields(category, provider, folder, scope, instructions, outcome, stepMs) {
        root.loading = true
        categoryBox.currentIndex = Math.max(0, categoryBox.indexOfValue(category))
        root.providerId = provider
        folderField.text = folder
        scopeArea.text = scope.join("\n")
        instructionsArea.text = instructions
        outcomeBox.currentIndex = Math.max(0, outcomeBox.indexOfValue(outcome))
        stepSpin.value = stepMs
        root.loading = false
        root.editGeneration += 1
        root.preflight = null
        ackBox.checked = false
        root.formError = ""
    }
    function reset() {
        root.taskId = ""
        root.revision = 0
        root.setFields("hunger", root.firstAvailableProvider(), "", [], "", "artifact", 0)
        root.dirty = true
    }
    // Fills the form from a full task summary (task.get), never from a list brief.
    function loadDraft(summary) {
        var mock = summary.mock || {}
        root.taskId = summary.task_id
        root.revision = summary.revision
        root.setFields(summary.category, summary.provider, summary.folder || "", summary.scope || [],
                       summary.instructions || "", mock.outcome || "artifact", mock.step_ms || 0)
        root.dirty = true
    }
    // state.get lists briefs without folder, scope or full instructions, so a draft is
    // loaded with task.get before editing. Calls `done(ok)` after the response.
    function loadDraftById(taskId, done) {
        root.host.request("task.get", { "task_id": taskId }, function (resp) {
            var ok = resp.ok === true && resp.result && resp.result.state === "draft"
            if (ok)
                root.loadDraft(resp.result)
            if (done)
                done(ok)
        })
    }
    function draftParams() {
        var p = {
            "category": categoryBox.currentValue,
            "provider": root.providerId,
            "folder": folderField.text.trim(),
            "scope": root.scopeList,
            "instructions": instructionsArea.text
        }
        if (root.providerId === "mock")
            p.mock = { "outcome": outcomeBox.currentValue, "step_ms": stepSpin.value }
        return p
    }
    function refresh() {
        root.host.request("state.get", {}, function () {})
    }
    function saveAndPreflight() {
        var gen = root.editGeneration
        var params = root.draftParams()
        var method = "task.create"
        if (root.taskId !== "") {
            method = "task.update"
            params.task_id = root.taskId
            params.expected_revision = root.revision
        }
        root.formError = ""
        root.host.request(method, params, function (resp) {
            if (!resp.ok) {
                root.refresh()
                return
            }
            root.taskId = resp.result.task_id
            root.revision = resp.result.revision
            if (gen !== root.editGeneration) {
                root.refresh()
                return
            }
            root.dirty = false
            root.host.request("task.preflight", { "task_id": root.taskId, "revision": root.revision }, function (pre) {
                if (pre.ok && gen === root.editGeneration && pre.result.task_id === root.taskId
                        && pre.result.revision === root.revision)
                    root.preflight = pre.result
                root.refresh()
            })
        })
    }
    // Called only from the Start button's onClicked.
    function startClicked() {
        if (!root.preflightFresh || !ackBox.checked || root.hostBusy)
            return
        if (Date.now() >= root.preflight.expires_at_ms) {
            root.preflight = null
            ackBox.checked = false
            root.formError = "The preflight expired. Run preflight again before starting."
            return
        }
        var params = { "task_id": root.taskId, "revision": root.revision, "preflight_token": root.preflight.preflight_token }
        root.preflight = null
        ackBox.checked = false
        root.host.request("task.start", params, function (resp) {
            if (resp.ok) {
                root.taskId = ""
                root.closed()
            } else {
                root.refresh()
            }
        })
    }

    Label { text: "New task"; font.bold: true; font.pixelSize: 18 }

    Label { text: "Care category" }
    ComboBox {
        id: categoryBox
        Layout.fillWidth: true
        textRole: "text"
        valueRole: "value"
        model: [
            { "value": "hunger", "text": "Hunger: fix a specific bug" },
            { "value": "thirst", "text": "Thirst: improve speed or quality" },
            { "value": "mental_health", "text": "Mental health: learn, explain or plan" },
            { "value": "hygiene", "text": "Hygiene: remove selected obsolete files" }
        ]
        Accessible.name: "Care category"
        onActivated: root.edited()
    }

    Label { text: "Provider" }
    ColumnLayout {
        Layout.fillWidth: true
        spacing: 0
        Repeater {
            model: root.providers
            // Not checkable: the click only sets providerId, so `checked` stays bound to it.
            delegate: RadioButton {
                id: providerButton
                required property var modelData
                readonly property string providerKey: modelData.id
                Layout.fillWidth: true
                checkable: false
                enabled: modelData.available === true
                checked: providerButton.providerKey === root.providerId
                text: String(modelData.label) + (modelData.available === true ? "" : " (Not yet supported on this version)")
                Accessible.name: text
                // Button labels default to AutoText; force plain text for snapshot strings.
                contentItem: Text {
                    leftPadding: providerButton.indicator.width + providerButton.spacing
                    verticalAlignment: Text.AlignVCenter
                    text: providerButton.text
                    textFormat: Text.PlainText
                    wrapMode: Text.WordWrap
                    font: providerButton.font
                    color: providerButton.palette.windowText
                    opacity: providerButton.enabled ? 1 : 0.5
                }
                onClicked: {
                    if (root.providerId !== providerButton.providerKey) {
                        root.providerId = providerButton.providerKey
                        root.edited()
                    }
                }
            }
        }
        Label {
            visible: root.providers.length === 0
            text: "No provider list loaded. Refresh the panel first."
            textFormat: Text.PlainText
        }
    }

    Label { text: "Folder (absolute path; CodePet validates it)" }
    TextField {
        id: folderField
        Layout.fillWidth: true
        maximumLength: 4096
        placeholderText: "/path/to/project"
        Accessible.name: "Project folder"
        onTextEdited: root.edited()
    }

    Label { text: "Scope (one relative path per line)" }
    TextArea {
        id: scopeArea
        Layout.fillWidth: true
        Layout.preferredHeight: 70
        textFormat: TextEdit.PlainText
        placeholderText: "src/example.rs"
        Accessible.name: "Scope, one relative path per line"
        onTextChanged: root.edited()
    }

    Label {
        text: "Instructions (" + root.instructionBytes + " / " + root.maxInstructionBytes + " bytes)"
        color: root.instructionBytes > root.maxInstructionBytes ? "#9b1c1c" : palette.windowText
    }
    TextArea {
        id: instructionsArea
        Layout.fillWidth: true
        Layout.preferredHeight: 110
        textFormat: TextEdit.PlainText
        wrapMode: TextEdit.Wrap
        placeholderText: "One specific outcome, then any bounded detail"
        Accessible.name: "Task instructions"
        onTextChanged: root.edited()
    }
    Label {
        visible: root.instructionBytes > root.maxInstructionBytes
        text: "Too long: instructions are limited to 16 KiB."
        font.bold: true
    }
    Label {
        Layout.fillWidth: true
        visible: root.instructionBytes <= root.maxInstructionBytes && root.draftJsonBytes > root.maxDraftJsonBytes
        text: "Too long: folder, scope and instructions together are limited to 32 KiB (quotes and control characters count extra)."
        wrapMode: Text.WordWrap
        font.bold: true
    }

    GroupBox {
        title: "Mock outcome (simulation only)"
        Layout.fillWidth: true
        visible: root.providerId === "mock"
        ColumnLayout {
            anchors.fill: parent
            ComboBox {
                id: outcomeBox
                Layout.fillWidth: true
                textRole: "text"
                valueRole: "value"
                model: [
                    { "value": "artifact", "text": "Artifact for review (not verified)" },
                    { "value": "provider_failure", "text": "Provider failure (no review)" },
                    { "value": "check_failed", "text": "Artifact with a failed check (SIMULATED)" }
                ]
                Accessible.name: "Mock outcome"
                onActivated: root.edited()
            }
            RowLayout {
                Label { text: "Delay between steps (ms)" }
                SpinBox {
                    id: stepSpin
                    from: 0
                    to: 2000
                    stepSize: 100
                    editable: true
                    Accessible.name: "Mock delay between steps in milliseconds"
                    onValueModified: root.edited()
                }
            }
        }
    }

    Button {
        objectName: "taskForm.preflight"
        text: root.taskId === "" ? "Save draft and run preflight" : "Save changes and run preflight"
        enabled: root.draftValid && !root.hostBusy
        onClicked: root.saveAndPreflight()
    }

    GroupBox {
        title: "Preflight summary"
        Layout.fillWidth: true
        visible: root.preflight !== null
        ColumnLayout {
            anchors.fill: parent
            Label {
                Layout.fillWidth: true
                text: {
                    var p = root.preflight
                    if (!p)
                        return ""
                    var s = p.summary || {}
                    return "Revision: " + p.revision
                        + "\nCategory: " + s.category
                        + "\nProvider: " + s.provider
                        + "\nFolder: " + s.folder
                        + "\nScope: " + (s.scope || []).join(", ")
                        + "\nInstructions SHA-256: " + s.instructions_sha256
                        + "\nNetwork: " + s.network
                        + "\nWritable: " + s.writable
                        + "\nTimeout: " + s.timeout_seconds + " s"
                        + "\nExpires: " + new Date(p.expires_at_ms).toLocaleTimeString()
                }
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }
        }
    }

    CheckBox {
        id: ackBox
        objectName: "taskForm.ack"
        enabled: root.preflightFresh
        text: "I reviewed this exact revision and want Start to run it"
        Accessible.name: text
    }

    Label {
        Layout.fillWidth: true
        visible: root.formError !== ""
        text: root.formError
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
        font.bold: true
    }

    RowLayout {
        Button {
            objectName: "taskForm.start"
            text: "Start"
            enabled: root.preflightFresh && ackBox.checked && !root.hostBusy
            Accessible.description: "Starts the reviewed task revision"
            onClicked: root.startClicked()
        }
        Button {
            text: "Back"
            enabled: !root.hostBusy
            onClicked: root.closed()
        }
    }
}
