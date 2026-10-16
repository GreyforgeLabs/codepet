pragma ComponentBehavior: Bound
import QtQuick

// The bar cell's right-click menu (SPEC-v0.3 §3.1): CodePet on/off, Pet on my desktop,
// Helpers and More. Helpers and More open the popup on that page.
Item {
    id: menu
    required property var app
    readonly property var t: app.t
    readonly property bool hasPet: !!(app.life && app.life.armed === true)

    implicitWidth: t.u(360)
    implicitHeight: column.implicitHeight

    Column {
        id: column
        width: parent.width
        spacing: menu.t.u(6)
        MenuRow {
            objectName: "cp.menu.power"
            t: menu.t
            width: parent.width
            icon: "care-floor/power"
            text: menu.app.s("menu.power")
            kind: "switch"
            checked: menu.app.isOn
            textOn: menu.app.s("header.on")
            textOff: menu.app.s("header.off")
            onToggled: c => menu.app.setPower(c)
        }
        MenuRow {
            objectName: "cp.menu.desktop"
            t: menu.t
            width: parent.width
            icon: "status/screen"
            text: menu.app.s("menu.desktop")
            kind: "switch"
            checked: menu.app.cfg.overlayOnLoad === true
            textOn: menu.app.s("header.on")
            textOff: menu.app.s("header.off")
            onToggled: c => {
                menu.app.ctl.setSetting("overlayOnLoad", c)
                if (c) menu.app.ctl.launch("show")
            }
        }
        MenuRow {
            objectName: "cp.menu.helpers"
            visible: menu.hasPet && menu.app.isOn
            t: menu.t
            width: parent.width
            icon: "status/laptop"
            text: menu.app.s("menu.helpers")
            onTapped: menu.app.ctl.openPage("helpers")
        }
        MenuRow {
            objectName: "cp.menu.more"
            visible: menu.hasPet && menu.app.isOn
            t: menu.t
            width: parent.width
            icon: "care-floor/gear_more"
            text: menu.app.s("menu.more")
            onTapped: menu.app.ctl.openPage("more")
        }
    }
}
