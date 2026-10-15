# CodePet QML UI

Milestone 1 makes Rust the only authority for state. QML renders the snapshot returned
by `codepet` and sends explicit requests; it never changes meters, computes rewards or
advances task states itself. Result shapes are in `docs/milestone-1.md`.

## Layout

| Path | Imports | Role |
| --- | --- | --- |
| `shared/Panel.qml` | QtQuick, Controls, Layouts | Meters with high-is-good legend, task status, New task, Pause decay, Cancel, Resolve recovery, Refresh, Check interrupted tasks, recent results |
| `shared/TaskForm.qml` | same | Draft editor (loads a draft with `task.get`), preflight summary, acknowledgement, Start |
| `shared/TaskReview.qml` | same | Provider-authored/SIMULATED artifact, verification, acknowledgement, override reason, Accept artifact / Discard; Abandon (no reward) for `recovery_required` |
| `shared/WellbeingMeter.qml`, `shared/StatusBadge.qml` | same | One meter (integer hundredths); status glyph + words |
| `standalone/Main.qml`, `standalone/PreviewHost.qml` | same + `../shared` | Pure-QML preview, not connected to Rust |
| `omarchy/HostAdapter.qml` | QtQuick, Quickshell.Io | Quickshell transport: one `codepet rpc` process per request |
| `tests/` | | Headless harnesses (below); not part of the UI |

Quickshell imports appear only under `omarchy/` (and its test harness).

## What changed

The old `codepet/shell.qml` kept its own phase, meters and acceptance guard and added
+20 in QML. It is removed, together with the directory; `WellbeingMeter.qml` moved to
`shared/` and now takes hundredths. The originals stay in Git (import commit `124f004`).
Because of this, `docs/source-manifest.sha256` no longer matches its three `qml/` entries;
that manifest records the original import and is not updated here.

## Host contract

Every shared component takes a `host` object:

- `property var snapshot`: last successful `state.get` result. Only the host sets it.
- `property bool busy`: a request is waiting for its response. Hosts allow one request at
  a time and answer a second one with `CLIENT_BUSY`.
- `property string lastError`: `"CODE: message"` of the last failed request. It is cleared
  when a new non-`state.get` request starts, so the refresh after a failed action keeps it.
- `function request(method, params, callback[, requestId])`: sends one envelope; `callback`
  receives the parsed response envelope (`ok`, `result` or `error`). Client-side failures use
  the same shape with codes `CLIENT_BUSY`, `CLIENT_TIMEOUT`, `TRANSPORT`, `PROTOCOL_MISMATCH`
  or `OUTPUT_LIMIT`. `requestId` is optional; TaskReview passes one so an identical
  acceptance retry reuses its idempotency key, and changed parameters get a new key.
- Optional `property bool supervisorActive`: a started job's process is still alive. The
  panel uses it only to word its "Refresh" hint.

After every action a component calls `host.request("state.get", {})`. Nothing refreshes on a
timer. Start is sent only from the Start button's `onClicked`, and only with a preflight token
for the current task id and revision, after the acknowledgement checkbox is ticked. Any edit
clears the token and the acknowledgement; the token is dropped once Start is clicked, and an
expired token (checked at click time) is refused locally. Unavailable providers are shown
disabled as "Not yet supported on this version". Untrusted strings are rendered with
`Text.PlainText` (including provider labels, whose button text would otherwise be AutoText).

`state.get` lists task *briefs* (no folder, scope or artifact text, an instructions
excerpt). "Continue draft" therefore loads the full draft with `task.get` before opening the
form, and the review loads the full task with `task.review`. The form mirrors Rust's limits:
16 KiB of instructions and 32 KiB for folder, scope and instructions as JSON. The override
reason is limited to 512 UTF-8 bytes (counted in bytes, not characters).

A `recovery_required` task holds the slot until it is resolved. The panel then shows
"Resolve recovery", which opens the review read-only. The only action there is "Abandon (no
reward)", enabled after an acknowledgement; it sends `task.discard` with the task's stored
digest and states that no files were restored or changed.

## Standalone preview

```sh
qml6 qml/standalone/Main.qml
```

This opens an ordinary window bannered "PREVIEW – not connected to Rust; no rewards are
stored". `PreviewHost` serves fixed fixtures (choose one in the header: idle, running,
review not verified, review with a failed check, recovery required) and logs each envelope it
would send with `console.log`. Draft and preflight answers are canned so the form can be
inspected; the preflight token is not valid anywhere. Fixture snapshots use the brief shape; `task.get` and `task.review` return the full fixture
task. Start, Accept, Discard (including Abandon), Cancel, Pause
and Recover are refused with `PREVIEW_ONLY`, so the snapshot and meters never change. Qt may
send `console.log` to the journal; set `QT_FORCE_STDERR_LOGGING=1` to see it in the terminal.

The real standalone host is a later minimal Qt 6 C++ launcher with a `QProcess` transport
(spec §6). It is not built in this milestone.

## Omarchy adapter

`HostAdapter.qml` runs `command` (default `["codepet", "rpc"]`, argv only, no shell) for
each request: it writes one request line on stdin, then sets `stdinEnabled = false`, and
reads the first stdout line through a `SplitParser`. The response must be a v1 envelope whose
id matches (or `null` on an error). Ids are `qs-<time>-<n>`. A one-shot watchdog
(`responseTimeoutMs`, 15 s) sends SIGTERM only if no response line has arrived. Each
`Process` is destroyed only after it stops running, so the `task.start` process, which stays
alive as the per-job mock supervisor, is not killed by cleanup. Stderr is kept as a short
tail for error messages.

Quickshell 0.3.1 API checked against `/usr/lib/qt6/qml/Quickshell/Io/quickshell-io.qmltypes`
(no Quickshell docs or sources are installed on this host): `Process.command`
(list<string>), `running`, `stdinEnabled`, `stdout`/`stderr` (DataStreamParser), `write(string)`,
`signal(int)`, `started()`, `runningChanged()`; `SplitParser.splitMarker` and `read(data)`.
`exited(exitCode, exitStatus)` exists but is not used: qmllint cannot resolve its
`QProcess::ExitStatus` parameter, and `runningChanged` also covers a failed start. Closing
stdin with `stdinEnabled = false` follows existing local Quickshell code; `codepet rpc` reads
one newline-terminated frame and does not need EOF.

Known limits:

- `SplitParser` buffers until a newline, so QML cannot bound a newline-free stdout stream;
  oversize lines are rejected after they arrive.
- The supervisor is a child of the Quickshell process. Reloading or quitting the shell
  during a mock job may stop it; `task.recover` then marks the task interrupted. The spec's
  transient systemd unit for Start is deferred.
- Quickshell only loads QML inside the config folder (paths outside resolve to
  `qrc:/qs-blackhole`), so an integration must ship `shared/` inside its plugin or config
  folder. No `BarWidget` exists yet. A host opening the panel should call `panel.refresh()`.

Nothing here registers a plugin, edits `~/.config`, enables autostart or touches a running
shell.

## Validated on greyarch (2026-10-14, after the milestone 1 review fixes)

- `/usr/lib/qt6/bin/qmllint -I qml` (Qt 6.11.2) on every file under `shared/`, `standalone/`,
  `omarchy/` and `tests/PreviewSmoke.qml`, and on the staged Quickshell harness: no output.
  This qmllint does report problems (checked with a deliberately broken file), but it exits 0
  on warnings, so "no output" is the criterion.
- `qml6 qml/tests/PreviewSmoke.qml` (offscreen, under `timeout`): 34 checks pass, including
  the new ones for the 512-byte override limit (200 CJK characters disable Accept), Resolve
  recovery and Abandon gating on a `recovery_required` fixture, and Continue draft loading
  the full draft through `task.get`.
- The staged Quickshell harness against `fake_rpc.py`: 15 checks pass (the earlier note below
  said 16; the harness has 15 checks).
- `tests/quickshell/real_rpc.qml`, staged as `shell.qml` next to `omarchy/` and run with
  `CODEPET_BIN=<repo>/target/debug/codepet CODEPET_TEST_ROOT=<tmp dir with home/ and project/>`
  (headless Quickshell, `env -i`, offscreen, `QT_FORCE_STDERR_LOGGING=1`, under `timeout`):
  9 checks pass against the **real** `codepet rpc`, through create, preflight, explicit start,
  review, one acceptance, replay and the refreshed snapshot.
- Still not validated: a visible window, a live
  Omarchy panel, keyboard and screen-reader use.

## Earlier validation in this environment (2026-10-14, before the review fixes)

- `qmllint` from Qt 6.11.2 (`/usr/lib/qt6/bin/qmllint -I qml <file>`) on every file: no
  warnings. `/usr/bin/qmllint` on this host is Qt 5's and reports nothing for any input,
  including deliberately broken files.
- `timeout 8 env QT_QPA_PLATFORM=offscreen qml6 qml/standalone/Main.qml`: loads with no QML
  errors; the only output is the logged `state.get` envelope; exit 124 from `timeout`.
- `qml6 qml/tests/PreviewSmoke.qml` (offscreen): 23 checks passed at that time. It covers the fixture load,
  disabled unavailable provider, Start disabled before preflight and acknowledgement, edits
  clearing preflight and acknowledgement, Start on a fresh revision 2 preflight,
  `PREVIEW_ONLY` refusal with meters unchanged, review acceptance gating (acknowledgement and
  override reason), and FAILED verification text.
- `qml/tests/quickshell/shell.qml` in headless Quickshell 0.3.1 against
  `tests/quickshell/fake_rpc.py`, a test-only stub: 16 checks pass. They cover the stdin/stdout
  round trip, error envelope to `lastError`, the start response read while the process
  lives, `supervisorActive` set and cleared, exit without a response, non-JSON, wrong id,
  watchdog timeout, a missing binary, and the shared Panel rendering the adapter snapshot. Run:

  ```sh
  T=$(mktemp -d) && cp -r qml/omarchy qml/shared qml/tests/quickshell/* "$T"/ && \
  mkdir -p "$T/xdg" && timeout 25 env -i PATH=/usr/bin HOME="$T/xdg" XDG_RUNTIME_DIR="$T/xdg" \
    XDG_CONFIG_HOME="$T/xdg" XDG_CACHE_HOME="$T/xdg" XDG_STATE_HOME="$T/xdg" \
    XDG_DATA_HOME="$T/xdg" LANG=C.UTF-8 QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 \
    quickshell -p "$T/shell.qml"
  ```

  A `quickshell.ipc` socket error can appear when `$T` is a long path; it does not affect
  the harness.

- An offscreen render of the panel, the form and the review was inspected once; markup in
  the artifact showed as literal text.

Not validated: the real `codepet rpc` (the binary built at the time had no `rpc` command),
a visible window, a live Omarchy panel, keyboard navigation, focus, screen readers, X11 or
Wayland sessions, and resource use.

## Manual checklist (target host)

- Preview: the banner is visible; every fixture renders; the console shows one logged envelope
  per action; meters never change.
- With `codepet rpc`: the panel shows the Rust meters (70.0) and five providers with only Mock
  enabled.
- Blank or oversized (more than 16 KiB) fields disable preflight; Start stays disabled until a
  fresh preflight and acknowledgement; any edit disables it again.
- Start once: the state moves only after Refresh; nothing starts from opening the panel,
  focus or a timer.
- Review shows SIMULATED and NOT VERIFIED or FAILED; Accept needs acknowledgement and a
  reason of at most 512 bytes; one +20 appears only in the category's meter after refresh; a second Accept
  reports the earlier receipt with no new reward.
- Discard, failure and cancellation grant nothing; "Check interrupted tasks" resumes nothing.
- A `recovery_required` task shows Resolve recovery; Abandon needs the acknowledgement, grants
  nothing and frees the slot.
- Keyboard-only use, visible focus, small-window scrolling, screen-reader names.
