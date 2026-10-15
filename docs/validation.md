# v0.2.0 validation: release branch after review fixes, greyarch, 2026-10-15

Branch `release/v0.2.0` with WP-5 (pet art) merged, the confirmed review findings fixed
in separate `Fix review finding:` commits, the WP-5 art QA fixes merged (re-extracted
frames) and the WP-INT cleanups (death-pending copy, review-digest hint, CI art checks,
docs). The gates below were rerun after those last changes. Local workstation (Arch Linux, kernel
7.2.5-3-omarchy, AMD Ryzen 5 3600): rustc/cargo 1.98.1, PySide6 6.11.2 / Qt 6.11.2,
Quickshell 0.3.1, Node 26.8.1. Cargo ran one build at a time in a 4 GiB memory-capped scope
with `-j 2 --offline`.

## Automated gates (this run)

- `cargo fmt --all --check`: passed
- `cargo test --workspace --locked`: 338 passed, 0 failed, 0 ignored (codepet unit 5, care 10,
  cli 10, crash 9, lifecycle 10, presence 6, rpc 17; codepet-core 54; codepet-protocol 55;
  codepet-storage 162)
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed
- `codepet mock-demo` and `codepet --version` (`codepet 0.2.0`): passed
- MSRV: the same fmt, test (338 passed), strict clippy, `mock-demo` and `--version` with
  `cargo +1.85.0` in a separate target directory: passed
- Offscreen QML smokes (`qml/tests/run_qml.py`), all 0 failures: CareFlowSmoke 42 checks,
  CareSmoke 34, ChooserSmoke 44, FilteredDraftSmoke 8, LifeSmoke 239, OverlaySmoke 107,
  PetSpriteSmoke 21, PreviewSmoke 193, TimelineSmoke 11
- `test_overlay_windows.py` (7 checks), `test_quickshell_host.py` (38 tests, plus the
  real-RPC harness with `CODEPET_TEST_BIN`), `test_bar_plugin.py` (27 tests, plus
  `omarchy plugin validate` on a staged copy), `test_sprite_assets.py` (36 legacy SVGs),
  `test_pet_assets.py` (15 of 15 packs, strict), `test_pet_assets_invalid.py` (10 broken-pack
  cases rejected), `test_pet_distinctness.py` (max cross-species idle IoU 0.753, capybara/owl),
  `test_install_scripts.py` (11 tests; temporary HOME with stub tools), `copy_lint.py`
  (0 findings), `test_ci_matrix.py`, `test_release_docs.py`: passed. Skipped here only:
  `shellcheck` (not installed).
- `node --test qml/tests/*.mjs`: 136 passed
- `python3 -I docs/art-library/verify.py` (15 source PNGs) and
  `python3 -I docs/art-library/test_verify.py` (22 tests): passed
- Qt 6 `qmllint -I qml` on `qml/shared`, `qml/standalone` and every `*Smoke.qml`: no output
- `python3 tools/petart/extract.py --check` (needs the source sheets; local only): the
  committed frames are byte-identical to a fresh extraction

Offscreen and isolated-Quickshell runs prove QML loading and the request/file contracts.
They are not Wayland, compositor, input, focus or resource validation.

## Contact-sheet review (spec §17.4 row 5)

Contact sheets come from `tools/petart/extract.py --contact` (light and dark backgrounds).
Scale, direction and playback previews come from `tools/petart/extract.py --previews`:
`<id>-scales.webp` shows each stage at 64, 96 and 112 px at 1x, 1.25x, 1.5x and 2x, facing
right and mirrored left, and `<id>-s<stage>-playback.webp` is an animated WebP stepping
through every state at its frame time in both directions. Both land in the run directory
(`~/Work/runs/codepet--2026-10-15--v0.2/previews`, generated for all 15 species in this
pass), not in Git. No human review is recorded in this run; each wave and column stays
**untested** until someone checks it and records the result here.

| Wave | Species | gait | care poses | stage readability | fish in water | not recolors | 64/96/112 px at 1x–2x | both directions | playback | Result |
|---|---|---|---|---|---|---|---|---|---|---|
| Wave 1 | cat, dog, capybara, penguin, robot | – | – | – | n/a | – | untested | untested | untested | untested |
| Wave 2 | iguana, spider, snake, goldfish, rabbit | – | – | – | – | – | untested | untested | untested | untested |
| Wave 3 | turtle, frog, owl, red_panda, axolotl | – | – | – | – | – | untested | untested | untested | untested |

The goldfish `sleep_enter` frame was corrected after these sheets were first drawn (c09 is
now rendered from the open-eyed idle pose); Wave 2 reviewers should check it in the
playback preview.

## Live QA on greyarch (spec §17.9)

Not run in this pass (the release step installs and runs it). Each item is pass, fail or
untested; nothing below has been observed on the live desktop yet.

| # | Check | Result |
|---|---|---|
| 1 | install.sh: plugin listed in `right`, no plugin QML errors in the shell log, bar shows Ginger with ◦ | untested |
| 2 | popup care commits and the badge updates via bar-state.json; zero idle execs over 10 minutes (strace); right click starts exactly one overlay; click-through and no focus theft | untested |
| 3 | separate test state home: presence ticks, stops on idle/lock/fullscreen/pause; warning → critical → death → memorial → adoption (playtest); stateHome restored to "" | untested |
| 4 | evolution through accepted mock tasks: one celebration only, none after restart | untested |
| 5 | all 15 looks at every stage at bar and overlay sizes (only while the operator is hands-off) | untested |
| 6 | `tools/measure-overlay.sh`: PSS, CPU and wakeups for the overlay `qs` and the shell, plugin off and on, idle and active, smallest and largest pack; first-switch latency (not measured by the tool) | untested |
| 7 | monitor hotplug and fractional scale | untested |

---

# Integration validation: PRs #1-#4 plus review fixes, greyarch, 2026-10-15

Branch `integrate/v0.2`, based on `origin/main` `6a5f654`. PRs merged in order with real
merge commits: #1 core settings preflight, #2 review-metadata recovery, #3 care/sleep/demo
progression, #4 transparent desktop overlay. The #2/#3 combination merged cleanly as text
but did not compile (`now` → `at.wall` in `recover()`) and #2's abandonment test expected the
old meter model; both were resolved inside the #3 merge commit. Twelve confirmed review
findings were then fixed in separate `Fix review finding:` commits (schema is now v3).

Local workstation (Arch Linux, kernel 7.2.5-3-omarchy): rustc/cargo 1.98.1, PySide6 6.11.2
/ Qt 6.11.2 (`/usr/lib/qt6/bin/qmllint`), Quickshell 0.3.1, Node 26.8.1. Cargo ran in a
3 GiB memory-capped scope with `-j 2 --offline`.

- `cargo fmt --all --check`: passed
- `cargo test --workspace --locked`: 184 passed, 0 failed, 0 ignored (codepet unit 2,
  care 10, cli 7, crash 5, rpc 13; codepet-core 29; codepet-protocol 40;
  codepet-storage 78)
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed
- `cargo run --locked -p codepet -- mock-demo`: passed
- QML offscreen (`QT_QPA_PLATFORM=offscreen`, commands from `.github/workflows/qml.yml`):
  PreviewSmoke 41, CareSmoke 26, CareFlowSmoke 10, FilteredDraftSmoke 3, OverlaySmoke 19,
  TimelineSmoke 11 checks, all passed; overlay window lifecycle 7 checks; 36 sprite/prop
  SVG assets; `test_quickshell_host.py` 8 tests passed, including the two real Quickshell
  load smokes (the real `qml/shell.qml` reaches `No PanelWindow backend loaded`; a staged
  copy with a FloatingWindow in place of the layer-shell PanelWindow loads and answers
  `qs ipc call codepet ...`)
- Qt 6 qmllint on `qml/shared`, `qml/standalone` and every `*Smoke.qml`: no output.
  On `qml/shell.qml` it reports only `Type PanelWindow is not creatable`, which the real
  load contradicts (Quickshell creates it when a layer-shell backend exists).

Not run here: hosted CI (pinned PySide6 6.8.3), live Omarchy/Wayland input pass-through,
focus, hotplug, lock/suspend/fullscreen integration and resource measurements. The
Quickshell load smokes run offscreen without a compositor and are not desktop validation.

---

# Care/progression validation — cloud workspace, 2026-10-15

This revision was implemented and tested only in the cloud workspace, based directly on
remote main `6a5f654`. Independent bugfix PRs #1 and #2 are not included or merged.
Real providers remain disabled; no user-computer files or project directories were changed.

## Rust gates

Official Rust 1.85.0 (declared minimum) and 1.99.0 (stable), existing locked dependency cache:

- `cargo fmt --all --check`: passed on both toolchains
- `cargo test --workspace --locked`: 174 tests passed on both toolchains, unfiltered
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed on both
- `cargo run --locked -p codepet -- mock-demo`: reaches review, NOT VERIFIED, no acceptance,
  no provider launch and no project changes (the older in-memory demo remains distinct)
- `git diff --check`: passed

The cancellation regression retains its no-child-process assertion on cloud kernels lacking
`/proc/<pid>/task/<tid>/children`, using a read-only PPid scan fallback. No test was skipped.

New coverage includes all-four-full Start gating, transactional two-process races and debit,
explicit care/acceptance idempotence, bathroom requirements, pre-execution refunds, mandatory
and voluntary naps, early wake, closed-app completion, backward/forward clocks, monotonic
settlement, review during sleep, migration preservation, isolated demo XP/levels/mastery,
no provider permission expansion, and redaction before persistence/digesting. An independent
review also checked stale preflight and saved-filtered-draft UI boundaries and care sequence
interleavings; fixes received regression coverage.

## Qt offscreen gates

Qt/PySide6 6.8.3 installed from official PyPI into an isolated temporary cloud directory:

- PreviewSmoke: 34 checks passed
- CareSmoke: 26 checks passed
- CareFlowSmoke: 10 checks passed (including concurrent busy-host interruption)
- FilteredDraftSmoke: 3 checks passed
- Qt6 qmllint: no output for shared/standalone components and all four smoke tests
- Standalone 720×900 offscreen render inspected

See [qml/README.md](../qml/README.md) for exact commands and scope. The existing Quickshell
adapter was not changed; its real-RPC harness was updated but could not run because
Quickshell is not installed here. Offscreen rendering does not establish live Omarchy,
Wayland/X11, screen-reader or keyboard usability, packaging, resource budgets or performance.

Hosted CI status is reported on the pull request, not inferred from local passes.

---

The previous validation records below are retained as history and describe earlier revisions.

# Validation

## Milestone 1 on greyarch, 2026-10-14 (after the review fixes)

Local workstation (Arch Linux, kernel 7.2.5-3-omarchy, x86_64). This is not an Omarchy,
Quickshell or desktop runtime validation.

### Platform versions recorded (handoff step 5)

Omarchy 4.0.4, Quickshell 0.3.1, Qt 6.11.2 (qt6-base, qt6-declarative), bubblewrap 0.12.0,
system SQLite 3.53.4 (unused; the build bundles 3.51.3), kernel 7.2.5-3-omarchy, Hyprland
session. These are recorded, not certified: no containment or desktop integration was tested.
Upstream Omagotchi/OmaPets commits are not pinned yet and no upstream code or assets are used.

### Toolchain

- rustc 1.98.1 (48a229cea 2026-09-01), cargo 1.98.1, clippy 0.1.98, rustfmt 1.9.0 (the
  default `stable` rustup toolchain on this host)
- GCC 16.2.1 (builds the bundled SQLite 3.51.3)
- Qt 6.11.2 (`/usr/lib/qt6/bin/qmllint`, `qml6`), Quickshell 0.3.1
- Every cargo command ran offline (`CARGO_NET_OFFLINE=true`) with 2 build jobs inside a
  `systemd-run --user --scope` with `MemoryMax=3G`, against the local registry cache.

**Rust 1.85.0, the declared minimum and the CI pin, was not run here.** That toolchain is not
installed on this host and none was installed for this run. The dependencies' declared
`rust-version` fields are all 1.85 or lower (rusqlite and libsqlite3-sys declare none), but
the MSRV is unverified until CI or a 1.85.0 toolchain runs the gates. No hosted CI run is
claimed.

### Gates

- `cargo fmt --all --check`: passed
- `cargo test --workspace --locked`: **143 passed, 0 failed** (core 24, protocol 39,
  storage 54, CLI unit 2, CLI integration: `cli` 7, `crash` 5, `rpc` 12; no doctests)
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed
- `cargo run -p codepet --locked -- mock-demo`: reached ReviewReady, NOT VERIFIED, mental
  health 70/100, no acceptance, no files touched
- `qmllint -I qml` on all 9 QML files under `qml/shared`, `qml/standalone`, `qml/omarchy` and
  `qml/tests/PreviewSmoke.qml`, plus the staged Quickshell harness: no output. This qmllint
  exits 0 even with warnings; a deliberately broken file confirmed that it does report them.
- `qml6 qml/tests/PreviewSmoke.qml` (offscreen, under `timeout`): 34 checks passed.
  `qml6 qml/standalone/Main.qml` (offscreen, stopped by `timeout`) loaded with no QML errors.
- Quickshell harness against the test stub `fake_rpc.py` (headless, under `timeout`):
  15 checks passed.
- Quickshell harness `qml/tests/quickshell/real_rpc.qml` against the **real** debug
  `codepet rpc` binary (headless Quickshell 0.3.1, `env -i`, offscreen, isolated
  `XDG_STATE_HOME`/`HOME`, under `timeout`): 9 checks passed. It covers fresh idle state,
  create, preflight, explicit start, the mock supervisor reaching `review_ready` NOT VERIFIED,
  one acceptance reward, same-request-id replay and the resulting Rust-owned snapshot
  (mental health 9000, slot free). The project folder stayed empty; the state directory was
  0700 and the database 0600. Quickshell logged its usual IPC-socket error for the long
  scratch path; it does not affect the harness.

All tests use temporary `XDG_STATE_HOME` and `HOME` directories; none touch the real state
directory. Integration tests run the built `codepet` binary, one process per request.

### Review findings reproduced and fixed

Each was first reproduced against the debug binary in temporary directories, then fixed with
a regression test:

- A valid draft could hold the active slot forever because every response carrying its
  review digest exceeded 64 KiB. Lists now carry task briefs, drafts are capped at 32 KiB
  of JSON and artifacts at 24 KiB; tests prove maximal tasks can be read, reviewed and
  discarded, and every response fits.
- A 65,530-byte `task.create` committed a draft and answered `OUTPUT_LIMIT`; it is now
  refused with `INVALID_PARAMS` before anything is stored.
- A tampered artifact could be accepted with the original digest and rewarded. Review and
  acceptance now recompute the digest and refuse with `REVIEW_CHANGED`.
- A 600-byte (200-character CJK) override reason got `OVERRIDE_REQUIRED`; it is now
  `INVALID_PARAMS`, with one 512-byte limit in core, protocol, storage and QML.
- A folder under a symlink into the state directory passed preflight; with `HOME` unset the
  home directory passed preflight. Both are refused now, and start repeats the folder check.
- A backward clock step extended a preflight token; history paging skipped tasks created in
  the same millisecond; the UI had no way to resolve `recovery_required`. All fixed and
  tested.

### Not run / not established

- Rust 1.85.0 (see above) and GitHub Actions
- A live Omarchy panel or bar widget, any visible
  window, keyboard or screen-reader use, X11 or Wayland sessions, other distributions
- Real providers, staging, sandboxing, apply/rollback (not implemented)
- CPU, memory or binary-size measurements
- Fuzzing; tests are example-based

---

The record below is the earlier scaffold validation, kept unchanged as history. It predates
milestone 1 and describes the dependency-free scaffold.

# Scaffold validation

Verified in the Linux cloud build workspace on 2026-10-14. This is not Omarchy/Qt runtime validation.

## Passed

Both official Rust toolchains were installed locally outside the repository, without changing the host shell profile:

- rustc 1.99.0 (b940084d7 2026-09-28)
- rustc 1.85.0 (4d91de4e4 2025-02-17), matching the declared minimum and CI

On **both** toolchains:

- `cargo fmt --all --check`: passed
- `cargo test --workspace --locked`: **24 passed, 0 failed** (21 core unit tests, 3 CLI integration tests; no doctests)
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed
- Explicit `mock-demo`: reached ReviewReady, retained mental health at 70, stated NOT VERIFIED and exited without acceptance

The `providers` smoke check lists Mock available and all four real providers unsupported. CLI tests additionally prove no-argument invocation only shows help and the `codex` command is rejected.

Core tests cover unchanged startup, stale/replaced/cross-engine/cross-job tokens, unsupported providers, single active slot, waiting, review, cancellation acknowledgement, duplicate acceptance, immutable review identity, override reasons, category-only reward and clamp, failure/discard behavior, restart interruption, preserved receipt semantics, stale token recovery, bounded history/events/frames, resync, version/UTF-8 checks, lexical paths, settings, and invalid transitions.

## Not run / not established

- QML lint/runtime: `qmllint` wrapper resolves to missing `/usr/lib/qt5/bin/qmllint`; no usable Qt 6 runtime/linter found. QML received static source review only.
- No Omarchy, Quickshell, X11, Wayland or distribution compatibility test
- No UI screenshots, rendering, accessibility or interaction test on a live host
- No connected Rust/QML end-to-end test; these remain separate prototypes
- No production provider, filesystem isolation/apply, real persistence/crash or cancellation test
- No CPU/RSS/binary-size measurements; resource budgets remain design targets
- GitHub Actions workflow is prepared, but no hosted CI run is claimed

The spec copy was compared byte-for-byte with the latest source specification at this milestone. This report describes this scaffold, not the full target specification's acceptance matrix.

## Overlay-first UI revision (2026-10-15)

Based on care PR #3 head `1f18e5e78c2f316804eb5205b95071da4160e3dd`;
no Rust, protocol, storage or provider code changed in this revision.

Fresh validation in the cloud workspace (not inherited earlier results):

- Official Rust 1.85.0: `cargo fmt --all --check` passed
- `cargo test --workspace --locked`: all 174 tests passed, unfiltered
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed
- `cargo run --locked -p codepet -- mock-demo`: passed, no commands/projects touched
- PySide/Qt 6.8.3 offscreen: existing Preview/Care/CareFlow/FilteredDraft smoke
  suites passed (73 checks); OverlaySmoke passed 18 checks
- Independent task-window lifecycle test passed 7 checks
- Six host source/pure-JavaScript policy tests passed (monitor selection, clipping,
  focus/exclusive-zone/mask declarations, explicit suppression/restore and lifetime)
- All 36 original SVG pose/prop assets parsed successfully; passive renderer check passed
- Portable QML qmllint clean; `git diff --check` passed
- Offscreen idle/menu renders visually inspected; original sprite/prop contact
  sheets and actual QML renderer inspected

New CI workflow runs pinned Qt 6.8.3 offscreen suites, lifecycle/policy/assets and
portable QML lint. These tests do not import a real Quickshell runtime. Host APIs
were source-reviewed against official 0.3.1 docs/source, but the entry was **never
loaded** by Quickshell in this run; a later real load showed it could not load at all
(its `../shared` import was outside the Quickshell config folder). That was fixed during
integration by moving the entry to `qml/shell.qml`; see the integration record at the top
of this file. Quickshell runtime,
compositor input pass-through/focus, monitor hotplug, trusted lock/suspend/fullscreen
hooks, accessibility integration and CPU/PSS/power measurements were **not run**.
No live desktop compatibility claim follows from offscreen tests. See
[the explicit live checklist](desktop-overlay.md#live-validation-checklist-not-performed-in-the-cloud).
