# Quickshell desktop pet host (v0.3)

[`../shell.qml`](../shell.qml) (`qml/shell.qml`) is the desktop pet, targeting **Quickshell 0.3.1** on a Wayland compositor with `wlr-layer-shell` (Hyprland/Omarchy). It is one transparent, non-focusable layer-shell surface on one screen whose input region is the pet's hit box; everything else, the picture-only thought bubble included, clicks through. There is no window: everything you do with CodePet happens in the bar popup (`omarchy-plugin/`), and a click on the pet opens it at Home (SPEC-v0.3 §3.8). No autostart, packages, compositor rules or lock hooks are installed by this file.

## What it reads, what it runs

- **`bar-state.json`** (`$XDG_STATE_HOME/codepet/`, default `~/.local/state/codepet/`), watched by `BarStateWatcher.qml` with a Quickshell `FileView { watchChanges: true }`; no polling timer. `BarState.js` parses it with bounds (≤ 16 KiB UTF-8, schema `codepet.bar.v1` or `codepet.bar.v2`, typed fields; the v0.3 `enabled`, `agent`, `treats` and `clients` blocks all or none, required for v2). Anything malformed is ignored as a whole. Only a strictly newer `state_version` of the same database (`db_id`) replaces the drawn document; another database's file starts a new order.
- **The face** comes from `../shared/petstate.js` (generated from `docs/v0.3/contracts/agent_vectors.json`), the same shared priority the bar cell and Home use: asleep/fainted, an app needs you, a need at now, an app working, a celebration, oops, a need at soon, idle. `../shared/OverlayFace.js` maps it to an animation and bubble icons. Celebrations play once per finish, first event and ready check (the scene records them in its own `seen`); growth plays once here and stays unseen in Rust until Home shows it. A new overlay process counts everything in the first export it draws as already celebrated (`OverlayFace.seenAtStart`), so a restart (Off to On, the switch off and on, a bar or session restart) never replays one; a celebration from before the process started is left to the bar and Home.
- **"Pet on my desktop"** is the bar plugin's `overlayOnLoad` setting in `$HOME/.config/omarchy/shell.json`, the file the Omarchy shell itself reads and writes (it ignores `XDG_CONFIG_HOME`, and so does the overlay), read-only through a watched `FileView`. A missing key reads as off (the manifest default), and so does a `version: 1` file whose bar has no CodePet cell (the cell was removed). Only no usable file (none, unparsable, not `version: 1`) reads as unknown: a standalone overlay keeps running. The bar side of the switch is `omarchy-plugin/DesktopPetFlow.js`.
- **Calm mode** is the same entry's `calm` setting, followed live (no walking, bob or hop; SPEC-v0.3 §3.6, §3.8). A missing key is off: Omarchy has no system reduced-motion signal, so the "follows system" default is the plugin default. Without a usable file, `CODEPET_REDUCED_MOTION=1` from the launch applies; a `setReducedMotion` call holds until the setting next changes.
- **RPC.** `HostAdapter.qml` is a plain explicit transport (one `codepet rpc` process per request, argv only, 64 KiB frames, a one-shot response timeout). The overlay sends at most two kinds of request: one startup `state.get` when the file is missing or unreadable (it creates the export), and one `state.get` per explicit `refresh` verb. A file event never launches codepet.
- **Home.** A click (or the `openHome` verb) runs `omarchy-shell -q shell summon greyforge.codepet` (`$OMARCHY_PATH/bin/omarchy-shell` when `OMARCHY_PATH` is set, else `omarchy-shell` from `PATH` run as `/usr/bin/env OMARCHY_PATH=/usr/share/omarchy omarchy-shell …`, because it refuses to run without `OMARCHY_PATH`), detached. Omarchy's `summon` calls the bar widget's `open()` (the bar-widget panel contract: `open()`, `close()`, `opened`), which the v0.3 bar plugin makes open Home. `-q` keeps it silent when the shell or widget is not there. Requests are coalesced: at most one Omarchy call per second however fast the pet is clicked (the hop and heart still answer every click); `status.home_requests` counts the calls sent.

## When the process exits

`OverlayFlow.exitReason(barState, desktopPet)`: when the file says `enabled: false` (CodePet is Off) or "Pet on my desktop" is off, `shell.qml` logs one line and calls `Qt.quit()` (SPEC-v0.3 §3.8, §7.3, K9: no overlay process while Off). Started while Off, it reads the file and exits at once without running codepet. Off always exits at once, and so does the switch turning off after this process saw it on. A switch read as off before that (a launch racing the bar's write after Let's go!, which Omarchy does not wait for) must hold for `OverlayFlow.SETTLE_MS` (1.5 s, one single-shot timer) before the process exits; the pet is not drawn meanwhile, and `status.exit_reason` names the pending reason so a launcher can report "the desktop pet is off" instead of a failure. Turning CodePet or the switch back on is the bar's job (`codepet-overlay show`).

## Environment

- `CODEPET_BIN`: the `codepet` binary for the explicit reads (argv `[CODEPET_BIN, "rpc"]`; default `codepet` from `PATH`). The launcher passes `~/.local/bin/codepet`.
- `CODEPET_SCREEN=DP-1` selects an exact output name; if absent or disconnected, the first connected screen is used. No screens means a hidden overlay.
- `CODEPET_REDUCED_MOTION=1`: Calm mode (no walking, hop or bob; transients show one frame) when there is no usable `shell.json`; the bar entry's live `calm` setting wins otherwise.
- `CODEPET_OPEN` (set by older launchers) is ignored: the launcher sends its verb after starting the overlay.

## Launcher and IPC

`packaging/omarchy/codepet-overlay` is the verb launcher; see its header for discovery, single-instance start and `quit`. The IPC target is `codepet`; always put `--` after `call` (`qs ipc -p <entry> call -- codepet <fn>`), or Quickshell 0.3.1 parses a function named like a subcommand (`show`) as that subcommand.

| Function | Effect |
|---|---|
| `ping` | `codepet-overlay shown` or `… hidden` (the launcher's liveness check) |
| `status` | one-line JSON: `pet`, `busy`, `supervisor` (always false: v0.3 has no task; the launcher's `quit` needs it), `enabled`, `desktop_pet`, `face`, `overlay`, `drawn`, `home_requests`, `state_version`, `appearance`, `stage` |
| `show`, `hide`, `toggle` | explicit visibility (does not override session/fullscreen suppression) |
| `openHome` | open the bar popup at Home (the stub `panel` verb and the v0.2 window verbs `tasks`, `care`, `companion`, `life` went with the v0.2 bar menu) |
| `refresh` | re-read the file and send one `state.get` (skipped while busy) |
| `setReducedMotion`, `selectScreen`, `setSessionState`, `setFullscreen` | integration inputs |

Automatic **screen-lock, suspend and fullscreen detection is not implemented**. `sessionLocked`, `sessionSuspended` and `selectedScreenFullscreen` are integration inputs (false initially), set only by `setSessionState` / `setFullscreen`; they are not evidence of a lock, sleep or fullscreen state. `WlSessionLock` is a lock-creation API, not an observer.

## Timers

The pet has one cosmetic scheduler (≤ 5 updates per second on a 200 ms grid, one long wake per idle pause, stopped while hidden, asleep or in Calm mode except to finish a short transient) and one single-shot timer armed for the face's `wake_at_ms` (the next moment the face can change without a new export: an app going stale, a celebration's grace or window, the error frame). Neither runs while nothing is drawn, so there is none while Off, before a pet exists, or while the surface is suppressed. `shell.qml` itself has one single-shot timer, the exit settle above.

## Verification

```sh
python qml/tests/test_quickshell_host.py
QT_QPA_PLATFORM=offscreen python qml/tests/run_qml.py qml/tests/OverlaySmoke.qml
QT_QPA_PLATFORM=offscreen python qml/tests/run_qml.py qml/tests/OverlayAgentSmoke.qml
node --test qml/tests/test_petstate.mjs qml/tests/test_overlay_flow.mjs qml/tests/test_desktop_pet_flow.mjs
```

`test_quickshell_host.py` checks source contracts, the pure policy JS (Node) and the launcher against a fake `qs`. With `qs` installed it runs the real entry offscreen in an isolated environment (private HOME/XDG/runtime directories, no display, stub `codepet` and `omarchy-shell` on `PATH`), with the layer-shell `PanelWindow` swapped for a `FloatingWindow`: reads with and without a file (no RPC when the file is there), file events (atomic renames, older/malformed/oversized ignored, database swaps), exit on Off and on "Pet on my desktop" off (switched off, the cell removed, a launch racing the write that turns it on, a `shell.json` under `XDG_CONFIG_HOME` ignored), Calm mode followed live, `openHome` through the stub (one call per burst, the packaged `OMARCHY_PATH` without one), the `tests/quickshell/shell.qml` transport harness and the launcher against one real instance. `tests/quickshell/real_rpc.qml` runs only with `CODEPET_TEST_BIN=<built codepet>`: it drives the real `codepet rpc` in a scratch state home (fresh install, `pet.start`, Off and On) and checks each export through `BarState.js`, `petstate.js` and `OverlayFlow.js`. Keep `TMPDIR` short (the default `/tmp`): Quickshell's IPC socket path must fit in 108 bytes.

These are **not** proof of layer-shell behaviour, native focus, Wayland input passthrough or the live Omarchy bar. Before release, on the target Quickshell/Omarchy (nested rig first, SPEC-v0.3 §9.4):

1. Empty desktop pixels and the thought bubble click through; the pet receives the click, hops and opens the bar popup at Home
2. The overlay never takes keyboard focus; zero reserved work area; unplug/replug of the owning output yields exactly one pet
3. Off from the popup or the bar menu ends the overlay process; "Pet on my desktop" off ends it; turning either back on brings it back
4. Explicit hide/show, Calm mode and the integration inputs stop and restart animation safely, including with no monitor
5. Once a trusted integration exists, test real locker/suspend/fullscreen events

API references: [PanelWindow](https://quickshell.org/docs/v0.3.1/types/Quickshell/PanelWindow/), [Region](https://quickshell.org/docs/v0.3.1/types/Quickshell/Region/), [WlrLayershell](https://quickshell.org/docs/v0.3.1/types/Quickshell.Wayland/WlrLayershell/), [FileView](https://quickshell.org/docs/v0.3.1/types/Quickshell.Io/FileView/), [IpcHandler](https://quickshell.org/docs/v0.3.1/types/Quickshell.Io/IpcHandler/).
