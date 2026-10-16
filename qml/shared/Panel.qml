pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "LifeText.js" as LifeText

// Shared CodePet panel (the v0.2 window views, kept for the standalone preview). Renders
// the Rust-owned snapshot only; it never changes meters or rewards itself. Every action
// goes through `host.request` and is followed by an explicit `state.get` refresh. The
// v0.3 task pipeline is retired (SPEC-v0.3 §6.5): no task form, review, timeline, meters
// or progression panel. The v0.3 UI is the bar popup (omarchy-plugin/).
//
// Host contract (see qml/README.md):
//   property var snapshot          last `state.get` result, set by the host
//   property bool busy             a request is awaiting its response
//   property string lastError      "CODE: message" of the last failed request, else ""
//   function request(method, params, callback[, requestId])
//                                  callback receives the parsed response envelope
// v0.2 optional: clientTag (request-id prefix), presentationLocks (care routines running),
//   openCompanion() / openLife() (host-level view requests; this panel also has its own).
//
// Views: home (everything, the standalone default), care, companion and life. With no living
// companion, home and life show the adoption form; with an unarmed life, a small "Set up
// life rules" banner links to Life.
ColumnLayout {
    id: root
    required property var host
    property string view: "home"
    property bool showPetCare: true
    // The host's "less motion" preference; reaches the Care tab's sprite.
    property bool reducedMotion: false
    property string notice: ""
    // Optional generated pet registry (PetRegistry.js REGISTRY); LifeText falls back to the roster.
    property var registry: null
    readonly property var snap: host && host.snapshot ? host.snapshot : null
    readonly property var life: snap && snap.life ? snap.life : null
    // A v0.2 snapshot that reports no living companion (a v0.1 snapshot has no `life` key).
    readonly property bool noLife: LifeText.hasNoLife(snap)
    readonly property bool lifeAware: !!(snap && ("life" in snap))
    readonly property var unseenEvents: snap && Array.isArray(snap.unseen_events) ? snap.unseen_events : []
    readonly property bool inView: view === "home" || view === "care" || view === "companion" || view === "life"
    function shows(names) { return names.indexOf(root.view) >= 0 }
    readonly property bool hostBusy: !!(host && host.busy)
    spacing: 14

    function refresh() {
        root.host.request("state.get", {}, function (resp) {
            // The adoption form's exact numbers: one read, chained after this explicit refresh.
            if (resp && resp.ok && root.noLife && adoption.visible && !adoption.rulesLoaded && !root.host.busy)
                adoption.loadRules()
        })
    }
    function openCompanion() {
        root.view = "companion"
    }
    function openLife() {
        root.view = "life"
    }
    // Marks the growth celebration as seen, after the user acknowledged the line.
    function acknowledgeGrowth() {
        if (!root.life || root.unseenEvents.length === 0 || root.hostBusy)
            return
        var through = 0
        for (var i = 0; i < root.unseenEvents.length; i++)
            through = Math.max(through, Number(root.unseenEvents[i].seq) || 0)
        root.act("pet.events_seen", { "life_id": root.life.life_id, "through_seq": through })
    }
    function growthText() {
        if (!root.life || root.unseenEvents.length === 0)
            return ""
        var top = 0
        for (var i = 0; i < root.unseenEvents.length; i++)
            top = Math.max(top, Number(root.unseenEvents[i].stage) || 0)
        return "★ " + LifeText.cleanName(root.life.name) + " grew into a "
            + LifeText.stageName(root.life.appearance || root.life.species, top || root.life.stage, root.registry) + "!"
    }
    // Run one action, then refresh from Rust whatever the outcome.
    function act(method, params, done) {
        root.notice = ""
        root.host.request(method, params, function (resp) {
            if (done)
                done(resp)
            root.refresh()
        })
    }

    Label {
        Layout.fillWidth: true
        visible: root.snap === null
        text: root.hostBusy ? "Loading CodePet state…" : "No state loaded yet."
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
    }

    Label {
        Layout.fillWidth: true
        visible: root.host && root.host.lastError !== ""
        text: "Error: " + (root.host ? root.host.lastError : "")
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        font.bold: true
        color: "#9b1c1c"
        Accessible.name: text
    }
    Label {
        Layout.fillWidth: true
        visible: root.notice !== ""
        text: root.notice
        textFormat: Text.PlainText
        wrapMode: Text.WordWrap
    }

    ColumnLayout {
        Layout.fillWidth: true
        visible: root.inView
        spacing: 14

        // Growth celebration from another UI: a static line until the user acknowledges it.
        RowLayout {
            objectName: "panel.growth"
            Layout.fillWidth: true
            visible: root.growthText() !== ""
            Label {
                Layout.fillWidth: true
                text: root.growthText()
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                font.bold: true
                Accessible.name: text
            }
            Button {
                objectName: "panel.growthSeen"
                text: "Yay"
                Accessible.name: "Mark the growth news as seen"
                enabled: !root.hostBusy
                onClicked: root.acknowledgeGrowth()
            }
        }

        // An unarmed life: nothing changes on its own until the rules are set up.
        RowLayout {
            objectName: "panel.rulesBanner"
            Layout.fillWidth: true
            visible: root.life !== null && root.life.armed === false && root.view !== "life"
            Label {
                Layout.fillWidth: true
                text: LifeText.cleanName(root.life ? root.life.name : "") + "'s life rules aren't set up yet, so the active-time clock is off."
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }
            Button {
                objectName: "panel.setUpRules"
                text: "Set up life rules"
                onClicked: root.openLife()
            }
        }

        // Life status at a glance (home and care).
        RowLayout {
            objectName: "panel.lifeSummary"
            Layout.fillWidth: true
            visible: root.life !== null && root.shows(["home", "care"])
            Label {
                objectName: "panel.lifeStatus"
                Layout.fillWidth: true
                text: root.life ? LifeText.lifeHeadline(root.life, root.registry) + ". " + LifeText.statusLine(root.snap, root.registry) : ""
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                Accessible.name: text
            }
            Button {
                text: "Life…"
                Accessible.name: "Open life and memorial"
                onClicked: root.openLife()
            }
        }

        // No living companion: adoption on home and life; a pointer elsewhere.
        AdoptionPanel {
            id: adoption
            objectName: "panel.adoption"
            Layout.fillWidth: true
            visible: root.noLife && root.shows(["home", "life"])
            host: root.host
            registry: root.registry
        }
        RowLayout {
            objectName: "panel.noLifeHint"
            Layout.fillWidth: true
            visible: root.noLife && root.shows(["care", "companion"])
            Label {
                Layout.fillWidth: true
                text: "No companion yet. Adopt one whenever you're ready; your tasks and history are unchanged."
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
            }
            Button {
                text: "Adopt…"
                onClicked: root.openLife()
            }
        }

        PetCare {
            objectName: "panel.petCare"
            visible: root.showPetCare && root.shows(["home", "care"]) && !root.noLife
            Layout.fillWidth: true
            host: root.host
            reducedMotion: root.reducedMotion
        }
        GroupBox {
            objectName: "panel.looks"
            Layout.fillWidth: true
            visible: root.view === "companion" && root.lifeAware && !root.noLife
            title: "Looks"
            PetChooser {
                id: chooser
                objectName: "panel.chooser"
                width: parent.width
                host: root.host
                registry: root.registry
                mode: "look"
                onClosed: root.view = "home"
            }
        }
        LifePanel {
            id: lifePanel
            objectName: "panel.life"
            Layout.fillWidth: true
            visible: root.view === "life" && root.lifeAware
            host: root.host
            registry: root.registry
        }
    }
}
