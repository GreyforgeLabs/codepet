# Omarchy bar plugin `greyforge.codepet`

The CodePet bar widget for Omarchy 4 (tested against the Omarchy 4.0.4 plugin API and
Quickshell 0.3.1). The source is `omarchy-plugin/`; the installer copies it, with no
symlinks, to `~/.config/omarchy/plugins/greyforge.codepet/`.

The widget is **read-mostly**. It renders the bar-safe `bar-state.json` that the `codepet` CLI
writes after every settling command, and it sends a request only after an explicit trigger.
Tasks are simulations: the plugin never sends a `task.*` method.

## Files

| File | Role |
|---|---|
| `manifest.json` | id `greyforge.codepet`, kind `bar-widget`, default section `right`, settings defaults and schema |
| `BarWidget.qml` | entry point: settings, paths, `bar-state.json` reader, leader election, cell, tooltip, popup loader, launcher verbs |
| `BarPopup.qml` | the popup card (an Omarchy `PopupCard` inside a `Loader`, the CRIS pattern) |
| `BarRpc.qml` | the request transport: one argv-only `codepet rpc` process per request |
| `PresenceRecorder.qml` | the attended-presence recorder (leader only) |
| `CodePetBar.js` | pure model: parsing, settings, paths, status, glyphs, copy, predictions |
| `PresenceModel.js` | pure presence model: ticks, look-back, session, payload |
| `README.md`, `CHANGELOG.md`, `LICENSE` | plugin docs and the repository MIT licence |
| `LifeText.js` | **not checked in here**: the installer copies `qml/shared/LifeText.js` in byte-for-byte |

`omarchy plugin validate` accepts the staged folder (checked on a copy under
`~/Work/runs/codepet--2026-10-15--v0.2/plugin-stage/`; never installed by the tests).

## Paths

Nothing is ever written inside the plugin folder: the shell hot-reloads plugins on any change
there. All paths are computed by `CodePetBar.paths()`:

| Path | Default | Use |
|---|---|---|
| state dir | `$XDG_STATE_HOME/codepet` (or `~/.local/state/codepet`) | `bar-state.json` (read), `presence.json` (written by the leader) |
| runtime | `$XDG_DATA_HOME/codepet/current/` | thumbnails `qml/assets/pets/<id>/thumb-s<n>.webp`, `registry.json` |
| binary | `~/.local/bin/codepet` (setting `binary`) | `codepet rpc` |
| launcher | `~/.local/bin/codepet-overlay` | desktop-pet verbs |
| settings | `$XDG_CONFIG_HOME/omarchy/shell.json` | this widget's own entry (read, watched) |

Settings are read from the widget's own entry in `shell.json` (preferred over the injected
copy, which goes stale after a hot reload) and written only through
`bar.shell.updateEntryInline()`.

### Test state home

When the `stateHome` setting holds an absolute path, both files move under it, every
`codepet rpc` process gets `XDG_STATE_HOME=<stateHome>` in its environment, and every
launcher call runs as `/usr/bin/env XDG_STATE_HOME=<stateHome> codepet-overlay <verb>`.
The popup shows "Test state". Live QA uses this so the operator's real pet is never touched.
Relative paths and paths containing `..` are ignored.

## Settings

| Key | Default | Meaning |
|---|---|---|
| `label` | `name` | `name`, `status` (name plus the most urgent need) or `none` |
| `presence` | `true` | count active time while the widget is on the bar |
| `overlayOnLoad` | `false` | the leader runs `codepet-overlay show` once at startup |
| `binary` | `""` | absolute path of `codepet` |
| `stateHome` | `""` | absolute test state home (see above) |

## What it shows

**Bar cell:** the thumbnail of the current look and stage (falls back to the text `CP`), a
status glyph, and the optional label. A vertical bar shows only the thumbnail and glyph. The
cell is static; there is no bar animation.

| Glyph | Status (highest precedence first) |
|---|---|
| `+` | no living companion |
| `✿` | memorial (an ending is recorded once the current task is closed) |
| `!!` | urgent, or fainted (gentle mode) |
| `◦` | not set up: time isn't counting until the life rules are chosen |
| `!` | warning: a need is low |
| `?` | a result is waiting for review |
| `z` | napping |
| `‖` | paused (Pause/Travel) |
| `•` | a simulation task is working |
| `★` | unseen growth |
| (none) | healthy, or no state yet (`CodePet`) |

Colour is never the only cue: every non-healthy status has a glyph. The cell's accessible
name reads, for example: "CodePet: Ginger, Desk Cat, level 2. Warning: hydration low; water
within about 1 hour 50 minutes of active time. Task: running (simulation)."

**Predicted badge (leader only).** `bar-state.json` changes only on a settling command, so the
leader compares the attended time it committed since `generated_at_ms` with the Rust-computed
windows in the file (`until_critical_ms`, and `until_warning_ms` when present; a remaining nap
is subtracted first). The badge shows the more severe of committed and predicted, capped at
`!!`. Predicted text is marked `≈`. A prediction never shows an ending, `+` or `✿`. Other
screens show the committed state only.

**Tooltip:** a few plain-text lines, shown through `bar.showTooltip` after a 250 ms
single-shot `codepetBar.hoverGrace` timer that absorbs the spurious hover exit/enter pairs
caused by mapping the popup window.

**Popup card:** a 48 px thumbnail and "Ginger · Desk Cat · Level 2 · 120 XP · grows into a
Grand Inspector at level 4 (280 XP to go)"; mode, pace, paused and set-up badges; the life
status line in the lifecycle copy; four need rows with "about X of active time"; readiness;
the task line; "as of HH:MM · simulation only"; and buttons:

- care (species care labels): Food, Water, Play, Wash, Bathroom, Routine;
- Bed or Wake; Pause or Resume;
- Show/hide pet, Tasks…, Companion & looks…, Life & memorial… (Adopt… with no life,
  Set up… when not set up), Refresh;
- the bar label mode (name, status, none).

A 1 s `codepetBar.sleepCountdown` timer runs only while the popup is open and the pet is
napping; it updates the displayed countdown only.

## Interaction and requests

| Trigger | Effect |
|---|---|
| left click | toggles the popup; opening sends one `state.get` (it settles presence and may record an ending); an unseen growth is shown once ("★ Ginger grew into a Desk Cat!") and then acknowledged with one `pet.events_seen` |
| right click | `codepet-overlay toggle` (detached argv) |
| middle click | `codepet-overlay tasks` (detached argv) |
| leader startup | one `state.get` (settles, exports `bar-state.json`, creates the boot cursor); `codepet-overlay show` if `overlayOnLoad` |
| Refresh | one `state.get` |
| care / bed / wake / pause | one `pet.care` / `pet.sleep` / `pet.wake` / `pet.pause` |

`BarRpc.qml` rules:

- **Allowlist:** `state.get`, `pet.care`, `pet.sleep`, `pet.wake`, `pet.pause`,
  `pet.events_seen`. Anything else, including every `task.*` method, is refused before a
  process exists. Task supervisors must never be children of the shell.
- One request in flight; buttons are disabled while busy; success is shown only after the
  response. Frames are bounded to 64 KiB in both directions.
- `codepetBar.rpcWatchdog` (15 s, single shot) runs only while a request is in flight; its
  handler only sends SIGTERM to that process and reports a timeout.
- Care, bed, wake and pause reuse one request id per press until a response confirms it
  (the `PetCare.qml` pattern), so a retried press is applied at most once.
- A binary that cannot start is reported as "CodePet is not installed: run
  packaging/omarchy/install.sh" (Quickshell 0.3.1 `Process` has no error signal, so this is
  detected as `running` returning to false without `started`).

File events never launch Rust: `bar-state.json` is watched with `FileView { watchChanges }`
and only re-read. After any response, every instance re-reads the file (a file read via
`broadcast`), in case a watcher missed the atomic rename or the file's creation.

## Attended presence (`PresenceRecorder.qml`, `PresenceModel.js`)

One bar exists per monitor; the recorder is created only on the leader,
`bar.moduleWidgets("greyforge.codepet")[0]`. A new leader starts a fresh session.

Eligibility (all must hold): the `presence` setting; `presence.wanted === true` in
`bar-state.json` (alive, armed, not paused, no slot task, no pending ending; sleep is
deliberately not part of it); a valid `presence.life_id`; not idle
(`IdleMonitor { timeout: 180; respectInhibitors: false }`, so video inhibitors do not count
and a lock leads to idle); no fullscreen window on the focused workspace; and the state
directory proven by a successful `bar-state.json` load.

`codepet.presenceTick` (60 s, repeating) runs only while eligible:

- starting the timer records the wall time and never credits;
- a tick whose wall gap is 50–90 s becomes a pending tick; any other gap (suspend, stall,
  clock jump, negative) clears every pending tick;
- pending ticks commit once they are at least 180 s old (the idle look-back);
- idle or any loss of eligibility clears all pending ticks, so the minutes before idle was
  noticed are never credited;
- each committed tick adds `min(gap, 60 s)` to `attended_ms` and one to `ticks`.

After a commit the recorder writes `presence.json` (atomic, at most once per tick) with exactly
`{v, session, life_id, started_wall_ms, updated_wall_ms, attended_ms, ticks}`. A session never
spans two lives: a new `life_id` starts a new session. A failed write freezes time; nothing
retries from a timer. Rust validates the file, caps credit by CLOCK_BOOTTIME and consumes each
session's minutes exactly once.

Active time counts only while the CodePet bar widget is on your Omarchy bar and you are at the
computer. The overlay and the standalone preview record nothing.

## Timers

Exactly four, none of which sends a request or launches anything:

| Timer | Where | Runs |
|---|---|---|
| `codepetBar.hoverGrace` | `BarWidget.qml` | 250 ms single shot after a hover change |
| `codepet.presenceTick` | `PresenceRecorder.qml` | 60 s, leader only, only while eligible |
| `codepetBar.sleepCountdown` | `BarPopup.qml` | 1 s, only while the popup is open and the pet naps |
| `codepetBar.rpcWatchdog` | `BarRpc.qml` | 15 s single shot, only while a request is in flight |

## Tests

- `node --test qml/tests/test_bar_model.mjs`: parsing bounds, settings and paths (including
  the test state home), thumbnails rejecting `../x`, the allowlist, retry keys, a glyph for
  every status, the predicted badge (escalates to `!`/`!!`, capped, never an ending), copy and
  formatting.
- `node --test qml/tests/test_presence_model.mjs`: gap 49,999 → nothing; 50,000 and 90,000
  credit after the look-back; 90,001 → nothing; negative gaps; idle/lock drops the last ≤ 3
  pending ticks; ineligible stops writes; a life change starts a new session; sleep does not
  stop recording; the Rust bounds hold over 3,000 jittery ticks.
- `python3 qml/tests/test_bar_plugin.py`: manifest and version, no symlinks, only
  `presence.json` is written, exactly the four named timers, no timer handler sends a request,
  `state.get` only in startup, popup-open and Refresh, argv-only processes, the test state
  home, `qmlformat` parsing, the staged `LifeText.js` byte identity. When `qs` and
  `/usr/share/omarchy/shell` exist it also loads the real plugin under an isolated offscreen
  Quickshell (two instances, a fake `codepet` and a fake launcher) and checks the leader's
  single startup read, re-reads on file events without requests, popup `state.get` then
  `pet.events_seen`, care, client-side refusal of `task.start`, launcher argv and environment,
  settings writes and the not-installed path.
  `CODEPET_PLUGIN_STAGE_DIR=<dir>` also stages the plugin there and runs
  `omarchy plugin validate` on that copy.

## Limits

- The offscreen load test has no compositor: idle monitoring, Hyprland fullscreen, the popup
  window and real bar placement are not exercised there. Live behaviour on greyarch is recorded
  separately in `docs/validation.md`.
- An ending is recorded only on the next settling command (popup open, care, Refresh, CLI, a
  task action or bar startup). The bar shows "as of HH:MM".
- Presence is a convenience clock, not a tamper-proof economy: a same-user process can write
  `presence.json` within the Rust bounds, and deleting it stops decay.
- `FileView` watching across atomic renames must be confirmed live; the explicit popup-open
  `state.get` and the post-response re-read are the fallback.
