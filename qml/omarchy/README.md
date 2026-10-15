# Quickshell desktop overlay host

[`../shell.qml`](../shell.qml) (`qml/shell.qml`) is the real host entry, targeting **Quickshell 0.3.1** on a Wayland compositor that supports `wlr-layer-shell` (including Hyprland/Omarchy). It uses the existing `HostAdapter` and real `codepet rpc`, whose sole provider remains the non-executing mock. It is not an Omarchy plugin installation. No autostart, packages, compositor rules or lock hooks are installed.

## Explicit launch

With Quickshell already installed and a built `codepet` on PATH, from the repository root:

```sh
PATH="$PWD/target/debug:$PATH" qs -p "$PWD/qml/shell.qml"
```

The entry lives at `qml/`, not in this directory: Quickshell 0.3.1 only loads QML inside the entry's folder (its qmlscanner refuses `../shared`), so `shared/`, `omarchy/` and `assets/` must all be below it. `HostAdapter.qml`, `OverlayPolicy.js`, `BarState.js` and `BarStateWatcher.qml` stay here and are imported as `omarchy`.

For isolated testing, set `XDG_STATE_HOME` to a temporary directory before launching. Startup performs one `state.get`; no idle RPC polling or autonomous Start is added. `CODEPET_SCREEN=DP-1` selects an exact output name; if absent/disconnected, the first currently connected screen is used. No screens means a hidden overlay. `CODEPET_REDUCED_MOTION=1` disables scene motion.

Further environment inputs (v0.2):

- `CODEPET_BIN`: the `codepet` binary used for every request (argv `[CODEPET_BIN, "rpc"]`; default `codepet` from `PATH`). The launcher passes `~/.local/bin/codepet`.
- `CODEPET_OPEN`: one of `tasks`, `care`, `companion`, `life` opens that tab of the window at startup; anything else (including empty) opens nothing. The startup `state.get` is issued first, so opening the window does not add a second read.

## Installed launcher: `codepet-overlay`

[`packaging/omarchy/codepet-overlay`](../../packaging/omarchy/codepet-overlay) (POSIX sh, installed to `~/.local/bin`) is the only supported way for the bar plugin and the user to drive the overlay:

```sh
codepet-overlay show|hide|toggle|tasks|care|companion|life|refresh|status
```

- The entry is `${XDG_DATA_HOME:-$HOME/.local/share}/codepet/current/qml/shell.qml` (a relative `XDG_DATA_HOME` is ignored). `CODEPET_OVERLAY_QML`, `CODEPET_BIN` and `CODEPET_QS` override the entry, the binary and the Quickshell CLI (used by tests).
- **Finding a running overlay.** First `qs ipc -p <entry> call -- codepet ping`, which must answer `codepet-overlay shown|hidden`. If that fails, `qs list --all --json` is scanned for an instance whose config path is under `…/codepet/current/` or `…/codepet/releases/` (an overlay started from an older release path, or one too old to have `ping`); that instance counts as running and every verb addresses it with `qs ipc --pid <pid>`. A second overlay is therefore never started. Quickshell keys instances by the path string it was given and does not resolve the `current` symlink, so re-pointing `current` during an install keeps addressing the running overlay.
- `hide`, `refresh` and `status` never start the overlay. Every other verb starts it if nothing is running: `CODEPET_BIN=… CODEPET_OPEN=<verb or empty> qs -n -d -p <entry>` (`-n` also refuses a duplicate if two launchers race), then a bounded readiness wait of 30 × 0.1 s for `ping`, then the verb (`toggle` from a stopped overlay means `show`).
- Exit status: 0 success / `status` running, 1 failure / `status` not running, 2 usage. `status` prints `running` (plus `pid N, older release path` when found by `qs list`) and the overlay's one-line JSON `status()`.
- An overlay from a release without a verb answers `Function not found.`; the launcher reports it and suggests restarting the overlay once no task is executing. There is no quit verb.
- Every expansion is quoted and nothing is evaluated (checked by `test_quickshell_host.py`).

## Window tabs and host contract (v0.2)

The focusable task window has the tabs **Care · Tasks · Companion · Life**; adoption and retirement text input lives there, never on the click-through overlay. The tab selects `Panel.view` when the shared v0.2 `Panel` (which exposes `openLife()`) is present; with an older `Panel` every tab shows its home view. `HostAdapter` adds the shared host contract: `clientTag` (request-id prefix; the overlay uses `overlay`), `presentationLocks`, and `openCompanion()` / `openLife()`, which only signal the host to open that tab. `DesktopPet`'s `chooserRequested()` (v0.2) also opens Companion; older components without it are ignored.

## Passive state: `bar-state.json`

`BarStateWatcher.qml` watches `$XDG_STATE_HOME/codepet/bar-state.json` (default `~/.local/state/codepet/`, the bar-safe subset the Rust store exports atomically under its database lock after every committed change) with a Quickshell `FileView { watchChanges: true }`; there is no polling timer. `BarState.js` parses it with bounds (≤ 16 KiB UTF-8, schema `codepet.bar.v1`, typed fields, short reasons); anything malformed is ignored as a whole. Only a strictly newer `state_version` is merged by `HostAdapter.applyBarState()` into a copy of the presentation snapshot: meters, life (appearance, stage, level, XP, status, mode, pace, pause), care flags and readiness, the active task's state, unseen evolution events, lethal-need rows, the critical card and the memorial count. Care done from the Omarchy bar therefore shows on the pet. The subset carries no task id, so a task the overlay does not already know is not invented; it appears on the next explicit refresh.

**Neither the watcher nor the merge ever issues RPC.** Full task details refresh only on explicit actions (opening the window, Refresh, user clicks). Shared components must not request in response to a snapshot change. After every explicit `state.get` the watcher re-reads the file (a read, not RPC): an inotify watch cannot be placed on a state directory that does not exist yet, so if the overlay starts before the first export, the first explicit refresh (normally the startup read, whose `codepet` run creates the file) arms it. A merged subset that arrives while a `state.get` is in flight is re-applied over that older full snapshot.


The overlay is transparent, uses the Top layer deliberately, reserves zero workspace area and requests no keyboard focus. It has one screen owner, not a pet per monitor. Its input region is the union of the scene's pet and open-menu rectangles; decorative care props (food, water, tree, bed) have no handler and stay click-through, so a bed shown for a whole nap never becomes a dead click zone; inactive/invalid rectangles are empty and all rectangles are clipped to current logical screen dimensions. The rest of the desktop is click-through. Scene geometry responds to size changes; switching/removing the owning screen preserves the persistent QML scene and its care controller, including in-flight callbacks. Only the native surface may be recreated by Quickshell; the transport and supervisor remain alive. Null screen assignment is supported by [Quickshell’s screen setter](https://github.com/quickshell-mirror/quickshell/blob/v0.3.1/src/window/proxywindow.cpp); visibility is explicitly false when there is no selected screen.

Tasks/review open in an independent, ordinary focusable `FloatingWindow` only after a pet action or explicit IPC call. Its care controls remain available for keyboard users. Closing it hides that window and leaves the root-owned transport and job processes intact. Quickshell 0.3.1 itself disables quit-on-last-window-close ([launch source](https://github.com/quickshell-mirror/quickshell/blob/v0.3.1/src/launch/launch.cpp)). Exiting/reloading the whole shell is different and is not a supported way to leave a live supervisor running.

## Explicit visibility controls and integration boundary

The IPC target is `codepet`. Prefer `codepet-overlay`; for direct calls select this config with the installed Quickshell CLI's instance/path selector (`qs ipc -p <entry>` or `--pid`) and inspect `qs ipc --help` first. **Always put `--` after `call`**: Quickshell 0.3.1 otherwise parses a function named like an ipc subcommand (`show`) as `qs ipc show`, prints the target list and exits 0 without calling anything. With this instance selected, calls include:

```sh
qs ipc call -- codepet ping          # "codepet-overlay shown" or "... hidden"
qs ipc call -- codepet status        # one-line JSON: pet, window, view, busy, state_version, appearance, stage
qs ipc call -- codepet hide
qs ipc call -- codepet show
qs ipc call -- codepet toggle
qs ipc call -- codepet tasks         # also: care, companion, life (window tabs)
qs ipc call -- codepet refresh       # one explicit state.get, skipped while busy
qs ipc call -- codepet closeTasks
qs ipc call -- codepet setReducedMotion true
qs ipc call -- codepet selectScreen DP-1
```

Hide only hides the pet, without closing an already-open task window. Restore does not override session/fullscreen suppression. The hidden root remains available over IPC, so restoring does not require killing/relaunching the process.

Automatic **screen-lock, suspend and fullscreen detection is not implemented**. The host provides `sessionLocked`, `sessionSuspended` and `selectedScreenFullscreen` integration inputs (false initially). `setSessionState(locked, suspended)` and `setFullscreen(value)` expose those inputs for explicit testing or a future trusted, event-driven session/compositor adapter. These are not evidence of a lock, sleep or fullscreen state. No polling, process-name heuristics, screen surveillance or `WlSessionLock` detector is used. `WlSessionLock` is a lock-creation API, not an observer of the user's existing locker.

A production integration must supply trustworthy events and a fresh initial state, handle disconnects/restarts, and recompute fullscreen for the current selected output. Do not claim automatic suppression until that integration is implemented and tested. Session suppression hides both windows and deactivates scene timers; fullscreen suppresses only the pet. Optional decay remains unimplemented in this host; these booleans do not implement or settle decay.

## Verification and remaining live gate

```sh
python qml/tests/test_quickshell_host.py
```

This checks source contracts plus executable JavaScript monitor-selection, hit-region and bar-state parse/merge policy (Node required for that portion), and the launcher: static checks (POSIX sh, no `eval`, every expansion quoted, bounded wait) and its behaviour against a scripted fake `qs` (never a second copy; `hide`/`refresh`/`status` never start; older-release instances addressed by pid; bounded readiness). When `qs` is installed it also runs two isolated load smokes (offscreen Qt, no Wayland/X11 display, temporary HOME/XDG/runtime directories, a stub `codepet` on PATH): the real `qml/shell.qml` must get past Quickshell's config scanner and fail only with `No PanelWindow backend loaded`, and a staged copy with the layer-shell `PanelWindow` replaced by a `FloatingWindow` must load and answer `qs ipc call codepet show/hide/tasks/closeTasks/...`. With `qs` installed it also runs the real overlay offscreen: `CODEPET_OPEN`, the tab verbs, `toggle`, `refresh` (exactly one `state.get`, logged by a stub), the bar-state watcher (atomic renames, older/malformed/oversized files ignored, no RPC from file events), the `tests/quickshell/shell.qml` HostAdapter harness, and the launcher against one real instance including a re-pointed `current` symlink and an instance started from a `releases/` path. `tests/quickshell/real_rpc.qml` runs only with `CODEPET_TEST_BIN=<built codepet>`. The smokes are skipped where Quickshell is absent (as in CI). They are **not** proof of layer-shell behavior, native focus or Wayland input passthrough; those still need the live checklist below. Qt6 qmllint can parse the file but cannot resolve Quickshell imports.

Before release, run the actual entry on the target Quickshell/Omarchy version and verify:

1. Empty desktop pixels and care props click through; pet and opened menu receive input, with no invisible hit boxes after closing them
2. Overlay never takes keyboard focus; task window can type into the form, closes/reopens, and closing it during an explicit mock task preserves the supervisor
3. Top-layer stacking is intentional; zero reserved work area; screen-scale/geometry changes clamp all controls; unplug/replug owning output yields exactly one pet, including during an in-flight care request
4. Explicit hide/show, reduced motion and integration inputs stop/restart animation safely, including when no monitor exists
5. Once a trusted integration exists, test real locker/suspend/fullscreen events and session-adapter failures, not merely toggled booleans

API references checked against the official 0.3.1 documentation:
[PanelWindow](https://quickshell.org/docs/v0.3.1/types/Quickshell/PanelWindow/),
[Region](https://quickshell.org/docs/v0.3.1/types/Quickshell/Region/),
[QsWindow](https://quickshell.org/docs/v0.3.1/types/Quickshell/QsWindow/),
[WlrLayershell](https://quickshell.org/docs/v0.3.1/types/Quickshell.Wayland/WlrLayershell/),
[FloatingWindow](https://quickshell.org/docs/v0.3.1/types/Quickshell/FloatingWindow/),
[IpcHandler](https://quickshell.org/docs/v0.3.1/types/Quickshell.Io/IpcHandler/).
