# CodePet

A manually controlled coding companion for **Omarchy by default, with Linux portability as
the goal**. Rust owns task policy, care, the companion's life and every reward; Qt
Quick/QML (Quickshell on Omarchy) only renders what Rust returns and sends explicit
requests. The only provider is a mock that runs no command and never touches a project
folder.

![The fifteen CodePet companions at their three stages](docs/screenshots/codepet-v0.2-roster.webp)

**Current release: [v0.2.1](https://github.com/GreyforgeLabs/codepet/releases/tag/v0.2.1),
the first public release (MIT)** (`codepet --version` prints `codepet 0.2.1`). See the
[changelog](CHANGELOG.md).

## Features

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
  acknowledged. Until then the clock is **unarmed**: nothing changes on its own. A pet
  carried over from an earlier build starts unarmed, so updating never costs a pet.
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
care controls. Care, Tasks, Companion and Life tabs open in a separate focusable window
only after an explicit action (task review is shown inside Tasks). The bar and the user drive it through `codepet-overlay`
(`show|hide|toggle|tasks|care|companion|life|refresh|status|quit`). Details:
[docs/desktop-overlay.md](docs/desktop-overlay.md) and [qml/omarchy/README.md](qml/omarchy/README.md).

## Requirements

- **Runtime:** Omarchy 4 on Hyprland (tested with Omarchy 4.0.4's plugin API), with
  `omarchy` on `PATH` for the bar plugin; Quickshell 0.3.1 with `qs` on `PATH` (or
  `CODEPET_QS`) for the desktop pet; Qt's WebP image plugin (`qt6-imageformats` on Arch),
  without which the pets render blank.
- **Release binary:** x86_64 Linux with glibc 2.34 or later.
- **Building from source:** Rust 1.85 or later with cargo, and a C compiler for the bundled
  SQLite.
- **Optional:** `jq` (the installer falls back to `grep`) and `systemd-run` (caps the
  build's memory).

`omarchy plugin add <repo-url>` is not supported: the repository root has no
`manifest.json`, and the bar plugin needs the `codepet` binary and the `LifeText.js` the
installer stages. Use `packaging/omarchy/install.sh`.

## Quick start

From a source checkout (builds `codepet`):

```sh
git clone https://github.com/GreyforgeLabs/codepet && cd codepet
packaging/omarchy/install.sh --dry-run   # list every staged file; changes nothing
packaging/omarchy/install.sh             # build, install, stage and enable the bar plugin
codepet-overlay show                     # the desktop pet
```

### Install from the release archive (no build)

Download `codepet-0.2.1-x86_64-linux.tar.gz` and `SHA256SUMS` from the
[v0.2.1 release](https://github.com/GreyforgeLabs/codepet/releases/tag/v0.2.1), then:

```sh
sha256sum -c SHA256SUMS --ignore-missing
tar xzf codepet-0.2.1-x86_64-linux.tar.gz
cd codepet-0.2.1-x86_64-linux
packaging/omarchy/install.sh --dry-run
packaging/omarchy/install.sh             # installs the bundled bin/codepet; nothing is built
```

## Install on Omarchy

The installer builds `codepet` (or installs the release archive's `bin/codepet`, or
`CODEPET_BIN_SOURCE`), installs `codepet` and
`codepet-overlay` into `~/.local/bin`, copies the overlay runtime to
`~/.local/share/codepet/releases/<version>/` behind a `current` symlink, validates the
plugin with `omarchy plugin validate`, moves it into
`~/.config/omarchy/plugins/greyforge.codepet` in one rename and enables it once in the
right section, after asking the running shell to rescan and waiting until it lists the
plugin. Without a running shell it prints the enable command instead of failing.
`--no-plugin` skips the bar plugin. It never touches the pet's data in
`$XDG_STATE_HOME/codepet`: the first `codepet state` migrates the database and keeps a
backup. It never restarts the shell or the overlay and never arms a pet. It keeps the
current and the previous release under `~/.local/share/codepet/releases/`.
`tools/measure-overlay.sh PID...` reports PSS, CPU and wakeups (read-only).

### First run

1. After installing, the bar shows Ginger the cat with `◦`: not set up yet. Time is not
   counting.
2. Left-click the bar cell and choose **Set up…**: the Life tab opens in the desktop pet's
   window. Read the rules, pick permadeath (preselected) or gentle, and acknowledge. That
   starts the attended-time clock.
3. Clicks on the bar cell: left opens the card, right shows or hides the desktop pet
   (`codepet-overlay toggle`), middle opens the task window (`codepet-overlay tasks`).
4. **Companion & looks…** changes the species look; it is cosmetic.
5. Ginger is on the steady pace. Playtest can only be chosen when adopting a new
   companion, after an ending or a retirement.

The rules and numbers are in [docs/lifecycle.md](docs/lifecycle.md); the bar glyphs are in
[docs/omarchy-plugin.md](docs/omarchy-plugin.md).

### Uninstall

```sh
codepet-overlay quit                         # stop the desktop pet (refused while a task runs)
packaging/omarchy/uninstall.sh --dry-run     # from the release archive or a checkout
packaging/omarchy/uninstall.sh
```

`uninstall.sh` disables the bar plugin and moves the plugin and the overlay runtime to
`~/.cache/codepet/uninstall-<timestamp>/`; it deletes nothing. If the plugin cannot be
disabled (for example, the shell is not running), it warns, carries on and prints the
command to run later. It keeps the pet's data and the two commands. To remove everything
else:

```sh
rm ~/.local/bin/codepet ~/.local/bin/codepet-overlay
rm -r ~/.cache/codepet          # uninstall-*, release-backup-* and plugin-backup-* folders
```

Only if you also want to delete your companion for good, including its history and the
pre-migration database backup:

```sh
rm -r "${XDG_STATE_HOME:-$HOME/.local/state}/codepet"   # permanent: this cannot be undone
```

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

## Design invariants (enforced)

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
`qml/assets/pets/<id>/s<stage>/`, extracted by `tools/petart/extract.py` from sheets
generated with GPT Image 2 (OpenAI image generation) via the Codex CLI image tool and
anchored on the source catalog below. Every source-sheet and frame SHA-256 is in the
manifests and [qml/assets/pets/PROVENANCE.md](qml/assets/pets/PROVENANCE.md). The
extraction sheets (about 200 MB) are not committed: they are the release asset
[`codepet-art-sources-v0.2.tar`](https://github.com/GreyforgeLabs/codepet/releases/download/v0.2.1/codepet-art-sources-v0.2.tar) of v0.2.1, with its checksum in that release's
`SHA256SUMS`. After extracting it into `art-src/`, anyone can run `extract.py --check`, a
byte-for-byte rebuild of the committed frames (steps in
[docs/pets.md](docs/pets.md#reproducing-the-frames)). Raster art departs from issue #5's SVG-generator
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
- [Milestone 1 contract](docs/milestone-1.md) (RPC shapes for the Milestone 1 methods)
- [Full target specification](docs/implementation-spec.md): design, not completed implementation
- [Architecture](docs/architecture.md), [providers](docs/providers.md),
  [licences](docs/licenses.md) and [third-party licences](THIRD-PARTY-LICENSES.md)

`docs/source-manifest.sha256` records the original import and is intentionally stale (see
[docs/import-provenance.md](docs/import-provenance.md)). No upstream code or sprites are vendored.
