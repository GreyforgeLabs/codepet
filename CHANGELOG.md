# Changelog

All notable changes to CodePet.

## 0.6.0 — 2026-10-20

- **A native desktop pet.** The pet on your desktop is now `codepet-pet`, a small Rust
  program drawn directly through the Wayland layer shell. Measured over one minute on
  the development desktop while it roamed, it used 3.9 MB of memory (PSS), 0.08 % of
  one core and about five wakeups a second (under one while resting), where the 0.5
  Quickshell pet used 38 to 168 MB, 0.25 to 0.3 % of a core and 26 to 60 wakeups a
  second. It sleeps until something changes:
  no polling, one timer at a time, and the pictures of only the current pet.
- Same behaviour as before: roaming, drag, Pin/Roam, the care wheel, care feedback,
  celebrations, observed-client bubbles, the work badge, placement memory, Calm mode,
  Off and "Pet on my desktop". Its rules are a Rust port of the QML ones, checked
  against them on 1,920 cases. A click opens the compact card window. See
  [desktop controls](docs/desktop-overlay.md).
- **HIDE.** Hovering the pet offers a **HIDE** pill. A click hides it and a toast names
  the show/hide key. Super + Alt + P (unused by Omarchy's defaults) is the suggested
  binding for `codepet-overlay toggle`; the README has the one line.
- **Clearer care icons.** Food is a meat on a bone, Drink a water droplet, Bath a cluster
  of bubbles, Pet a hand and Pin a push pin, in the wheel, Home and the bar badges.
- **Softer sounds.** Care plays one short, gentle sound per action (mallet notes, a
  water drop, bubble pops); the synthesized animal voices are gone.
- Home's care bubble is smaller and sits below the pet's head.
- Helpers: a row that says Tap to connect does it on that tap and opens the sheet.
  An empty Grok hook file is filled; a file that is not CodePet's is left alone.
  A Grok dispatch wrapper reports the real binary's version.
- `CODEPET_PET_ENGINE=quickshell` keeps the previous Quickshell desktop pet as a
  fallback. The bar plugin and Work window remain Qt Quick.
- The bar plugin reads Omarchy's theme through a qualified `qs.Commons` import, so its
  popup keeps its colours and sizes under Quickshell 0.3.2 / Omarchy 4.0.6, where a
  plain import resolves `Color` to another type.

## 0.5.0 — 2026-10-18

- Adds an explicit Gentle care tempo: Lively uses a five-minute full hold and
  noticeable attended-time care needs. Existing lives keep Steady until changed;
  new Gentle starts use Lively. Switching preserves progress and critical clocks.
- Schema 6 adds a per-life tempo while retaining frozen lifecycle-v1 rules, stored
  leniency and all historical data. Older binaries refuse the newer schema.
- Delayed observer events cannot replace a newer coding turn or approval state;
  matching late tools still count toward their original turn.
- Adds **Updates** directly to the desktop pet's compact card, and a right-click
  care wheel for Pet, Snack, Drink, Play, Nap/Wake and Pin/Roam. Pinning retains
  occasional expressions; petting is cosmetic.
- Need cues survive work status, attended minute checkpoints refresh settled care,
  and repeated Play restarts its reaction. Treats shows the finite jar count and
  confirms the actual grant; growth feedback follows saved evolution.
- The compact card and Work window share client selection. Each job stays bound to
  its selected adapter; changing the next client never retries or launches work.
  Codex, Claude Code and OpenCode have separate execution and verification limits.
- The work controller is split into filesystem, review and runtime modules while
  keeping private prompts, paths, reports and errors out of display exports.
  Bounded stream completion and nonblocking private-file reads retain the earlier
  delayed-completion and unsafe-read corrections.
- Recovery status checks preserve failed Apply feedback and stop fast polling when
  an interrupted application is waiting for a reviewed retry.
- Source preparation refuses repositories overlapping CodePet's private state in
  either direction, before any provider can receive a staged copy. Source hardlinks
  are rejected during traversal and again on the opened file before copying.
- Final provider output waits for actual stream completion within a five-second
  drain budget; a stuck stream fails explicitly instead of losing a late export.
- Sprite decode size uses Qt's supported screen pixel ratio with a finite fallback,
  including fractional scaling.
- See [lifecycle rules](docs/lifecycle.md),
  [provider boundaries](docs/providers.md) and [candidate qualification](docs/validation.md).

## 0.4.0 — 2026-10-17

CodePet can find a suspected bug in your local repositories, prepare a repair when
you choose **Fix**, and grow when you review and apply useful work.

### A pet that codes

- **Updates in the app:** More → Updates checks the latest stable GitHub release.
  One Update button verifies and installs it in a bounded independent job, preserving
  pet progress and installer recovery backups. Active work/review and open Work
  windows block installation; interrupted updates remain visible. See
  [updates and recovery](docs/updates.md).
- The desktop icon opens a compact card: suspected bug, Fix, progress, Stop and
  Review. Research and custom coding tasks remain available in the advanced window.
- Automatic discovery searches conventional local development folders. Read-only
  scouts use isolated source copies, return at most one provider-reported suspected
  bug, and never fix or apply code automatically. Checks are enabled after Let's go!
  when the pet is ready; **Details → Checks off** pauses future checks.
- One scout attempt per six hours globally and one per repo per day, including
  failed preparation. A bounded preflight skips unsuitable checkouts before a single
  provider job. Scouts run for at most five minutes and earn no XP or care.
- Accepted bug fixes fill food, research fills water, and builds lift mood. Each
  accepted job earns 20 XP exactly once. Play and free recovery care remain available;
  happiness, sleep and energy gate new jobs without requiring full food/water bars.
  Three accepted jobs or 45 minutes of accepted work call for a two-minute nap.
- Codex 0.163.0 uses an existing ChatGPT subscription sign-in. Selected source context
  can be sent to the provider and the job can access the network. Other coding-app
  connections continue to observe externally started sessions independently.

### Visible progress and desktop controls

- Home shows four care percentages and bars, exact XP, level and growth progress.
  The pet card shows level, XP and age from the current life's saved birth timestamp.
- Drag the desktop pet in either direction, pin it from its context menu or More,
  and retain its position and monitor across restarts. Hide/show preserves placement
  and pinning. Calm mode remains available.
- Preparation failures remain visible after refresh or reopening. File-watch refresh
  no longer forms a request loop. Interrupted application reopens Review so the
  existing recovery path remains reachable.

### Containment and compatibility

- One job owns the slot through review. The provider sees a bounded isolated copy,
  with process-tree cancellation/deadlines, memory/process limits, credential read
  restrictions and no automatic escalation. Missing containment refuses work.
- Explicit Apply checks the reviewed digest and original bytes, refuses conflicts,
  preserves recovery data, and binds its durable reward to the original companion.
  Provider-reported checks require acknowledgement; they are not presented as
  independent verification. Failed/cancelled jobs earn nothing.
- Review export supports small edits to larger text files within a 384 KiB combined
  text budget, retaining the 512 KiB encoded review limit. Export failures identify
  CodePet's limit instead of blaming the provider; no changed files are dropped.
- Existing companions, XP, age, life history and observer connections are preserved.
  SQLite remains schema 5. The retired mock pipeline stays behind non-default
  `legacy-tasks` for compatibility; real work does not depend on it.

See [work and limits](docs/work.md), [provider requirements](docs/providers.md),
[security](docs/security.md) and [qualification](docs/validation.md).
Actual compositor pointer/focus behavior for the new card remains unqualified;
offscreen tests and actual hide/show checks are recorded separately.

## 0.3.0 — release candidate

CodePet now keeps company with your real coding apps. The old mock task window is
replaced by one simple popup with painted care buttons and an obvious power switch.

- Pick a pet and tap Let's go! Gentle mode is the default; existing pets and their
  growth survive. Hardcore lives in Help behind a typed-name acknowledgement.
- Connect Claude Code, Grok CLI, OpenCode and Hermes; Codex has a guided review of
  its hook entries. CodePet observes only and stores no chats, code or paths.
- Qualifying finished work earns bounded treats. Tap Treats to give one and help
  the pet grow. Failed, cancelled and test work never earns.
- Off freezes the pet, stops recording presence and exits the desktop pet. Hooks
  short-circuit before CodePet starts; Codex still starts its own login shell.
- Home, Helpers, More, Memories and the desktop pet share the same state priority.
  Memories includes the saved life journal with pages for older entries.
- Crash-safe client jobs, nonblocking hooks, bounded inbox/fold processing,
  metadata-only diagnostics, migration backups and uninstall tombstones.
- The default binary no longer routes task methods. Historical tables remain;
  the mock pipeline is behind non-default `legacy-tasks` (still retained for compatibility in 0.4.0).

Qualification is in [validation](docs/validation.md). See
[architecture](docs/architecture.md), [security](docs/security.md),
[lifecycle](docs/lifecycle.md) and [bar installation](docs/omarchy-plugin.md).


## 0.2.1 - 2026-10-15

Prepared release candidate, superseded by v0.3.0 before publication. No change to
the lifecycle rules, the companions' art or the storage schema.

### Install and release packaging
- The release archive installs as shipped: `packaging/omarchy/install.sh` reads the
  archive's `VERSION` file and installs its `bin/codepet` instead of failing on the missing
  `Cargo.toml` or trying to build. README covers requirements, a quick start, installing
  from the archive, the first run and a full uninstall.
- `tools/release/package.sh` builds the archive from an explicit file list, with no
  build-machine paths in the binary (path remapping, stripped symbols), owner 0:0 entries,
  tag-pinned links for files it does not ship, and release notes with absolute links. It
  gates on a dry run, an install and an uninstall of the extracted archive in a throwaway
  HOME. `tools/release/art_sources.py` builds `codepet-art-sources-v0.2.tar` from cleaned provenance files; the preserved
  archive accompanies the first public v0.3.0 release.
- [THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md) carries the licence texts of every
  crate linked into `codepet`, plus SQLite's public-domain dedication, and ships in the
  archive; CI checks it against `Cargo.lock`.
- The installer checks for Omarchy's plugin CLI before installing anything, warns when Qt's
  WebP image plugin is missing, stages the bar plugin on the same filesystem as its
  destination so each swap is one rename, keeps only the current and previous release
  trees, prints next steps after a first install and a pasteable shell-restart line.
- `codepet-overlay quit` stops the desktop pet. It is always refused while a simulation
  task runs. When the overlay cannot say whether one does (an overlay from 0.2.0) it is
  refused unless `--force` is given, and the installer's update advice says so.
  `uninstall.sh` names it, and warns and continues when the bar plugin cannot be disabled.

### Safety
- The Adopt, Retire and Keep buttons show the companion's name as plain text, like every
  other label.
- The bar ends a `codepet rpc` child whose answer it refused, and starts no new request
  until it has exited. The `binary` setting now reaches the desktop pet too, and right or
  middle click open the card when the desktop-pet launcher is missing.

### Documentation
- Public wording throughout: no private-prerelease status, internal paths or host names;
  the plugin README's glyph table matches the bar ([docs/omarchy-plugin.md](docs/omarchy-plugin.md));
  architecture, licence and validation notes are current.
- Pet-art provenance names the generation route neutrally and points at the published
  source sheets; `tools/petart/extract.py` defaults to `art-src/v0.2` and
  `target/petart-run` inside the checkout.
- Cargo metadata (description, repository, `publish = false`) and the plugin manifest's
  homepage.

### Fixed
- Running the test suite no longer floods the desktop with "Process crashed: codepet"
  alerts. Debug-build fault injection (`CODEPET_FAULT`) now marks the process
  non-dumpable before it aborts, so systemd-coredump never records the deliberate abort
  as a crash. `ulimit -c 0` alone suppressed only the core file. Release builds are
  unchanged.

## 0.2.0 - 2026-10-15 (testing prerelease)

Closes issues #5 (15 companions with evolution) and #6 (lifecycle, evolution and
permadeath). Decisions: [docs/decisions/0001-v0.2-lifecycle.md](docs/decisions/0001-v0.2-lifecycle.md).
Issue #5 is closed with one recorded departure: the companions ship as raster WebP frames
extracted from AI-generated sheets, not as SVG generators
([docs/decisions/0002-v0.2-raster-pet-art.md](docs/decisions/0002-v0.2-raster-pet-art.md)).

### Pull requests included
- #1 Invalidate core preflight approvals after settings updates.
- #2 Unblock explicit abandonment of damaged saved reviews (review-metadata recovery).
- #3 Interactive care, mandatory naps and isolated demo progression; acceptance no longer
  refills care.
- #4 Transparent desktop pet overlay replacing the garden UI.
- #7 Generated source-art library and provenance catalog (`docs/art-library/`), with its
  review findings fixed: `verify.py` uses explicit checks that also fail under `-O`, plus
  bit-depth, grid, path, inventory, C2PA and alpha-254 checks and a licence statement.

### Companions and evolution (#5)
- 15 companions, each with three stages (young, grown at level 2, mature at level 4) and
  15 animated states, shipped as WebP frames extracted from the GPT Image 2 art sheets by
  `tools/petart` ([docs/pets.md](docs/pets.md), [PROVENANCE](qml/assets/pets/PROVENANCE.md)).
- Looks are cosmetic: switching never changes needs, rest, level, XP or countdowns.
- Evolution follows the life's accepted-task XP; each stage is journaled once and
  celebrated once (by the UI that accepted, or on the next click elsewhere).

### Lifecycle (#6)
- Per-life permadeath (preselected) or gentle mode, armed only after the rules are
  acknowledged; the migrated pet starts unarmed, so an upgrade never costs a pet.
- Needs change only during attended time recorded by the bar plugin and settled by Rust
  (CLOCK_BOOTTIME-capped, no offline catch-up), never while paused, napping or working.
- Critical periods with a frozen grace window; retirement by typed name; memorial; explicit
  level-1 adoption. Death never touches tasks, artifacts, receipts, settings or projects.
- Storage schema v4 (additive lifecycle tables), migration with a pre-upgrade backup.

### Omarchy integration
- Bar plugin `greyforge.codepet` (read-mostly bar-state.json, glyph badges, popup care,
  presence recording) and the separate desktop overlay with Care, Tasks, Companion and
  Life tabs.
- `packaging/omarchy/install.sh` / `uninstall.sh` and `tools/measure-overlay.sh`.
- `codepet --version`; the CLI republishes `bar-state.json` after every successful mutating
  request and reports a failed export on one stderr line without changing the response.

### Fixes and polish
- A companion whose grace has run out while a task is open is described the same way in
  the bar and the life panel: it will pass away once the current task is finished.
- The review screen keeps Accept, Discard and Abandon disabled for a saved review digest
  outside the protocol's 1 to 128 bytes and points to "Check interrupted tasks"; an
  `INVALID_PARAMS` refusal shows the same hint.
- The life UI's built-in roster now uses the registry's stage names for owl, red panda and
  axolotl, so the overlay and the bar name stages alike.
- CI runs the source-art verifier and its regression test.

### Known limits
- Every companion plays its own drawn poses through one shared state timing table (same
  frame order, frame time and mode); per-species idle and care timing, asked for in #5, is
  deferred ([docs/pets.md](docs/pets.md), Limits).
- Live desktop QA, resource measurements and the contact-sheet human review are recorded in
  [docs/validation.md](docs/validation.md); anything not yet run there is marked untested.
- Presence is not tamper-proof; neglect accrues only while the bar plugin runs on Omarchy.
