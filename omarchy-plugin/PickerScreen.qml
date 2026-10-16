pragma ComponentBehavior: Bound
import QtQuick
import "CodePetBar.js" as Model

// The pets picker (SPEC-v0.3 §3.3): first run (cat preselected, Let's go!), welcome back
// (the life's look preselected, name kept), the Off variant of first run (Turn on instead
// of Let's go!) and Change ("Turn {name} into…", stage row, stars, This one!).
// One strip of 15 animated portraits is the only sideways scroller in the UI.
Item {
    id: picker
    required property var app
    property string mode: "first_run"
    readonly property var t: app.t
    readonly property var life: app.life
    readonly property bool isChange: mode === "change"
    readonly property bool isFresh: mode === "first_run" || mode === "first_run_off"
    readonly property var ids: Model.speciesIds(app.reg)

    property string selected: "cat"
    // A name the kid typed; until then a fresh adoption follows the species' defaultName.
    property string typedName: ""
    // A fresh adoption preselects the cat, welcome back and Change the life's current look
    // (SPEC-v0.3 §3.3); a new life or another mode starts over.
    readonly property string resetKey: picker.mode + "|" + (picker.life ? picker.life.life_id : "")
    onResetKeyChanged: picker.reset()
    Component.onCompleted: picker.reset()
    function reset() {
        picker.selected = picker.isFresh || !picker.life ? "cat" : String(picker.life.appearance || "cat")
        picker.typedName = ""
        var i = picker.ids.indexOf(picker.selected)
        if (i >= 0) Qt.callLater(function () { strip.positionViewAtIndex(i, ListView.Contain) })
    }
    readonly property string shownName: picker.typedName !== "" ? picker.typedName
        : picker.isFresh ? (Model.defaultName(app.reg, picker.selected) || app.s("name.type"))
        : (picker.life ? picker.life.name : "")
    readonly property string set: Model.careSet(app.reg, picker.selected)

    implicitWidth: t.u(448)
    implicitHeight: column.implicitHeight

    function pick(id) { picker.selected = id }
    function stageThumb(stage: int): string {
        var rel = Model.thumbPath(picker.selected, stage)
        return picker.app.runtimeBase !== "" && rel !== "" ? picker.app.runtimeBase + rel : ""
    }
    function scrollBy(n) {
        var step = picker.t.u(88)
        strip.contentX = Math.max(0, Math.min(strip.contentWidth - strip.width, strip.contentX + n * step))
    }
    function editName() {
        picker.app.editName(picker.typedName !== "" ? picker.typedName : (picker.isFresh ? Model.defaultName(picker.app.reg, picker.selected) : picker.shownName),
            function (name) { picker.typedName = name })
    }
    function go() {
        if (picker.mode === "first_run_off") { picker.app.setPower(true); return }
        if (picker.isChange) {
            if (!picker.life || picker.selected === picker.life.appearance) { picker.app.back(); return }
            picker.app.request("pet.select", { life_id: picker.life.life_id, pet_id: picker.selected }, function (resp) {
                if (resp.ok) picker.app.back()
                else picker.app.failed(resp)
            })
            return
        }
        picker.app.start(picker.selected, picker.typedName)
    }

    Column {
        id: column
        width: parent.width
        spacing: 0

        // ---- strip: 96 px
        Item {
            width: parent.width
            height: picker.t.u(96)
            IconButton {
                id: left
                objectName: "cp.strip.left"
                t: picker.t
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                glyph: "‹"
                accessibleName: "‹"
                onTapped: picker.scrollBy(-3)
            }
            ListView {
                id: strip
                objectName: "cp.strip"
                anchors.left: left.right
                anchors.right: right.left
                anchors.leftMargin: picker.t.u(6)
                anchors.rightMargin: picker.t.u(6)
                anchors.verticalCenter: parent.verticalCenter
                height: picker.t.u(88)
                orientation: ListView.Horizontal
                spacing: picker.t.u(8)
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                model: picker.ids
                WheelHandler {
                    onWheel: event => {
                        var d = event.angleDelta.y !== 0 ? event.angleDelta.y : event.angleDelta.x
                        picker.scrollBy(d > 0 ? -1 : 1)
                    }
                }
                delegate: Rectangle {
                    id: portrait
                    required property string modelData
                    required property int index
                    readonly property bool cpControl: true
                    readonly property string cpKind: "portrait"
                    readonly property bool chosen: picker.selected === modelData
                    objectName: "cp.portrait." + modelData
                    width: picker.t.u(80)
                    height: picker.t.u(80)
                    y: (strip.height - height) / 2
                    radius: picker.t.radius
                    color: chosen ? picker.t.glowFill : portraitMouse.containsMouse ? picker.t.tileHover : picker.t.tileFill
                    border.width: chosen || activeFocus ? Math.max(2, picker.t.u(4)) : 1
                    border.color: chosen || activeFocus ? picker.t.accent : picker.t.tileBorder
                    activeFocusOnTab: true
                    Accessible.role: Accessible.Button
                    Accessible.name: Model.speciesWord(picker.app.reg, modelData)
                    Accessible.onPressAction: picker.pick(modelData)
                    Keys.onReturnPressed: picker.pick(modelData)
                    Keys.onSpacePressed: picker.pick(modelData)
                    PetPlayer {
                        anchors.centerIn: parent
                        width: picker.t.u(68)
                        height: width
                        registry: picker.app.reg
                        species: portrait.modelData
                        stage: 1
                        petState: "idle"
                        clockMs: picker.app.clockMs + portrait.index * 170
                        calm: picker.app.calm
                        runtimeBase: picker.app.runtimeBase
                        dim: picker.mode === "first_run_off"
                    }
                    MouseArea {
                        id: portraitMouse
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: picker.pick(portrait.modelData)
                    }
                }
            }
            IconButton {
                id: right
                objectName: "cp.strip.right"
                t: picker.t
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                glyph: "›"
                accessibleName: "›"
                onTapped: picker.scrollBy(3)
            }
        }
        Item { width: 1; height: picker.t.u(8) }

        // ---- big preview: 160 px
        PetPlayer {
            objectName: "cp.preview"
            anchors.horizontalCenter: parent.horizontalCenter
            width: picker.t.u(160)
            height: width
            registry: picker.app.reg
            species: picker.selected
            stage: picker.isChange ? Model.stageOf(picker.life) : (picker.life && !picker.isFresh ? Model.stageOf(picker.life) : 1)
            petState: picker.isChange || picker.mode === "first_run_off" ? "idle" : "idle"
            clockMs: picker.app.clockMs
            calm: picker.app.calm
            runtimeBase: picker.app.runtimeBase
            dim: picker.mode === "first_run_off"
        }
        Item { width: 1; height: picker.t.u(8) }

        // ---- first run / welcome back: name button 56 + the two rule lines
        Rectangle {
            id: nameButton
            objectName: "cp.pickerName"
            readonly property bool cpControl: true
            readonly property string cpKind: "control"
            visible: !picker.isChange
            anchors.horizontalCenter: parent.horizontalCenter
            width: Math.min(parent.width, Math.max(picker.t.u(200), nameRow.implicitWidth + picker.t.u(40)))
            height: picker.t.u(56)
            radius: picker.t.radius
            color: nameMouse.pressed ? picker.t.tilePress : nameMouse.containsMouse ? picker.t.tileHover : picker.t.tileFill
            border.width: activeFocus ? Math.max(2, picker.t.u(3)) : 1
            border.color: activeFocus ? picker.t.focusRing : picker.t.tileBorder
            activeFocusOnTab: true
            Accessible.role: Accessible.Button
            Accessible.name: picker.shownName
            Accessible.onPressAction: picker.editName()
            Keys.onReturnPressed: picker.editName()
            Keys.onSpacePressed: picker.editName()
            Row {
                id: nameRow
                anchors.centerIn: parent
                spacing: picker.t.u(10)
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: picker.shownName
                    textFormat: Text.PlainText
                    font.family: picker.t.family
                    font.pixelSize: picker.t.fontHeading
                    font.bold: true
                    color: picker.t.text
                    elide: Text.ElideRight
                    width: Math.min(implicitWidth, picker.t.u(330))
                }
                Icon {
                    anchors.verticalCenter: parent.verticalCenter
                    icon: "ui/pencil"
                    size: picker.t.u(28)
                }
            }
            MouseArea {
                id: nameMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: picker.editName()
            }
        }
        Item { width: 1; height: picker.t.u(8); visible: !picker.isChange }
        Item {
            objectName: "cp.rules"
            visible: !picker.isChange
            width: parent.width
            height: picker.t.u(80)
            Column {
                anchors.left: parent.left
                anchors.right: helpButton.left
                anchors.rightMargin: picker.t.u(8)
                anchors.verticalCenter: parent.verticalCenter
                Repeater {
                    model: [{ icon: Model.careIcon(picker.app.reg, picker.selected, "food"), key: "rules.1" },
                            { icon: "status/bandage", key: "rules.2" }]
                    Item {
                        id: rule
                        required property var modelData
                        width: parent.width
                        height: picker.t.u(40)
                        Icon {
                            id: ruleIcon
                            anchors.verticalCenter: parent.verticalCenter
                            icon: rule.modelData.icon
                            size: picker.t.u(34)
                        }
                        Text {
                            anchors.left: ruleIcon.right
                            anchors.leftMargin: picker.t.u(10)
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            text: picker.app.sFor(rule.modelData.key, picker.set, {})
                            textFormat: Text.PlainText
                            font.family: picker.t.family
                            font.pixelSize: picker.t.fontTitle
                            font.bold: true
                            color: picker.t.text
                            elide: Text.ElideRight
                        }
                    }
                }
            }
            IconButton {
                id: helpButton
                objectName: "cp.rulesHelp"
                t: picker.t
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                icon: "status/help"
                accessibleName: picker.app.s("help.title")
                onTapped: picker.app.go("help", {})
            }
        }

        // ---- Change: the three stages (real art + word) and the stars
        Item {
            objectName: "cp.stages"
            visible: picker.isChange
            width: parent.width
            height: picker.t.u(68)
            Row {
                anchors.centerIn: parent
                spacing: picker.t.u(14)
                Repeater {
                    model: [1, 2, 3]
                    Row {
                        id: stageCell
                        required property int modelData
                        spacing: picker.t.u(14)
                        Text {
                            visible: stageCell.modelData > 1
                            anchors.verticalCenter: parent.verticalCenter
                            text: "→"
                            textFormat: Text.PlainText
                            font.family: picker.t.family
                            font.pixelSize: picker.t.fontHeading
                            color: picker.t.dim
                        }
                        Column {
                            id: stageColumn
                            readonly property bool current: Model.stageOf(picker.life) === stageCell.modelData
                            opacity: current ? 1 : 0.4
                            Image {
                                anchors.horizontalCenter: parent.horizontalCenter
                                width: picker.t.u(44)
                                height: width
                                source: picker.stageThumb(stageCell.modelData)
                                sourceSize.width: 96
                                sourceSize.height: 96
                                fillMode: Image.PreserveAspectFit
                                smooth: true
                                asynchronous: true
                            }
                            Text {
                                anchors.horizontalCenter: parent.horizontalCenter
                                text: picker.app.s("stage." + stageCell.modelData)
                                textFormat: Text.PlainText
                                font.family: picker.t.family
                                font.pixelSize: picker.t.fontBody
                                font.bold: stageColumn.current
                                color: picker.t.text
                            }
                        }
                    }
                }
            }
        }
        Row {
            objectName: "cp.stars"
            visible: picker.isChange
            anchors.horizontalCenter: parent.horizontalCenter
            height: picker.t.u(28)
            spacing: picker.t.u(6)
            Repeater {
                model: 5
                Icon {
                    required property int index
                    icon: "care-floor/star"
                    size: picker.t.u(26)
                    opacity: index < Model.stars(picker.life) ? 1 : 0.22
                }
            }
        }
        Item { width: 1; height: picker.t.u(8) }

        // ---- sticky footer: 72 px
        BigButton {
            objectName: "cp.go"
            t: picker.t
            width: parent.width
            height: picker.t.u(72)
            icon: picker.mode === "first_run_off" ? "care-floor/power" : ""
            text: picker.mode === "first_run_off" ? picker.app.s("btn.turn_on")
                : picker.isChange ? picker.app.s("btn.this") : picker.app.s("btn.go")
            onTapped: picker.go()
        }
    }
}
