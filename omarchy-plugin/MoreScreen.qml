pragma ComponentBehavior: Bound
import QtQuick

// More, the grown-up corner (SPEC-v0.3 §3.6): advanced jobs and preferences. Pet on my desktop, Bedtime
// (only while awake), Calm mode, How it works, Memories and Say goodbye.
Item {
    id: more
    required property var app
    readonly property var t: app.t
    readonly property var life: app.life
    readonly property bool awake: !!(life && app.face && ["asleep", "paused", "fainted", "off"].indexOf(app.face.state) < 0)

    implicitWidth: t.u(448)
    implicitHeight: column.implicitHeight

    // On shows the desktop pet; off needs no verb: the pet reads the switch and exits by
    // itself (notes.md §9 item 2).
    function setCalm(on) { more.app.ctl.setSetting("calm", on) }
    function setPinned(on) {
        var next = Object.assign({}, more.app.cfg.desktopPlacement || {})
        next.pinned = on
        more.app.ctl.setSetting("desktopPlacement", next)
    }
    function bedtime() {
        more.app.request("pet.sleep", {}, function (resp) {
            if (resp.ok) more.app.home()
            else more.app.failed(resp)
        })
    }

    Column {
        id: column
        width: parent.width
        spacing: more.t.u(6)
        MenuRow {
            objectName: "cp.more.desktop"
            t: more.t
            width: parent.width
            height: more.t.u(52)
            icon: "status/screen"
            text: more.app.s("more.desktop")
            kind: "switch"
            checked: more.app.cfg.overlayOnLoad === true
            textOn: more.app.s("header.on")
            textOff: more.app.s("header.off")
            onToggled: c => more.app.setDesktop(c)
        }
        MenuRow {
            objectName: "cp.more.pin"
            t: more.t
            width: parent.width
            height: more.t.u(52)
            icon: "care-floor/pin"
            text: more.app.s("more.pin")
            kind: "switch"
            checked: !!(more.app.cfg.desktopPlacement && more.app.cfg.desktopPlacement.pinned)
            textOn: more.app.s("header.on")
            textOff: more.app.s("header.off")
            onToggled: c => more.setPinned(c)
        }
        MenuRow {
            objectName: "cp.more.bedtime"
            visible: more.awake
            t: more.t
            width: parent.width
            height: more.t.u(52)
            icon: "status/moon"
            text: more.app.s("more.bedtime")
            kind: "action"
            actionIcon: "status/moon"
            actionText: more.app.s("btn.sleep")
            onTapped: more.bedtime()
        }
        MenuRow {
            objectName: "cp.more.calm"
            t: more.t
            width: parent.width
            height: more.t.u(52)
            icon: "status/leaf_calm"
            text: more.app.s("more.calm")
            kind: "switch"
            checked: more.app.calm
            textOn: more.app.s("header.on")
            textOff: more.app.s("header.off")
            onToggled: c => more.setCalm(c)
        }
        MenuRow {
            objectName: "cp.more.jobs"
            t: more.t
            width: parent.width
            height: more.t.u(52)
            icon: "status/laptop"
            text: more.app.s("more.jobs")
            onTapped: more.app.openWork("build")
        }
        MenuRow {
            objectName: "cp.more.updates"
            t: more.t
            width: parent.width
            height: more.t.u(52)
            icon: "care-floor/star"
            text: more.app.s(more.app.ctl && more.app.ctl.updateState && more.app.ctl.updateState.state === "available" ? "updates.ready_label" : "updates.title")
            onTapped: more.app.go("updates", {})
        }
        MenuRow {
            objectName: "cp.more.help"
            t: more.t
            width: parent.width
            height: more.t.u(52)
            icon: "status/help"
            text: more.app.s("more.help")
            onTapped: more.app.go("help", {})
        }
        MenuRow {
            objectName: "cp.more.memories"
            t: more.t
            width: parent.width
            height: more.t.u(52)
            icon: "status/memories"
            text: more.app.s("more.memories")
            onTapped: more.app.go("memories", {})
        }
        MenuRow {
            objectName: "cp.more.goodbye"
            visible: more.life !== null
            t: more.t
            width: parent.width
            height: more.t.u(52)
            icon: "status/wave_goodbye"
            text: more.app.s("more.goodbye")
            onTapped: more.app.go("goodbye", {})
        }
    }
}
