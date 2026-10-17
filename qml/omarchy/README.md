# Quickshell desktop and work hosts (v0.4)

[`../shell.qml`](../shell.qml) targets Quickshell 0.3.1 with `wlr-layer-shell`
(Hyprland/Omarchy). It draws a transparent desktop pet and compact work card.
[`../work.qml`](../work.qml) hosts the native compact fallback or advanced
form/review window. No compositor rules, packages or lock hooks are installed.

## State and actions

- `bar-state.json` is watched through `BarStateWatcher.qml`. Its bounded parser
  accepts the compatibility v1/v2 envelope and only newer state versions from the
  same database. A changed database starts a new order.
- `work-state.json` contains owned-job display status/readiness, the scout toggle
  and deadline, a finding flag and generic preparation feedback. Private paths,
  prompts, findings, reports and raw errors are excluded.
- `WorkController.qml` hydrates private card details with `work-rpc get`. It merges
  display exports directly and reads details on relevant task transitions, avoiding
  an export/get feedback loop. Open active work uses bounded status polling;
  automatic scouting uses an eligible one-shot deadline, not fast idle polling.
- `WorkRpc.qml` invokes `[codepet, "work-rpc"]` with bounded JSON frames, one request
  slot and a response deadline. The backend owns scope, readiness, cooldowns,
  containment, cancellation, review and rewards.
- Clicking the pet opens its card. Care uses Omarchy's bar panel to open Home.
  Fix binds a finding and its digest; Review opens the full result. Closing either
  surface does not cancel or relaunch the independent bounded supervisor.
- `shared/petstate.js` retains external-app face priority; `OverlayFace.js` also
  reflects owned work. The UI never turns a cosmetic event into XP.

The shared `DesktopPet.qml` handles deliberate dragging in both axes. The host
persists position, monitor and pinning. Pinning stops wandering, not dragging.
Calm mode follows the bar widget's setting. The surface requests no keyboard
focus; text entry uses the bar popup or work window.

## Visibility and environment

Off and **Pet on my desktop** off exit the overlay. A short settle interval handles
a launch racing the bar's setting write. Explicit hide/show affects visibility
without losing saved placement/pinning. New owned work requires On; the backend
refuses Off/sleep/life replacement during execution or unresolved application.

The overlay follows the widget entry in `$HOME/.config/omarchy/shell.json`, matching
Omarchy's setting location. `CODEPET_BIN` selects the backend. `CODEPET_SCREEN`
selects an output; `CODEPET_REDUCED_MOTION=1` supplies Calm mode when no usable
widget setting exists. Launchers supply the installed runtime and sibling work
launcher; test overrides use isolated paths.

Automatic screen-lock, suspend and fullscreen detection is not implemented.
`setSessionState` and `setFullscreen` are explicit integration inputs; they are
not evidence of actual desktop state. The overlay does not record presence.

## Launcher and IPC

`codepet-overlay show|hide|toggle|card|refresh|status|quit` controls the installed
runtime. `card` opens the visible pet's card or `codepet-work compact` without
silently enabling a hidden pet. `codepet-work bug|research|build` opens advanced
work. The review window has its own `codepet-work` IPC target.

The overlay IPC target is `codepet`. Always use the separator:
`qs ipc -p <entry> call -- codepet <function>`.

| Function | Effect |
|---|---|
| `ping`, `status` | Scoped liveness and bounded display/placement diagnostics |
| `show`, `hide`, `toggle` | Explicit visibility, subject to suppression inputs |
| `openCard` | Open the compact card on an active surface |
| `openHome` | Open the care popup through Omarchy's bar-panel contract |
| `refresh` | Re-read exported pet state and request a bounded refresh |
| `setReducedMotion`, `selectScreen`, `setSessionState`, `setFullscreen` | Integration inputs |

The status `supervisor` flag is legacy overlay compatibility, not the authoritative
owned-work slot. Use `work-rpc get` for owned work. Stop/finish owned jobs and close
Work before upgrade or uninstall; neither hiding the pet nor closing Work cancels
its independent supervisor.

## Verification

```sh
python3 qml/tests/test_overlay_host.py
python3 qml/tests/test_overlay_drag.py
python3 qml/tests/test_work_host.py
python3 qml/tests/test_quickshell_host.py
QT_QPA_PLATFORM=offscreen python3 qml/tests/run_qml.py qml/tests/WorkCardSmoke.qml
node --test qml/tests/test_work_model.mjs qml/tests/test_overlay_flow.mjs
```

These use disposable fixtures, offscreen Qt/Quickshell and stubbed providers.
Installed hide/show and pin/position preservation were checked separately.
Actual compositor pointer/focus behavior for the new card, multi-monitor hotplug,
fractional scaling and whole-desktop resource behavior remain unqualified.
See [validation](../../docs/validation.md) for current and historical evidence,
and [work](../../docs/work.md) for limits and recovery.
