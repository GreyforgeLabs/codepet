# CodePet QML UI

Rust is the authority for care, readiness, sleep, acceptance and progression. QML
renders snapshots and sends deliberate requests. It never grants stats or XP,
completes sleep, or starts work from an animation, timer, care action or level.
Only the mock provider is available. The shared UI prominently labels simulation;
no commands run and no project files are read or changed by mock execution.

See [overlay capabilities and live validation](../docs/desktop-overlay.md) and
[Quickshell host controls](omarchy/README.md) for the primary desktop entry.

## Components

- `shared/Panel.qml`: care, meters, task controls, saved progression and recent results
- `shared/DesktopPet.qml`: transparent floor-local pet scene, hover meters, compact
  care menu, transient props and one bounded cosmetic scheduler
- `shared/PetSprite.qml`: passive original vector ginger-cat poses (see asset provenance)
- `shared/PetCare.qml`: explicit care controller and keyboard-accessible task-window
  controls, including interruptible care routine
- `shared/ProgressionPanel.qml`: isolated demo/real XP, levels, four mastery tracks
  and a textual level-up notice. Advanced guided templates remain previews only
- `shared/TaskForm.qml`: bounded draft, small guided simulation, providers,
  revision-bound preflight, explicit acknowledgement and Start
- `shared/TaskReview.qml`: literal-text artifacts, honest verification status,
  override reason, explicit acceptance/discard and saved reward receipts
- `shared/TaskTimeline.qml`: bounded, explicitly loaded durable task-state events
- `shared/WellbeingMeter.qml`, `shared/StatusBadge.qml`: textual values and states
- `standalone/Main.qml`, `standalone/PreviewHost.qml`: pure-QML fixed-fixture preview
- `omarchy/shell.qml`: transparent non-focusable layer-shell entry and independent
  explicit-open task/review window (live compositor validation outstanding)
- `standalone/OverlayPreview.qml`: fixture-only scene preview in a normal window
- `omarchy/HostAdapter.qml`: Quickshell transport, one `codepet rpc` process/request
- `tests/`: headless UI/transport harnesses, not product runtime code

Quickshell imports remain confined to the Omarchy adapter and its test harnesses.
The real standalone QProcess launcher is still not implemented. The optional
Python test runner is a test dependency, not a replacement product architecture.

## Host contract

Shared components receive a `host` object with:

- `snapshot`: last successful `state.get` result; only the host replaces it
- `busy`: an outstanding request; hosts refuse overlap with `CLIENT_BUSY`
- `lastError`: `CODE: message`, cleared on the next non-state request
- `request(method, params, callback[, requestId])`: a v1 response envelope to the
  callback; optional request IDs support identical acceptance/care retries
- Optional `supervisorActive`: used only for the explicit Refresh hint

Existing task/transport contracts are retained. `snapshot.care` provides readiness,
bathroom/rest counters and sleep status; `snapshot.progression` separates `demo`
and `real` lifetime XP, levels and mastery. See `docs/milestone-1.md` for RPC shapes.
Care mutations are `pet.care {action: food|water|play|wash|bathroom|routine}`,
`pet.sleep {}` and `pet.wake {}`. The UI care routine deliberately sends only needed
individual actions with a pause between confirmations; Stop or hiding cancels
remaining actions. The atomic backend `routine` is also available to other hosts.
Each confirmed care/sleep action is followed by an explicit state refresh.

The Start button and its handler require the authoritative `ready_to_work`, a
fresh preflight for this exact draft and care revision, and acknowledgement.
Any edit or care/rest revision change clears preflight and acknowledgement.
Start checks token expiry at click time and drops the token once clicked. Rust
rechecks readiness and consumes the token transactionally. Care changes while
already running do not cancel the task or prevent review/acceptance.

Saved instructions returned by Rust replace any locally entered text changed by
privacy filtering. The form visibly discloses the change before acknowledgement.
Folder/scope/instructions remain bounded (16 KiB instructions, 32 KiB combined
JSON), and override reasons remain bounded to 512 UTF-8 bytes. Untrusted artifacts,
events, summaries and labels are plain text. Preview logging includes only request
ID/method, never instructions, reasons or full payloads.

## Care, motion and sleep

- Four high-is-good bars: Nourishment, Hydration, Mood and Hygiene
- Food/water/play/wash fill their bar to 100; no care action grants XP
- Bathroom due makes the wash button say `Bathroom + wash`; the tree always offers
  the actual bathroom action. A plain wash cannot clear bathroom due in Rust
- Bed discloses a two-minute nap. Early wake preserves remaining rest; review and
  acceptance remain available while sleeping
- Countdown uses local display-only elapsed time at 1 Hz while visible/asleep.
  At zero it explicitly asks for Refresh/Wake to confirm saved readiness. It never
  issues RPC, persists ticks, wakes the pet, or enables Start
- Pose changes are capped at 5 FPS (200 ms), including short reactions. Hidden or
  minimized windows, reduced-motion and quiet mode stop pose ticks. Sleeping uses
  a static pose. No interpolated animation runs at an unbounded render-loop rate
- Text feedback is retained with reduced motion, plus a four-entry care trail in
  this view. The durable task timeline is separate. Sound is off
- UI options are session-local; this milestone does not persist reduced-motion or
  quiet-mode preferences or integrate compositor lock signals

The timeline uses `events.subscribe {after_seq, limit:50}` only on a user click.
It displays actual saved `task.state` transitions, validates event shapes, bounds
its list to 100 entries and marks omitted/malformed content. It does **not** invent
file reads, edits, tests, commands or completion percentages. Refresh is explicit;
there is no process polling and no live provider-operation stream in this milestone.

Acceptance displays the stored receipt, including isolated demo XP, real-work XP
and any historical care reward from older receipts. Small eligible mock artifacts
may grant 20 demo XP; failed checks grant none. No mock result earns real-work XP.
Levels never expand providers, scope or permissions. Higher-tier template execution
and richer unlocks remain deferred behind real-provider and containment gates.

## Fixed-fixture standalone preview

With a Qt6 runner: `qml6 qml/standalone/Main.qml`.

The banner says `PREVIEW – not connected to Rust; no rewards are stored`. Fixtures:
`idle`, `ready`, `bathroom_due`, `sleep_due`, `sleeping`, `demo_level_2`, `running`,
`review_not_verified`, `review_check_failed`, and `recovery_required`.

Preview snapshots never mutate in response to care, sleep, Start, acceptance,
discard, cancel or recovery: those requests return `PREVIEW_ONLY`. Draft/preflight
responses are canned for form inspection; the token is invalid outside preview.
The fixed event history is explicitly fixture data, not work that ran. Select
`ready` to inspect fresh-preflight/acknowledgement Start gating. Selecting a fixture
is not a real state mutation.

## Validation in the cloud workspace (2026-10-15)

Tested with **Qt 6.8.3 / PySide6 6.8.3**, offscreen Linux only. PySide6 was installed
from PyPI into `/tmp/codepet-qt`; it is not a project/runtime dependency. No packages
or launchers were installed on the user's computer. Exact commands:

```sh
python -m pip install --target /tmp/codepet-qt PySide6==6.8.3
mkdir -p /tmp/codepet-qt-cache
export PYTHONPATH=/tmp/codepet-qt
export XDG_CACHE_HOME=/tmp/codepet-qt-cache
export QT_QPA_PLATFORM=offscreen
python qml/tests/run_qml.py qml/tests/PreviewSmoke.qml
python qml/tests/run_qml.py qml/tests/CareSmoke.qml
python qml/tests/run_qml.py qml/tests/CareFlowSmoke.qml
python qml/tests/run_qml.py qml/tests/FilteredDraftSmoke.qml
/tmp/codepet-qt/bin/pyside6-qmllint \
  -I /tmp/codepet-qt/PySide6/Qt/qml -I qml \
  qml/shared/*.qml qml/standalone/*.qml qml/tests/*Smoke.qml
```

Results:

- `PreviewSmoke.qml`: 34 checks, 0 failures; retained draft, review, failed-check,
  override-byte-limit and recovery gates
- `CareSmoke.qml`: 26 checks, 0 failures; actionable care/readiness, sleeping/bathroom
  gates, stale-care preflight invalidation, no RPC from countdown, hidden/reduced/
  quiet animation pause, actual timeline events and isolated demo progression
- `CareFlowSmoke.qml`: 10 checks, 0 failures; confirmed sequential care, Stop between
  actions, skip full bars, stop when hidden/busy, no stranded routine and no task launch
- `FilteredDraftSmoke.qml`: 3 checks, 0 failures; saved filtered instructions are
  reviewed with visible disclosure, not stale unsanitized local text
- Qt6 `qmllint`: no output on shared/standalone components and these smoke tests
- Standalone offscreen rendering inspected at 720 × 900; this is not a live desktop
  or accessibility certification

Quickshell is not installed in this cloud test environment. Its existing transport
was left unchanged. `tests/quickshell/real_rpc.qml` was updated for mandatory care,
care debits and demo-only receipts but was **not rerun here**. Prior 2026-10-14
headless Quickshell 0.3.1 validation on greyarch is historical evidence only, not
certification of this care revision. The adapter still uses argv arrays (no shell),
64 KiB frame checks, one-shot response timeout and per-job supervisor lifetime.
Its known newline-free stdout buffering and shell-reload lifetime limits remain.

Not validated: live Omarchy/Quickshell, visible Wayland/X11 desktop, real keyboard
and screen-reader workflows, compositor lock signals, packaging, resource budgets
or performance claims. No real provider or host-project writing is enabled.

## Target-host manual checklist

1. Verify every preview fixture and prominent preview/simulation labels
2. With a real mock RPC host, care to 100 and observe persisted state after reopening
3. Start only after fresh exact-revision preflight and acknowledgement; changing care
   or draft must demand re-preflight, never auto-start
4. Bathroom due requires tree/bathroom action; ordinary wash leaves the badge due
5. Bed/wake and reopen preserve rest credit, show due state, and never start work
6. Review/accept while sleeping works; acceptance leaves care alone and grants at most
   one eligible demo reward. Failed checks remain failed and grant no XP
7. Hide/minimize/reduce motion, then inspect timers and CPU; measure rather than infer
8. Check keyboard focus, screen-reader labels, small-window scrolling and theme contrast
