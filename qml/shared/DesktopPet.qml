pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

// Portable presentation only. The host/Rust snapshot owns all care/task state.
// This component never starts tasks or refreshes from a timer.
Item {
    id: root
    required property var host
    clip: true
    Keys.onEscapePressed: selected = false
    property bool surfaceActive: true
    property bool reducedMotion: false
    property bool selected: false
    property bool initialized: false
    property real petX: 40
    property int frame: 0
    property bool facingLeft: false
    property real destination: 40
    property int idleTicks: 60
    property bool walking: false
    readonly property bool active: visible && surfaceActive && width > 0 && height > 0
    readonly property real petSize: Math.min(110, width, height)
    readonly property real floorY: Math.max(0, height - petSize - 12)
    readonly property bool hovered: petMouse.containsMouse
    readonly property bool sleeping: controller.sleeping
    readonly property string pose: sleeping ? "sleep" : controller.reaction !== "" && controller.reaction !== "wake"
        ? controller.reaction : walking ? "walk" : "idle"
    readonly property bool motionRunning: active && !reducedMotion && !sleeping
    readonly property rect petHit: active ? Qt.rect(pet.x, pet.y, pet.width, pet.height) : Qt.rect(0, 0, 0, 0)
    readonly property rect menuHit: active && selected ? Qt.rect(menu.x, menu.y, menu.width, menu.height) : Qt.rect(0, 0, 0, 0)
    readonly property rect propsHit: active && prop.visible ? Qt.rect(prop.x, prop.y, prop.width, prop.height) : Qt.rect(0, 0, 0, 0)
    signal reducedMotionRequested(bool enabled)
    signal taskPanelRequested()
    signal hideRequested()

    Component.onCompleted: { initialized = true; clampPosition() }
    component SceneButton: Button {
        id: control
        background: Rectangle { radius: 8; color: control.down ? "#425271" : control.hovered ? "#34425c" : "#29364b"; opacity: control.enabled ? 1 : 0.4 }
        contentItem: Text { text: control.text; color: control.enabled ? "#edf2fa" : "#8090a4"; font.pixelSize: 11; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter }
    }
    onWidthChanged: if (initialized) clampPosition()
    onHeightChanged: if (initialized) clampPosition()
    onActiveChanged: if (!active) { selected = false; walking = false }
    onReducedMotionChanged: if (reducedMotion) walking = false
    onSleepingChanged: if (sleeping) walking = false
    onSelectedChanged: if (selected) walking = false
    function clampPosition() {
        if (width <= 0 || height <= 0) return
        petX = Math.max(0, Math.min(petX, Math.max(0, width - petSize)))
        destination = Math.max(0, Math.min(destination, Math.max(0, width - petSize)))
    }
    function requestCare(action) {
        if (!active || !selected) return
        walking = false
        idleTicks = 60
        if (action === "sleep" || action === "wake") {
            if (controller.canSleep)
                controller.perform(action === "wake" ? "pet.wake" : "pet.sleep", {}, action)
        } else controller.careClicked(action)
    }
    function cosmeticTick() {
        if (!motionRunning) return
        frame = (frame + 1) % 10
        if (selected || hovered || controller.reaction !== "") { walking = false; return }
        if (idleTicks > 0) { idleTicks -= 1; return }
        if (!walking) {
            destination = Math.max(0, Math.min(width - petSize, petX + (Math.random() < 0.5 ? -1 : 1) * (70 + Math.random() * 150)))
            facingLeft = destination < petX
            walking = Math.abs(destination - petX) > 2
            if (!walking) idleTicks = 40
        }
        if (walking) {
            var distance = destination - petX
            petX += Math.max(-5, Math.min(5, distance))
            clampPosition()
            if (Math.abs(destination - petX) < 1) {
                walking = false
                idleTicks = 40 + Math.floor(Math.random() * 71) // 8–22 seconds of stillness
            }
        }
    }
    Timer {
        objectName: "desktopPet.cosmeticTimer"
        interval: 200; repeat: true; running: root.motionRunning
        onTriggered: root.cosmeticTick()
    }
    PetCare {
        id: controller
        objectName: "desktopPet.careController"
        host: root.host
        visible: false
        enabled: false
        reducedMotion: true
        quietMode: true
    }
    Item {
        id: pet
        objectName: "desktopPet.pet"
        x: root.petX; y: root.floorY
        width: root.petSize; height: root.petSize
        visible: root.active
        activeFocusOnTab: root.active
        Keys.onReturnPressed: root.selected = !root.selected
        Keys.onSpacePressed: root.selected = !root.selected
        PetSprite {
            anchors.fill: parent
            pose: root.pose
            frame: root.frame
            facingLeft: root.facingLeft
        }
        MouseArea {
            id: petMouse
            anchors.fill: parent
            hoverEnabled: true
            enabled: root.active
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            cursorShape: Qt.PointingHandCursor
            onClicked: root.selected = !root.selected
        }
        Accessible.role: Accessible.Button
        Accessible.name: "CodePet. Open care menu"
        Accessible.onPressAction: if (root.active) root.selected = !root.selected
    }
    // Hover bars are informational and deliberately outside the input region.
    Rectangle {
        id: bars
        x: Math.max(0, Math.min(root.width - width, pet.x + pet.width / 2 - width / 2))
        y: Math.max(0, pet.y - height - 4)
        width: Math.min(136, root.width); height: 31; radius: 10
        visible: root.active && (root.hovered || root.selected) && !root.selected
        color: "#ed171c29"; border.color: "#354157"
        Row {
            anchors.centerIn: parent; spacing: 5
            Repeater {
                model: ["hunger", "thirst", "mental_health", "hygiene"]
                Rectangle {
                    required property string modelData
                    width: 26; height: 5; radius: 2; color: "#364052"
                    Rectangle {
                        width: parent.width * Math.max(0, Math.min(1, Number(controller.meters[parent.modelData] || 0) / 10000))
                        height: parent.height; radius: 2; color: "#99d4ac"
                    }
                }
            }
        }
    }
    Item {
        id: prop
        objectName: "desktopPet.prop"
        readonly property string action: root.sleeping ? "sleep" : controller.reaction
        readonly property string asset: action === "sleep" ? "prop-bed.svg" : action === "food" ? "prop-food.svg"
            : action === "bathroom" ? "prop-tree.svg" : "prop-water.svg"
        x: Math.max(0, Math.min(root.width - width, pet.x + pet.width - 12))
        y: Math.max(0, pet.y + pet.height - height)
        width: Math.min(76, root.width); height: Math.min(64, root.height)
        visible: root.active && ["sleep", "food", "water", "wash", "bathroom"].indexOf(action) >= 0
        Image {
            anchors.fill: parent; source: "../assets/" + prop.asset
            fillMode: Image.PreserveAspectFit
            sourceSize.width: 152; sourceSize.height: 128
        }
        // Props are decorative and have no handler, so hosts keep them out of the input
        // region (click-through). propsHit is reported for layout/tests only.
    }
    Rectangle {
        id: menu
        objectName: "desktopPet.menu"
        x: Math.max(0, Math.min(root.width - width, pet.x + pet.width / 2 - width / 2))
        y: Math.max(0, pet.y - height - 12)
        width: Math.min(260, root.width)
        height: Math.min(content.implicitHeight + 28, root.height)
        radius: 18; color: "#f519202e"; border.color: "#45516a"; border.width: 1
        visible: root.active && root.selected
        enabled: visible
        clip: true
        // Swallow empty menu-space clicks; nothing leaks to the underlying desktop.
        MouseArea { anchors.fill: parent }
        Flickable {
            anchors.fill: parent; anchors.margins: 14
            contentHeight: content.implicitHeight; clip: true
            boundsBehavior: Flickable.StopAtBounds
            Column {
                id: content
                width: parent.width; spacing: 9
                Row {
                    width: parent.width
                    Text { width: parent.width - 36; text: "CodePet"; color: "#f0f4fa"; font.pixelSize: 17; font.bold: true }
                    SceneButton {
                        width: 32; height: 26; text: "×"
                        Accessible.name: "Close care menu"
                        onClicked: root.selected = false
                    }
                }
                Text {
                    width: parent.width; text: root.sleeping ? "RESTING · no work starts automatically" : controller.readinessText
                    color: "#a8b9cf"; font.pixelSize: 11; wrapMode: Text.Wrap; textFormat: Text.PlainText
                }
                Repeater {
                    model: [ { key: "hunger", label: "Nourishment", tint: "#e6bd8a" }, { key: "thirst", label: "Hydration", tint: "#8ecae5" }, { key: "mental_health", label: "Mood", tint: "#c3abe8" }, { key: "hygiene", label: "Hygiene", tint: "#97d4b0" } ]
                    Column {
                        id: meterRow
                        required property var modelData
                        width: content.width; spacing: 4
                        property real value: Math.max(0, Math.min(100, Number(controller.meters[modelData.key] || 0) / 100))
                        Row {
                            width: parent.width
                            Text { width: parent.width - 40; text: meterRow.modelData.label; color: "#c4d0e1"; font.pixelSize: 11 }
                            Text { width: 40; horizontalAlignment: Text.AlignRight; text: Math.round(meterRow.value) + "%"; color: "#c4d0e1"; font.pixelSize: 11 }
                        }
                        Rectangle {
                            width: parent.width; height: 4; radius: 2; color: "#354052"
                            Rectangle { width: parent.width * meterRow.value / 100; height: 4; radius: 2; color: meterRow.modelData.tint }
                        }
                    }
                }
                Grid {
                    columns: 2; spacing: 6
                    Repeater {
                        model: [{action:"food",label:"Food"},{action:"water",label:"Water"},{action:"play",label:"Pet / play"},{action:"wash",label:"Wash"},{action:"bathroom",label:"Bathroom"},{action:"sleep",label:"Bed · 2 min"}]
                        SceneButton {
                            id: careButton
                            required property var modelData
                            width: (content.width - 6) / 2; height: 31
                            text: modelData.action === "sleep" && root.sleeping ? "Wake / settle" : modelData.label
                            enabled: menu.visible && (modelData.action === "sleep" ? controller.canSleep : controller.canCare)
                            Accessible.name: text
                            onClicked: root.requestCare(modelData.action === "sleep" && root.sleeping ? "wake" : modelData.action)
                            background: Rectangle { radius: 8; color: careButton.down ? "#425271" : careButton.hovered ? "#34425c" : "#29364b"; opacity: careButton.enabled ? 1 : 0.4 }
                            contentItem: Text { text: careButton.text; color: careButton.enabled ? "#edf2fa" : "#8090a4"; font.pixelSize: 11; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter }
                        }
                    }
                }
                Text {
                    width: parent.width; text: controller.feedback; color: "#a8b9cf"; font.pixelSize: 10; wrapMode: Text.Wrap; textFormat: Text.PlainText
                }
                SceneButton {
                    width: parent.width; height: 33; text: "Open task panel · simulation"
                    enabled: menu.visible
                    onClicked: { root.selected = false; root.taskPanelRequested() }
                }
                Row {
                    width: parent.width; spacing: 4
                    CheckBox {
                        width: parent.width - 50; text: "Less motion"; checked: root.reducedMotion
                        onToggled: root.reducedMotionRequested(checked)
                        palette.windowText: "#bac8dc"
                    }
                    SceneButton {
                        width: 46; height: 32; text: "Hide"
                        onClicked: { root.selected = false; root.hideRequested() }
                    }
                }
            }
        }
    }
}
