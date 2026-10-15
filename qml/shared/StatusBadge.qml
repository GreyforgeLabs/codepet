import QtQuick
import QtQuick.Controls

// Task state label with a shape glyph and words, so status never depends on color.
// `state` is the raw string from Rust; unknown values are shown verbatim as plain text.
Label {
    id: root
    property string taskState: ""
    readonly property var table: ({
        "draft": ["✎", "Draft (not started)"],
        "preparing": ["◔", "Preparing"],
        "running": ["▶", "Running"],
        "waiting_for_user": ["?", "Waiting for you"],
        "verifying": ["⌕", "Verifying"],
        "review_ready": ["☐", "Review ready: awaiting your decision"],
        "cancelling": ["■", "Cancelling"],
        "cancelled": ["■", "Cancelled: no reward"],
        "failed": ["✕", "Failed: no reward"],
        "interrupted": ["⚠", "Interrupted: no reward"],
        "recovery_required": ["⚠", "Recovery required"],
        "accepted_artifact": ["✓", "Accepted artifact"],
        "discarded": ["⌫", "Discarded: no reward"]
    })
    readonly property var entry: table[taskState] || ["?", "Unknown state: " + taskState]
    text: "[" + entry[0] + "] " + entry[1]
    textFormat: Text.PlainText
    font.bold: true
    Accessible.name: "Task status: " + entry[1]
}
