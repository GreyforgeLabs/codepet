# CodePet

A manually controlled coding companion for **Omarchy by default, with Linux portability as
the goal**. Rust owns task policy, care, the companion's life and every reward; Qt
Quick/QML (Quickshell on Omarchy) only renders what Rust returns and sends explicit
requests. The only provider is a mock that runs no command and never touches a project
folder.

**Current release: [v0.2.1](https://github.com/GreyforgeLabs/codepet/releases/tag/v0.2.1),
the first public release (MIT)** (`codepet --version` prints `codepet 0.2.1`). See the
[changelog](CHANGELOG.md).

## What v0.2.0 adds

### Fifteen companions that grow with your work

Cat, dog, capybara, penguin, robot, iguana, spider, snake, goldfish, rabbit, turtle, frog,
owl, red panda and axolotl. Each has three stages (young at level 1, grown at level 2,
mature at level 4) and fifteen animated states: idle, walking or swimming, food, water,
play, wash, bathroom, falling asleep, sleep, waking, celebrate, working, waiting,
needs-care and error. Stage follows the
life's XP from accepted mock tasks; each growth is journaled once and celebrated once.
Changing the look is cosmetic: needs, rest, level, XP and countdowns stay as they are.
Roster, stage names and care labels: [docs/pets.md](docs/pets.md).

### A humane lifecycle, with permadeath or gentle mode

- Every life chooses **permadeath** (preselected) or **gentle** when its rules are
  acknowledged. Until then the clock is **unarmed**: nothing changes on its own, exactly as
  in v0.1. An upgraded pet starts unarmed, so updating never costs a pet.
- Needs change only during **attended time**: minutes the bar plugin observed you at the
  computer while the clock was running. Nothing counts offline, overnight, while paused,
  napping or while a task holds the slot.
- Only hunger and thirst are lethal. A lethal need that stays empty turns critical, and
  critical starts a grace window that is frozen when it begins. Matching care always
  recovers. In permadeath mode the life ends if grace runs out; in gentle mode the
  companion faints and work waits until you care for it.
- If grace runs out while a task is still open, the companion will pass away once that task
  is finished. The task and its result stay safe and can be reviewed, accepted, discarded
  or cancelled as usual.
- An ending is recorded in the memorial. A new companion is adopted explicitly and starts at
  level 1. Endings never change tasks, artifacts, receipts, settings or projects.
- Retirement is voluntary and needs the companion's name typed in. Per-life settings can only
  become more lenient.
- Two paces: **steady** (default; with every warning ignored, thirst ends a permadeath
  life after about 17 attended hours) and **playtest** (opt-in at adoption, badged
  everywhere; the same walk takes about 51 minutes).

Rules, numbers and copy: [docs/lifecycle.md](docs/lifecycle.md). Product decisions D1 to
D12: [docs/decisions/0001-v0.2-lifecycle.md](docs/decisions/0001-v0.2-lifecycle.md).

### Omarchy bar plugin `greyforge.codepet`

A read-mostly bar widget: the companion's thumbnail, name and a non-colour status glyph, a
tooltip, and a popup with care buttons. It renders the bar-safe `bar-state.json` that the
CLI writes, records attended presence, and sends requests only after an explicit click. It
never sends a `task.*` method. Details: [docs/omarchy-plugin.md](docs/omarchy-plugin.md).

### Desktop overlay

A small transparent pet on the desktop (Quickshell layer shell, click-through outside the
pet and its open menu, no keyboard focus). Hover shows the meters; a click opens compact
care controls. Task, review, Companion and Life tabs open in a separate focusable window
only after an explicit action. The bar and the user drive it through `codepet-overlay`
(`show|hide|toggle|tasks|care|companion|life|refresh|status`). Details:
[docs/desktop-overlay.md](docs/desktop-overlay.md) and [qml/omarchy/README.md](qml/omarchy/README.md).

## Install on Omarchy

```sh
packaging/omarchy/install.sh --dry-run   # list every staged file; changes nothing
packaging/omarchy/install.sh             # build, install, stage and enable the bar plugin
```

The installer builds `codepet` (or uses `CODEPET_BIN_SOURCE`), installs `codepet` and
`codepet-overlay` into `~/.local/bin`, copies the overlay runtime to
`~/.local/share/codepet/releases/<version>/` behind a `current` symlink, validates the
plugin with `omarchy plugin validate`, moves it into
`~/.config/omarchy/plugins/greyforge.codepet` in one rename and enables it once in the
right section, after asking the running shell to rescan and waiting until it lists the
plugin. Without a running shell it prints the enable command instead of failing.
`--no-plugin` skips the bar plugin. It never touches the pet's data in
`$XDG_STATE_HOME/codepet`: the first `codepet state` migrates the database and keeps a
backup. It never restarts the shell or the overlay and never arms a pet.

`packaging/omarchy/uninstall.sh` disables the plugin and moves the plugin and the overlay
runtime to `~/.cache/codepet/uninstall-<timestamp>/`. It keeps the pet's data and the two
commands. `tools/measure-overlay.sh PID...` reports PSS, CPU and wakeups (read-only).

## Honest limits

- **Not live-validated yet.** Everything here passes offscreen and isolated tests (Rust,
  QML smokes, node models, isolated Quickshell, staged `omarchy plugin validate`). The live
  Omarchy checks (install, idle behaviour, the playtest death walk, PSS/CPU/wakeups,
  hotplug and scale) and the human review of the art contact sheets are listed in
  [docs/validation.md](docs/validation.md) and stay **untested** until they are run.
- Neglect accrues only while the bar widget runs on Omarchy and you are active. With the
  overlay alone, or on another desktop, the companion's time is frozen.
- Presence is a convenience clock, not a tamper-proof economy: a process running as the same
  user can write `presence.json` within Rust's bounds, and deleting it stops decay.
- An ending is recorded only on the next settling command (popup open, care, Refresh, the
  CLI, a task action or bar startup); the bar shows "as of HH:MM".
- Evolution uses demo (mock) XP only. Levels and stages unlock no capability.
- There are no notifications, sounds or streaks by design.
- The portable preview (`qml/standalone/`) is a normal fixture window, not an overlay for
  other desktops. "Any Linux" is the goal, not a tested claim.

## Core (unchanged from v0.1, still enforced)

- Four crates: `codepet-core` (pure rules: care, evolution, lifecycle, presence),
  `codepet-protocol` (versioned, bounded JSON envelopes), `codepet-storage` (SQLite state)
  and the `codepet` CLI.
- Durable state in SQLite under `$XDG_STATE_HOME/codepet` (fallback
  `~/.local/state/codepet`): explicit migrations (schema v4 in v0.2), ownership and mode
  checks, WAL, one `BEGIN IMMEDIATE` transaction per mutation.
- Explicit care (food, water, play, wash, bathroom, routine), full-care Start gates and
  mandatory two-minute naps; acceptance never refills care and grants isolated demo XP
  exactly once. See [care and progression](docs/care-progression.md).
- One task owns the slot; one-use, 5-minute preflight tokens tied to the exact draft and
  care revision; a per-job mock supervisor that spawns no process; review digests rechecked
  at review, acceptance and recovery; recovery never resumes work.
- `codepet rpc` (one request frame in, one response out), `codepet state`, `codepet
  providers`, `codepet mock-demo`, `codepet --version`. After every successful mutating
  request (and every `codepet state`) the CLI republishes `bar-state.json` from committed
  state; if that fails it prints one line, `codepet: bar-state.json not updated: <reason>`,
  on stderr and the response is unchanged.

## Build and test

Requires Rust 1.85+ (MSRV, checked with `cargo +1.85.0`) and a C compiler for the bundled
SQLite. `--locked` builds need the crates from crates.io or a populated Cargo cache.

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run -p codepet -- --version
cargo run -p codepet -- mock-demo
printf '%s\n' '{"v":1,"id":"r1","method":"state.get","params":{}}' | cargo run -q -p codepet -- rpc
```

Set `XDG_STATE_HOME` to a temporary directory to experiment without touching real state.
The QML, node and asset gates are the commands in [.github/workflows/qml.yml](.github/workflows/qml.yml);
see [qml/README.md](qml/README.md).

## Not implemented

- Real providers (Codex, OpenCode, Hermes, Claude Code), adapters and authentication.
  Nothing in this repository can launch them, and none may be connected until containment,
  staging, review/apply and adapter release gates exist and are tested.
- Staging, sandboxing, cgroups, process containment, deadlines and real verification commands.
- Apply, rollback, diffs, `task.logs`, artifact paging (artifacts are capped at 24 KiB).
- A job socket, event streaming, and the Qt C++ standalone launcher.
- Automatic lock/suspend/fullscreen suppression for the overlay (the bar's presence recorder
  does stop on idle and fullscreen).

The architecture keeps **no resident Rust daemon**: short explicit commands plus one bounded
per-job supervisor while a task runs.

## Pet art and source-art catalog

The shipped companions are raster frames: 192×192 lossy WebP with alpha under
`qml/assets/pets/<id>/s<stage>/`, extracted by `tools/petart/extract.py` from GPT Image 2
sheets generated through the operator's ChatGPT subscription and anchored on the source
catalog below. Every source-sheet and frame SHA-256 is in the manifests and
[qml/assets/pets/PROVENANCE.md](qml/assets/pets/PROVENANCE.md). The extraction sheets
(about 200 MB) are not committed. They are a planned release asset,
`codepet-art-sources-v0.2.tar`; until it is attached, the sheets exist only on the
operator's workstation, which is the only place `extract.py --check` (a byte-for-byte
rebuild of the committed frames) can run. Raster art departs from issue #5's SVG-generator
contract; the decision and its consequences are in
[docs/decisions/0002-v0.2-raster-pet-art.md](docs/decisions/0002-v0.2-raster-pet-art.md).

The [generated art library](docs/art-library/README.md) keeps the 15 original concept/pose
sheets from PR #7 in Git as the style anchor, with prompts, hashes, C2PA fields, QA notes
and the licence statement; `docs/art-library/verify.py` checks them. The older vector
ginger-cat sprites remain under `qml/assets/` with their own provenance record.

## Documentation

- [Companions](docs/pets.md) and [asset provenance](qml/assets/pets/PROVENANCE.md)
- [Lifecycle rules](docs/lifecycle.md) and [v0.2 decisions](docs/decisions/0001-v0.2-lifecycle.md)
- [Raster pet art decision](docs/decisions/0002-v0.2-raster-pet-art.md)
- [Omarchy bar plugin](docs/omarchy-plugin.md)
- [Desktop overlay](docs/desktop-overlay.md) and [Quickshell host](qml/omarchy/README.md)
- [QML UI and host contract](qml/README.md)
- [Care, rest and demo progression](docs/care-progression.md)
- [Security and manual-only invariants](docs/security.md)
- [Validation report](docs/validation.md)
- [Milestone 1 contract](docs/milestone-1.md) (RPC shapes for the v0.1 methods)
- [Full target specification](docs/implementation-spec.md): design, not completed implementation
- [Architecture](docs/architecture.md), [providers](docs/providers.md),
  [licences](docs/licenses.md)

`docs/source-manifest.sha256` records the original import and is intentionally stale (see
[qml/README.md](qml/README.md)). No upstream code or sprites are vendored.
