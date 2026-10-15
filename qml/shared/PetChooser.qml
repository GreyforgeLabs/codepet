pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "LifeText.js" as LifeText

// Companion look chooser (SPEC-v0.2 §12). Two modes:
//   pick: species pick inside adoption; emits picked(id); sends nothing itself.
//   look: cosmetic appearance switch for the living companion; "Use this look" sends
//         pet.select {life_id, pet_id}, reusing one retry key per choice.
// Thumbnails are static images (thumb-s<n>.webp) created in a Loader only while the
// chooser is visible, so a hidden chooser loads no pack. A missing thumbnail shows the
// species initial instead; it never issues a request. Plain text only.
FocusScope {
    id: root
    required property var host
    property string mode: "look"
    // Optional generated pet registry (PetRegistry.js REGISTRY); LifeText falls back to the roster.
    property var registry: null
    property string highlightedId: ""
    property string notice: ""
    property string retryKey: ""
    property string retrySignature: ""
    property int choiceSeq: 0
    signal picked(string petId)
    signal closed()

    readonly property var ids: LifeText.speciesIds()
    readonly property var snap: host && host.snapshot ? host.snapshot : null
    readonly property var life: snap && snap.life ? snap.life : null
    readonly property bool hostBusy: !!(host && host.busy)
    readonly property int locks: host && typeof host.presentationLocks === "number" ? host.presentationLocks : 0
    readonly property int stage: mode === "look" && life && life.stage >= 1 && life.stage <= 3 ? life.stage : 1
    readonly property string currentLook: life ? String(life.appearance || "") : ""
    readonly property int currentIndex: Math.max(0, ids.indexOf(highlightedId))
    // Why a look cannot change right now, or "" when it can.
    readonly property string lockReason: hostBusy ? "Waiting for the last request to finish."
        : mode === "pick" ? ""
        : snap === null ? "Refresh to load your companion first."
        : life === null ? "Adopt a companion first; looks belong to a living companion."
        : life.death_pending ? "Looks stay as they are while the current task finishes."
        : locks > 0 ? "A care routine is running. Looks can change when it finishes."
        : ""
    readonly property bool canSelect: lockReason === "" && mode === "look" && highlightedId !== ""
        && highlightedId !== currentLook
    readonly property string cosmeticText: "Changing looks is cosmetic: " + LifeText.cleanName(life ? life.name : "")
        + "'s needs, rest, level and countdowns stay exactly the same."

    implicitWidth: 520
    implicitHeight: layout.implicitHeight
    activeFocusOnTab: true

    Component.onCompleted: if (highlightedId === "") highlightedId = currentLook !== "" ? currentLook : ids[0]
    onCurrentLookChanged: if (mode === "look" && currentLook !== "" && highlightedId === "") highlightedId = currentLook

    function thumbUrl(id, stage) {
        if (!/^[a-z_]{2,24}$/.test(String(id)) || stage < 1 || stage > 3)
            return ""
        return Qt.resolvedUrl("../assets/pets/" + id + "/thumb-s" + stage + ".webp")
    }
    function highlight(index) {
        if (index < 0 || index >= root.ids.length)
            return
        root.highlightedId = root.ids[index]
        root.notice = ""
        if (root.mode === "pick")
            root.picked(root.highlightedId)
    }
    // Keyboard: arrows move in the 5 × 3 grid, Enter activates, Esc closes.
    function handleKey(key) {
        var i = root.currentIndex
        if (key === Qt.Key_Right) root.highlight(Math.min(root.ids.length - 1, i + 1))
        else if (key === Qt.Key_Left) root.highlight(Math.max(0, i - 1))
        else if (key === Qt.Key_Down) root.highlight(i + 5 < root.ids.length ? i + 5 : i)
        else if (key === Qt.Key_Up) root.highlight(i - 5 >= 0 ? i - 5 : i)
        else if (key === Qt.Key_Return || key === Qt.Key_Enter) root.activate()
        else if (key === Qt.Key_Escape) root.closed()
        else return false
        return true
    }
    function activate() {
        if (root.mode === "pick")
            root.picked(root.highlightedId)
        else
            root.useLook()
    }
    // Sends pet.select for the highlighted look. A retry of the same choice reuses its key,
    // so a lost response can never apply twice; a different choice gets a new key.
    function useLook() {
        if (!root.canSelect)
            return
        var params = { "life_id": root.life.life_id, "pet_id": root.highlightedId }
        var signature = JSON.stringify(params)
        if (root.retryKey === "" || root.retrySignature !== signature) {
            root.choiceSeq += 1
            root.retryKey = String(root.host.clientTag || "ui") + "-select-" + Date.now().toString(36) + "-" + root.choiceSeq
            root.retrySignature = signature
        }
        var chosen = root.highlightedId
        root.notice = ""
        root.host.request("pet.select", params, function (resp) {
            if (resp && resp.ok) {
                root.retryKey = ""
                root.notice = LifeText.cleanName(root.life ? root.life.name : "") + " now looks like a "
                    + LifeText.displayName(chosen, root.registry).toLowerCase() + "."
                if (!root.host.busy)
                    root.host.request("state.get", {}, function () {})
            } else {
                var e = resp && resp.error ? resp.error : {}
                root.notice = "The look did not change: " + String(e.message || e.code || "unknown error")
            }
        }, root.retryKey)
    }

    Keys.onPressed: event => {
        if (root.handleKey(event.key))
            event.accepted = true
    }

    ColumnLayout {
        id: layout
        width: parent.width
        spacing: 8

        Label {
            Layout.fillWidth: true
            text: root.mode === "pick" ? "Choose a companion. Every look is available from the start."
                                       : root.cosmeticText
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
        }

        Loader {
            id: gridLoader
            objectName: "chooser.gridLoader"
            Layout.fillWidth: true
            active: root.visible
            sourceComponent: GridLayout {
                id: grid
                objectName: "chooser.grid"
                columns: 5
                rowSpacing: 6
                columnSpacing: 6
                Repeater {
                    model: root.ids
                    delegate: Rectangle {
                        id: cell
                        required property string modelData
                        required property int index
                        readonly property bool current: root.highlightedId === modelData
                        objectName: "chooser.cell." + modelData
                        Layout.fillWidth: true
                        Layout.preferredHeight: 104
                        radius: 6
                        color: current ? "#dfeee6" : "transparent"
                        border.width: current ? 2 : 1
                        border.color: current ? "#2f6b4f" : "#b9c3c7"
                        Accessible.role: Accessible.Button
                        Accessible.name: LifeText.lookLabel(modelData, root.registry)
                            + (root.mode === "look" && modelData === root.currentLook ? " (current look)" : "")
                        Accessible.onPressAction: root.highlight(index)
                        Image {
                            id: thumb
                            anchors.horizontalCenter: parent.horizontalCenter
                            y: 4
                            width: 72
                            height: 72
                            source: root.thumbUrl(cell.modelData, root.stage)
                            sourceSize.width: 96
                            sourceSize.height: 96
                            asynchronous: true
                            cache: true
                            fillMode: Image.PreserveAspectFit
                        }
                        Label {
                            anchors.centerIn: thumb
                            visible: thumb.status !== Image.Ready
                            text: LifeText.displayName(cell.modelData, root.registry).charAt(0)
                            font.pixelSize: 28
                            textFormat: Text.PlainText
                        }
                        Label {
                            anchors.horizontalCenter: parent.horizontalCenter
                            anchors.bottom: parent.bottom
                            anchors.bottomMargin: 4
                            width: parent.width - 4
                            horizontalAlignment: Text.AlignHCenter
                            elide: Text.ElideRight
                            text: LifeText.displayName(cell.modelData, root.registry)
                                + (root.mode === "look" && cell.modelData === root.currentLook ? " ✓" : "")
                            textFormat: Text.PlainText
                        }
                        MouseArea {
                            anchors.fill: parent
                            onClicked: {
                                root.forceActiveFocus()
                                root.highlight(cell.index)
                            }
                        }
                    }
                }
            }
        }

        // Look mode: the highlighted species at all three stages (static thumbnails).
        RowLayout {
            objectName: "chooser.stagePreview"
            Layout.fillWidth: true
            visible: root.mode === "look" && root.highlightedId !== ""
            spacing: 12
            Repeater {
                model: root.mode === "look" && root.visible ? [1, 2, 3] : []
                delegate: ColumnLayout {
                    id: stageCell
                    required property int modelData
                    spacing: 2
                    Accessible.role: Accessible.StaticText
                    Accessible.name: LifeText.stageName(root.highlightedId, modelData, root.registry)
                    Image {
                        Layout.preferredWidth: 56
                        Layout.preferredHeight: 56
                        source: root.thumbUrl(root.highlightedId, stageCell.modelData)
                        sourceSize.width: 96
                        sourceSize.height: 96
                        asynchronous: true
                        fillMode: Image.PreserveAspectFit
                    }
                    Label {
                        text: LifeText.stageName(root.highlightedId, stageCell.modelData, root.registry)
                            + (stageCell.modelData === root.stage ? " (now)" : "")
                        textFormat: Text.PlainText
                    }
                }
            }
        }

        Label {
            objectName: "chooser.highlightText"
            Layout.fillWidth: true
            text: root.highlightedId !== "" ? LifeText.lookLabel(root.highlightedId, root.registry) : ""
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            font.bold: true
        }

        Label {
            objectName: "chooser.lockReason"
            Layout.fillWidth: true
            visible: root.mode === "look" && root.lockReason !== ""
            text: root.lockReason
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
        }

        RowLayout {
            visible: root.mode === "look"
            Button {
                objectName: "chooser.use"
                text: "Use this look"
                enabled: root.canSelect
                Accessible.name: "Use the " + LifeText.displayName(root.highlightedId, root.registry) + " look"
                Accessible.description: root.lockReason !== "" ? root.lockReason : root.cosmeticText
                onClicked: root.useLook()
            }
            Button {
                text: "Close"
                onClicked: root.closed()
            }
        }

        Label {
            objectName: "chooser.notice"
            Layout.fillWidth: true
            visible: root.notice !== ""
            text: root.notice
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            Accessible.name: text
        }
    }
}
