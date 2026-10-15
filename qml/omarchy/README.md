# Quickshell desktop overlay host

[`../shell.qml`](../shell.qml) (`qml/shell.qml`) is the real host entry, targeting **Quickshell 0.3.1** on a Wayland compositor that supports `wlr-layer-shell` (including Hyprland/Omarchy). It uses the existing `HostAdapter` and real `codepet rpc`, whose sole provider remains the non-executing mock. It is not an Omarchy plugin installation. No autostart, packages, compositor rules or lock hooks are installed.

## Explicit launch

With Quickshell already installed and a built `codepet` on PATH, from the repository root:

```sh
PATH="$PWD/target/debug:$PATH" qs -p "$PWD/qml/shell.qml"
```

The entry lives at `qml/`, not in this directory: Quickshell 0.3.1 only loads QML inside the entry's folder (its qmlscanner refuses `../shared`), so `shared/`, `omarchy/` and `assets/` must all be below it. `HostAdapter.qml` and `OverlayPolicy.js` stay here and are imported as `omarchy`.

For isolated testing, set `XDG_STATE_HOME` to a temporary directory before launching. Startup performs one `state.get`; no idle RPC polling or autonomous Start is added. `CODEPET_SCREEN=DP-1` selects an exact output name; if absent/disconnected, the first currently connected screen is used. No screens means a hidden overlay. `CODEPET_REDUCED_MOTION=1` disables scene motion.

The overlay is transparent, uses the Top layer deliberately, reserves zero workspace area and requests no keyboard focus. It has one screen owner, not a pet per monitor. Its input region is the union of the scene's pet, menu and care-prop rectangles; inactive/invalid rectangles are empty and all rectangles are clipped to current logical screen dimensions. The rest of the desktop is click-through. Scene geometry responds to size changes; switching/removing the owning screen preserves the persistent QML scene and its care controller, including in-flight callbacks. Only the native surface may be recreated by Quickshell; the transport and supervisor remain alive. Null screen assignment is supported by [Quickshell’s screen setter](https://github.com/quickshell-mirror/quickshell/blob/v0.3.1/src/window/proxywindow.cpp); visibility is explicitly false when there is no selected screen.

Tasks/review open in an independent, ordinary focusable `FloatingWindow` only after a pet action or explicit IPC call. Its care controls remain available for keyboard users. Closing it hides that window and leaves the root-owned transport and job processes intact. Quickshell 0.3.1 itself disables quit-on-last-window-close ([launch source](https://github.com/quickshell-mirror/quickshell/blob/v0.3.1/src/launch/launch.cpp)). Exiting/reloading the whole shell is different and is not a supported way to leave a live supervisor running.

## Explicit visibility controls and integration boundary

The IPC target is `codepet`. Select this config using the installed Quickshell CLI's instance/path selector when other shells run; inspect `qs ipc --help` and `qs ipc show` first. With this instance selected, calls include:

```sh
qs ipc call codepet hide
qs ipc call codepet show
qs ipc call codepet tasks
qs ipc call codepet closeTasks
qs ipc call codepet setReducedMotion true
qs ipc call codepet selectScreen DP-1
```

Hide only hides the pet, without closing an already-open task window. Restore does not override session/fullscreen suppression. The hidden root remains available over IPC, so restoring does not require killing/relaunching the process.

Automatic **screen-lock, suspend and fullscreen detection is not implemented**. The host provides `sessionLocked`, `sessionSuspended` and `selectedScreenFullscreen` integration inputs (false initially). `setSessionState(locked, suspended)` and `setFullscreen(value)` expose those inputs for explicit testing or a future trusted, event-driven session/compositor adapter. These are not evidence of a lock, sleep or fullscreen state. No polling, process-name heuristics, screen surveillance or `WlSessionLock` detector is used. `WlSessionLock` is a lock-creation API, not an observer of the user's existing locker.

A production integration must supply trustworthy events and a fresh initial state, handle disconnects/restarts, and recompute fullscreen for the current selected output. Do not claim automatic suppression until that integration is implemented and tested. Session suppression hides both windows and deactivates scene timers; fullscreen suppresses only the pet. Optional decay remains unimplemented in this host; these booleans do not implement or settle decay.

## Verification and remaining live gate

```sh
python qml/tests/test_quickshell_host.py
```

This checks source contracts plus executable JavaScript monitor-selection and hit-region policy (Node required for that portion). It is **not** proof of Quickshell loading, native focus behavior or Wayland input passthrough. Qt6 qmllint can parse the file here but cannot resolve missing Quickshell imports. No live Omarchy/Wayland session or Quickshell binary was available for this change.

Before release, run the actual entry on the target Quickshell/Omarchy version and verify:

1. Empty desktop pixels click through; pet, opened menu and care props receive input, with no invisible hit boxes after closing them
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
