pragma ComponentBehavior: Bound
import QtQuick
import Quickshell.Io
import "BarState.js" as BarState

// Passive reader of `$XDG_STATE_HOME/codepet/bar-state.json` (SPEC-v0.3 §8). Rust rewrites
// that file atomically (temp + rename, under its database lock) after every committed
// change: a click in the bar popup, a fold of coding-app events, a clients job. This is
// the overlay's only input.
//
// This component NEVER issues RPC and holds no reference to a transport. A file event
// only parses the file and emits `updated(state)` for a strictly newer `state_version`
// (or another database's file). A missing, unreadable or malformed file emits
// `unavailable()` and is otherwise ignored. Changes are watched by inotify; there is no
// polling timer.
QtObject {
    id: root
    // Absolute file path, or "" to stay idle.
    property string path: ""
    // Highest state_version emitted so far for database `lastDbId`; older or equal files
    // of that database are ignored. A file naming another database (restored, recreated
    // or swapped) starts a new order instead of being ignored until its counter catches up.
    property real lastVersion: -1
    property string lastDbId: ""
    property int rejected: 0

    signal updated(var state)
    // The file could not be read or did not parse (nothing was emitted).
    signal unavailable()

    // Re-read the file (for example after an explicit `state.get` created it, since an
    // inotify watch cannot be placed in a state directory that did not exist yet).
    function reload() {
        if (root.path !== "")
            view.reload()
    }

    function ingest(text) {
        var state = BarState.parse(text)
        if (state === null) {
            root.rejected += 1
            root.unavailable()
            return false
        }
        var dbId = typeof state.db_id === "string" ? state.db_id : ""
        if (dbId === root.lastDbId && state.state_version <= root.lastVersion)
            return false
        root.lastDbId = dbId
        root.lastVersion = state.state_version
        root.updated(state)
        return true
    }

    property FileView view: FileView {
        id: view
        path: root.path
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onLoaded: root.ingest(text())
        onLoadFailed: root.unavailable()
    }
}
