# CodePet QML UI mock

This is a standalone Qt Quick `Window`, **not a Quickshell plugin**. It is disconnected from Rust and does not implement the specification's release gates. No upstream code, sprites, or audio are copied.

## Preview

With a compatible Qt 6 development/runtime installation containing QtQuick, QtQuick.Controls, QtQuick.Layouts and QtQuick.Window, from the repository root:

```sh
qml qml/codepet/shell.qml
```

Some distributions name the Qt 6 launcher differently. This command is a development preview, not installation or an Omarchy enablement command. Qt/Quickshell/Omarchy compatibility and visual behavior require target-host validation.

## Walkthrough

1. Four high-is-good meters start at 70. There is no decay or timer.
2. Choose a category and fill task, folder and scope. These strings are display-only: no filesystem canonicalization or validation is claimed.
3. Read the summary and acknowledge it. Edits clear the acknowledgement. Only **Start mock task** starts a simulated task, with a frozen snapshot of the form.
4. Click explicit buttons to advance preparing → running → verifying → review. The running state can simulate waiting for user, and active states can simulate failure or cancellation. Cancelling requires a second explicit advance to cancelled.
5. Review says **NOT VERIFIED**, because no command ran. Acknowledge it, enter a nonblank short override reason (maximum 512 characters), and accept to demonstrate one in-memory +20 category reward, clamped to 100. The acceptance guard prevents a second reward for that mock task. Discard, failure and cancellation grant nothing.
6. Only terminal states offer a new task. Closing the window drops all mock state, including rewards.

This UI never launches a provider or shell command, reads/writes project files, connects to a daemon, or applies changes. It has no actual file picker, diff, provider integration, persistence, process containment, verification evidence, rollback, or restart idempotency. The acknowledgement and required override reason demonstrate an explicit unverified-result override. The trimmed reason is held only in memory after acceptance and cleared for a new task; this is not a production verification override flow. No resource-performance targets have been measured. Its plain-text labels must remain plain text when connected to untrusted real output later.

## Manual validation on a Qt-capable host

- Confirm startup is idle and all meters read 70; idle/waiting cause no progression
- Confirm blank fields or an unchecked acknowledgement disable Start; changing any field clears acknowledgement
- Confirm acceptance stays disabled until review is acknowledged and a nonblank override reason is entered
- Run each category through review: only its meter increases, and never above 100
- Confirm failed/cancelled/discarded tasks grant no reward; a second active task cannot be created
- Verify keyboard navigation, focus visibility, small-window scrolling and screen-reader labels
- Close and reopen: all values reset and nothing resumes

No Omarchy or target-desktop runtime testing is implied by these instructions.

### Validation status in the generation environment

A `qmllint` invocation was attempted but could not run: the installed wrapper points to missing `/usr/lib/qt5/bin/qmllint`. No Qt 6 QML runtime/linter was available at the inspected locations. The source was statically reviewed; it has not been linted, launched, visually inspected, or tested on Qt, Quickshell, or Omarchy. The manual checklist above remains open.
