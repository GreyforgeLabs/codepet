pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import Quickshell.Io
import "omarchy"
import "shared"
import "omarchy/OverlayFlow.js" as OverlayFlow
import "omarchy/WorkModel.js" as Work
import "omarchy/BarState.js" as BarState

ShellRoot {
    id: work
    property var state: null
    property var pet: null
    FileView {
        path: BarState.statePath(Quickshell.env("XDG_STATE_HOME"), Quickshell.env("HOME"))
        watchChanges: true
        printErrors: false
        onLoaded: {
            try { work.pet = JSON.parse(text()).life || null } catch (e) { work.pet = null }
        }
        onLoadFailed: work.pet = null
        onFileChanged: reload()
    }
    property bool compact: Quickshell.env("CODEPET_WORK_MODE") === "compact"
    readonly property bool busy: workController.busy
    readonly property var task: workController.task
    readonly property string error: workController.error
    function refresh() { workController.refresh() }
    function action(method, params) { workController.action(method, params) }
    function newJob() { workController.newJob() }
    WorkController {
        id: workController
        binary: Quickshell.env("CODEPET_BIN") || "codepet"
        pet: work.pet
        calm: Quickshell.env("CODEPET_REDUCED_MOTION") === "1"
        opened: window.visible
        automatic: work.compact && window.visible && !!work.pet && work.pet.armed === true
        summary: work.state
    }
    FileView {
        path: BarState.statePath(Quickshell.env("XDG_STATE_HOME"), Quickshell.env("HOME")).replace(/bar-state\.json$/, "work-state.json")
        watchChanges: true; printErrors: false
        onLoaded: { try { work.state = JSON.parse(text()) } catch (e) { work.state = null } }
        onFileChanged: reload()
    }
    IpcHandler {
        target: "codepet-work"
        function open(kind: string): void {
            work.compact = kind === "compact"
            if (!work.compact) panel.openKind(kind)
            window.width = work.compact ? 312 : 820
            window.height = work.compact ? Math.min(620, compactCard.implicitHeight + 100) : 820
            window.minimized = false; window.visible = true; work.refresh()
        }
        function status(): string { return JSON.stringify({ visible: window.visible, compact: work.compact, busy: work.busy, task: work.task ? work.task.status : null, error: work.error }) }
    }
    FloatingWindow {
        id: window
        title: "CodePet — Work"
        implicitWidth: work.compact ? 312 : 820
        implicitHeight: work.compact ? Math.min(620, compactCard.implicitHeight + 100) : 820
        minimumSize: work.compact ? Qt.size(280, 260) : Qt.size(480, 400)
        visible: true
        color: work.compact ? "#1f242c" : "#171e1b"
        onClosed: Qt.quit()
        Item {
            anchors.fill: parent
            visible: work.compact
            PetSprite {
                x: 18; y: 0; width: 80; height: 80
                petId: work.pet ? work.pet.appearance : "penguin"
                stage: work.pet ? work.pet.stage : 1
                semanticState: Work.active(workController.task) ? "working" : "idle"
                reducedMotion: workController.calm
            }
            Text {
                x: 106; y: 32; width: parent.width - 124
                text: work.pet && work.pet.name ? work.pet.name : "CodePet"
                textFormat: Text.PlainText; elide: Text.ElideRight; font.pixelSize: 19; font.bold: true; color: "#edf0f5"
            }
            WorkCard {
                id: compactCard
                x: 10; y: 80; width: parent.width - 20
                height: Math.min(implicitHeight, parent.height - 90)
                controller: workController
                closable: false
                onReviewRequested: { work.compact = false; window.width = 820; window.height = 820 }
                onHomeRequested: Quickshell.execDetached(OverlayFlow.openHomeArgv(Quickshell.env("OMARCHY_PATH")))
            }
        }
        WorkPanel {
            id: panel
            anchors.fill: parent
            visible: !work.compact
            controller: workController
            selectedKind: Work.kind(Quickshell.env("CODEPET_WORK_KIND"))
        }
    }
    Component.onCompleted: Qt.callLater(work.refresh)
}
