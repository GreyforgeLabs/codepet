# Omarchy bar plugin (v0.4)

`greyforge.codepet` is a bar widget installed by
`packaging/omarchy/install.sh`. It needs the CodePet binary, the shared pet runtime,
Quickshell and Qt's WebP plugin. The repository root is not an installable plugin.

Left-click opens Home. When Off, the same click turns CodePet on and opens Home.
Right-click opens a small menu with power, desktop pet, Helpers and More.
There is no middle-click task action. The bar shows a pet picture and painted
status badge; there are no meters, counters or technical tooltips.

Home shows exact level, total XP and current-life age beside the pet,
with a progress bar and remaining XP to the next evolution. Each care button
shows a percentage and fill bar; full means the need is satisfied. These display
the last saved state and update with care actions and state exports. The compact
bar cell remains unchanged. See the [operator correction](v0.3/contracts/notes.md#13-operator-correction-visible-stats-2026-10-16).

The popup uses Omarchy's keyboard panel so naming, Hardcore and goodbye fields
can receive keyboard focus. Every screen has the power switch. First run needs
one click and one tap; Connect is a separate optional step. All configuration
and client details are kept in Help.

## Settings and files

Settings belong to the widget entry in `~/.config/omarchy/shell.json`:
`label` (`name` or `none`), `presence`, `overlayOnLoad`, and `calm`. Test/development
overrides `binary` and `stateHome` are supported. A state-home override sets
`CODEPET_TEST_STATE=1`, refusing Connect/Disconnect/test jobs.

Pet state is `$XDG_STATE_HOME/codepet` (default `~/.local/state/codepet`). The widget
reads `bar-state.json`; its presence recorder writes only bounded presence data.
The binary owns SQLite and the export. Bar file changes do not launch an RPC;
leader startup, popup opening and explicit bar actions do so. The separate work
controller can issue a bounded automatic scout at an eligible one-shot deadline. Each action has a bounded
child process and error handling; the UI remains responsive during client jobs.

Stop active owned execution or recover pending application before turning Off.
**Off** stops presence and bar timers. The desktop pet watches the same setting
and state and exits. `openHome()`/shell summon while Off shows the saved Off look;
only a user power action or left-click turns the pet on.

## Installation, upgrade and recovery

The installer supports source builds, bundled release binaries and
`CODEPET_BIN_SOURCE`. Use `--dry-run` to inspect the exact paths. It stages a
complete plugin, validates it, swaps it in, then requests a shell rescan. It never
restarts the shell. A running desktop pet is stopped for the runtime swap and
started again afterwards if it was running. Finish/cancel owned work and close
Work before upgrading; the installer does not cancel its independent supervisor.

Runtime versions live under `~/.local/share/codepet/releases/`, with `current`
selecting one. The current and previous runtime are retained. Replaced binary and
plugin copies are kept in `~/.cache/codepet/`; the installer prints their exact
paths. Stable shim and integration directories are outside the versioned runtime.

`codepet migrate` makes a `VACUUM INTO` database backup before upgrading v4 to v5.
A v0.2 binary refuses the newer schema. If migration fails, the installer prints
the exact binary/plugin/runtime rollback commands for that install. Follow those
paths; do not guess backup names or rebuild the old binary.

After a successful migration, downgrading the binary alone is insufficient.
Turn CodePet Off, stop the desktop pet, and retain the complete current state
before restoring the pre-migration database together with the matching old binary
and runtime. That restoration discards newer progress from the active view, so
keep the newer state as a recovery copy. Never replace a live SQLite database or
mix it with another snapshot's WAL/SHM files. Prefer fixing the new version when
progress has already been made.

Uninstall uses each app's disconnect path, disables and backs up the plugin and
runtime, preserves pet data, and leaves a silent hook tombstone where needed.
Run `codepet doctor --json` for bounded metadata diagnostics.

Actual offscreen, nested-compositor and live-desktop checks are distinguished in
[validation](validation.md). See [plugin source notes](../omarchy-plugin/README.md)
for the rendering contract.

### Desktop placement and work

Drag the pet to move it. Right-click to pin or unpin it; **More → Pin pet in place** provides the same switch. Pinning stops wandering while allowing deliberate dragging. The chosen position and monitor survive restarts. Click without dragging to open the compact work card. Its Care action opens Home.
Work/Fix on Home also opens that card, falling back to a compact window when the
desktop pet is hidden. It preserves the hidden setting. Details offers the automatic
checks toggle; a finding requires explicit Fix, followed by reviewed Apply. See
[work](work.md) for scope, readiness and recovery.
