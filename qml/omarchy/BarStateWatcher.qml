pragma ComponentBehavior: Bound
import QtQuick
import Quickshell.Io
import "BarState.js" as BarState

// Passive reader of `$XDG_STATE_HOME/codepet/bar-state.json` (docs/omarchy-plugin.md, docs/lifecycle.md).
// The Rust store rewrites that file atomically, under its database lock, after every
// committed change, so care done from the Omarchy bar reaches the desktop pet here.
//
// This component NEVER issues RPC and holds no reference to the host transport. A
// file event only parses the file and emits `updated(state)` for a strictly newer
// `state_version` (or another database's file); the host merges the subset into its presentation snapshot. Full
// task details refresh only on explicit actions (opening the window, Refresh, clicks).
// A missing or malformed file is ignored. Changes are watched by inotify; there is no
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

    // Re-read the file. Used after explicit host actions so a file created after
    // startup (first `codepet` run) is picked up and watched. Reads the file only.
    function reload() {
        if (root.path !== "")
            view.reload()
    }

    function ingest(text) {
        var state = BarState.parse(text)
        if (state === null) {
            root.rejected += 1
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
    }
}
