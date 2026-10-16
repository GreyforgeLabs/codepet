pragma ComponentBehavior: Bound
import QtQuick
import "CodePetBar.js" as Model
import "Strings.js" as Strings

// Everything inside the CodePet popup card (SPEC-v0.3 §3): the shared header with the power
// switch, and one screen at a time, chosen by the export (agent_vectors.json popup_routing)
// or by the kid's taps (Change, Helpers, More, Help, ...). Sub-pages swap in place.
//
// It imports only QtQuick and qs.Commons (through its children), so the offscreen smokes in
// qml/tests run it under PySide with a fake controller. `ctl` is the bar widget in the
// shell; it provides:
//   barState (normalized export or null), snapshot (last state.get result), registry,
//   face (petstate.js sharedPriority), cfg (plugin settings), runtimeBase (file:// URL of
//   the installed runtime, for pet art), popupVisible, testState, now (ms),
//   request(method, params, callback), setSetting(key, value), launch(verb), closePopup().
//
// Timers: one animation clock (only while open, On and not in Calm mode) and one one-shot
// that hides a tap's word bubble. Neither sends a request.
Item {
    id: popupRoot
    required property var ctl
    property alias t: theme
    // "popup" (the card) or "menu" (the right-click menu).
    property string mode: "popup"
    // A page to open on (e.g. "helpers" or "more" from the right-click menu).
    property string startPage: ""

    PopupTheme { id: theme }

    readonly property var doc: ctl ? ctl.barState : null
    readonly property var life: doc && doc.life ? doc.life : null
    readonly property var face: ctl ? ctl.face : null
    readonly property var reg: ctl ? ctl.registry : null
    readonly property var cfg: ctl && ctl.cfg ? ctl.cfg : ({})
    readonly property bool calm: cfg.calm === true
    readonly property bool isOn: doc ? doc.enabled !== false : true
    readonly property string careSet: Model.careSet(reg, life ? life.appearance : "")
    readonly property string runtimeBase: ctl ? String(ctl.runtimeBase || "") : ""
    readonly property real now: ctl && ctl.now ? ctl.now : Date.now()

    // ---------------------------------------------------------------- copy

    function s(key, vars) {
        var v = {}
        if (life) v.name = life.name
        if (vars) for (var k in vars) v[k] = vars[k]
        return Model.t(Strings.STRINGS, key, popupRoot.careSet, v)
    }
    function sFor(key, set, vars) { return Model.t(Strings.STRINGS, key, set, vars || {}) }
    function appName(id) { return Model.appName(id) }
    function faceLine() { return Model.faceLine(Strings.STRINGS, popupRoot.reg, popupRoot.doc, popupRoot.face) }

    // ---------------------------------------------------------------- navigation

    property string page: ""
    property var pageArgs: ({})
    property var history: []
    // Meet a new pet: memorial -> picker, for this popup session only (UI-local).
    property bool meetNew: false
    readonly property string route: Model.route(doc)
    readonly property string screen: {
        if (popupRoot.mode === "menu") return "menu"
        if (popupRoot.route === "loading") return "loading"
        // Off wins over any page: the Off look, or the picker with Turn on.
        if (popupRoot.route === "off_look" || popupRoot.route === "first_run_off") return popupRoot.page === "help" ? "help" : popupRoot.route
        if (popupRoot.page !== "") return popupRoot.page
        if (popupRoot.route === "memorial" && popupRoot.meetNew) return "first_run"
        return popupRoot.route
    }

    function go(p, args) {
        var h = popupRoot.history.slice()
        h.push({ page: popupRoot.page, args: popupRoot.pageArgs })
        popupRoot.history = h
        popupRoot.pageArgs = args || {}
        popupRoot.page = p
        popupRoot.editing = false
    }
    function back() {
        popupRoot.editing = false
        var h = popupRoot.history.slice()
        var prev = h.pop()
        popupRoot.history = h
        popupRoot.page = prev ? prev.page : ""
        popupRoot.pageArgs = prev ? prev.args : {}
    }
    function home() {
        popupRoot.history = []
        popupRoot.page = ""
        popupRoot.pageArgs = {}
        popupRoot.editing = false
    }
    Component.onCompleted: if (popupRoot.startPage !== "") popupRoot.go(popupRoot.startPage, {})
    // Off from anywhere (this switch, the bar menu or `codepet off`) shows the Off look; On
    // again routes afresh to Home, not to the page that was open (popup_routing transitions).
    onIsOnChanged: if (!popupRoot.isOn) popupRoot.home()

    // ---------------------------------------------------------------- requests

    property string notice: ""
    function request(method, params, callback) {
        if (!popupRoot.ctl) return false
        return popupRoot.ctl.request(method, params, function (resp) {
            if (callback) callback(resp || { ok: false, error: { code: "CLIENT_ERROR" } })
        })
    }
    function failed(resp) {
        var code = resp && resp.error ? String(resp.error.code || "") : ""
        if (code === "NAME_INVALID") popupRoot.say(popupRoot.s("err.name_too_long"), "")
        else popupRoot.say(popupRoot.s("err.generic"), "")
    }

    function setPower(on) {
        popupRoot.request("app.set_enabled", { enabled: on }, function (resp) {
            if (!resp.ok) { popupRoot.failed(resp); return }
            popupRoot.home()
            if (on && popupRoot.cfg.overlayOnLoad === true) popupRoot.ctl.launch("show")
        })
    }

    // Let's go!: pet.start (adopt, or arm the living unarmed life), the desktop pet comes
    // on, then the Connect card when an app was found (SPEC-v0.3 §2.1).
    function start(species, typedName) {
        var params = { species: species, mode: "gentle", acknowledge_rules: true }
        if (popupRoot.life) params.life_id = popupRoot.life.life_id
        var name = Model.cleanName(typedName)
        if (name !== "") params.name = name
        popupRoot.request("pet.start", params, function (resp) {
            if (!resp.ok && !(resp.error && resp.error.code === "ALREADY_ARMED")) { popupRoot.failed(resp); return }
            popupRoot.meetNew = false
            popupRoot.home()
            if (resp.ok) {
                popupRoot.ctl.setSetting("overlayOnLoad", true)
                popupRoot.ctl.launch("show")
            }
            popupRoot.request("clients.status", {}, function (st) {
                var offered = st.ok ? Model.connectOffered(st.result) : []
                if (offered.length > 0) popupRoot.go("connect", { offered: offered })
            })
        })
    }

    // ---------------------------------------------------------------- name editing

    property bool editing: false
    property string editValue: ""
    property string editError: ""
    property var editDone: null
    function editName(initial, done) {
        popupRoot.editValue = initial
        popupRoot.editError = ""
        popupRoot.editDone = done
        popupRoot.editing = true
    }
    function finishEdit(value) {
        var problem = Model.nameProblem(value)
        if (problem !== "") { popupRoot.editError = popupRoot.s(problem); return }
        var done = popupRoot.editDone
        popupRoot.editing = false
        if (done) done(Model.cleanName(value))
    }
    function renameLife() {
        if (!popupRoot.life) return
        var lifeId = popupRoot.life.life_id
        popupRoot.editName(popupRoot.life.name, function (name) {
            if (name === popupRoot.life.name) return
            popupRoot.request("pet.rename", { life_id: lifeId, name: name }, function (resp) {
                if (!resp.ok) popupRoot.failed(resp)
            })
        })
    }

    // ---------------------------------------------------------------- the clock and bubbles

    property real clockMs: 0
    readonly property bool animating: popupRoot.ctl !== null && popupRoot.ctl.popupVisible === true && popupRoot.isOn && !popupRoot.calm
        && popupRoot.screen !== "loading" && popupRoot.screen !== "menu"
    Timer {
        id: frames
        objectName: "codepetPopup.frames"
        interval: 100
        repeat: true
        running: popupRoot.animating
        onTriggered: popupRoot.clockMs += interval
    }

    // A tap's word bubble and the care animation it plays (K5: within 150 ms, always).
    property string bubbleText: ""
    property string actionState: ""
    property real actionStartMs: 0
    Timer {
        id: bubbleTimer
        objectName: "codepetPopup.bubble"
        interval: 2600
        repeat: false
        onTriggered: {
            popupRoot.bubbleText = ""
            popupRoot.actionState = ""
        }
    }
    function say(text, action) {
        popupRoot.bubbleText = text
        if (action !== undefined && action !== "") {
            popupRoot.actionState = action
            popupRoot.actionStartMs = popupRoot.clockMs
        }
        bubbleTimer.restart()
    }

    // ---------------------------------------------------------------- layout

    implicitWidth: theme.u(448)
    implicitHeight: column.implicitHeight

    readonly property var headerSpec: {
        var name = popupRoot.life ? popupRoot.life.name : ""
        switch (popupRoot.screen) {
        case "home": return { title: name, back: false, nameButton: !popupRoot.editing }
        case "off_look": return { title: name, back: false }
        case "first_run": case "first_run_off": return { title: popupRoot.s("picker.title"), back: false }
        case "welcome_back": return { title: popupRoot.s("picker.welcome"), back: false }
        case "memorial": return { title: popupRoot.s("memories.title"), back: false }
        case "change": return { title: popupRoot.s("change.title"), back: true }
        case "connect": return { title: "", back: false }
        case "helpers": return { title: popupRoot.s("helpers.title"), back: true }
        case "more": return { title: popupRoot.s("more.title"), back: true }
        case "help": return { title: popupRoot.s("help.title"), back: true }
        case "memories": return { title: popupRoot.s("memories.title"), back: true }
        case "goodbye": return { title: popupRoot.s("btn.goodbye"), back: true }
        default: return { title: "", back: false }
        }
    }

    Column {
        id: column
        width: parent.width
        spacing: 0

        PopupHeader {
            id: header
            objectName: "cp.header"
            width: parent.width
            visible: popupRoot.mode !== "menu"
            t: theme
            title: popupRoot.headerSpec.title
            showBack: popupRoot.headerSpec.back === true || popupRoot.editing
            nameButton: popupRoot.headerSpec.nameButton === true
            powerOn: popupRoot.isOn
            backText: popupRoot.s("btn.back")
            textOn: popupRoot.s("header.on")
            textOff: popupRoot.s("header.off")
            onBack: {
                if (popupRoot.editing) popupRoot.editing = false
                else if (!popupRoot.screenHandlesBack()) popupRoot.back()
            }
            onRename: popupRoot.renameLife()
            onPower: on => popupRoot.setPower(on)
        }
        Item { width: 1; height: popupRoot.mode === "menu" ? 0 : theme.u(8) }

        // The screen stays loaded under the name field, so a picker keeps its choice while
        // the kid types a name.
        Item {
            width: parent.width
            height: popupRoot.editing ? popupRoot.heightOf(editor.item) : body.height
            Loader {
                id: body
                objectName: "cp.body"
                width: parent.width
                height: popupRoot.heightOf(item)
                visible: !popupRoot.editing
                sourceComponent: popupRoot.componentFor(popupRoot.screen)
            }
            Loader {
                id: editor
                objectName: "cp.editor"
                width: parent.width
                active: popupRoot.editing
                sourceComponent: editorComponent
            }
        }
    }

    // A screen with its own inner pages (Helpers) walks back through them first: it sets
    // backHook while it is shown.
    property var backHook: null
    function screenHandlesBack() { return typeof popupRoot.backHook === "function" && popupRoot.backHook() === true }
    function heightOf(loaded) { return loaded ? loaded.implicitHeight : 0 }

    function componentFor(name) {
        switch (name) {
        case "home": case "off_look": return homeComponent
        case "first_run": case "first_run_off": case "welcome_back": case "change": return pickerComponent
        case "connect": return connectComponent
        case "helpers": return helpersComponent
        case "more": return moreComponent
        case "help": return helpComponent
        case "memories": return memoriesComponent
        case "memorial": return memorialComponent
        case "goodbye": return goodbyeComponent
        case "menu": return menuComponent
        default: return loadingComponent
        }
    }

    Component { id: homeComponent; HomeScreen { app: popupRoot } }
    Component { id: pickerComponent; PickerScreen { app: popupRoot; mode: popupRoot.screen } }
    Component { id: connectComponent; ConnectScreen { app: popupRoot; offered: popupRoot.pageArgs.offered || [] } }
    Component { id: helpersComponent; HelpersScreen { app: popupRoot } }
    Component { id: moreComponent; MoreScreen { app: popupRoot } }
    Component { id: helpComponent; HelpScreen { app: popupRoot } }
    Component { id: memoriesComponent; MemoriesScreen { app: popupRoot } }
    Component { id: memorialComponent; MemorialScreen { app: popupRoot } }
    Component { id: goodbyeComponent; GoodbyeScreen { app: popupRoot } }
    Component { id: menuComponent; MenuScreen { app: popupRoot } }
    Component { id: editorComponent; NameEditor { app: popupRoot } }
    Component {
        id: loadingComponent
        Item {
            implicitHeight: theme.u(160)
            PetPlayer {
                anchors.centerIn: parent
                width: theme.u(120)
                height: width
                registry: null
                species: ""
            }
        }
    }
}
