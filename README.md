<p align="center">
  <img src="docs/screenshots/codepet-cover.jpg" alt="CodePet — fifteen animated companions for your Omarchy desktop" width="100%">
</p>

# CodePet

**A little company while you code.**

Pick a companion. Feed it, play with it, and watch it grow. Connect your coding
apps and your pet reacts as they work, wait for you, and finish. Good work earns
treats; you choose when to give them.

[**Download v0.3.0**](https://github.com/GreyforgeLabs/codepet/releases/tag/v0.3.0)
 · [Get started](#quick-start)
 · [Meet the pets](#meet-your-companion)
 · [How it works](docs/architecture.md)
 · [MIT license](LICENSE)

Built for **Omarchy 4 + Hyprland**. Local, quiet, and yours.
`codepet --version` prints `codepet 0.3.0`.
See the [changelog](CHANGELOG.md) and [qualification record](docs/validation.md).

<p align="center">
  <img src="docs/screenshots/codepet-cat-states.gif" alt="CodePet's orange cat animating through its pet states" width="640">
</p>

## Features

- **Fifteen animated companions**, each with three growth stages: cat, dog,
  capybara, penguin, robot, iguana, spider, snake, goldfish, rabbit, turtle,
  frog, owl, red panda and axolotl.
- **Big painted buttons** for food, water, play and washing. The pet reacts to
  every tap. Home uses short words and pictures; details live in Help.
- **Coding friends:** Claude Code, Codex, OpenCode, Hermes and Grok CLI. CodePet
  observes their local events; it never runs, stops or answers a coding app,
  reads its chats, or opens your project folders.
- **Treats you give:** qualifying finished work earns a treat. Tap Treats to
  give it to your pet. The jar holds ten, with durable earning limits; failed,
  cancelled and test work earns nothing. Headless helpers are excluded by default.
- **One On/Off switch.** Off saves and freezes the pet, stops presence recording,
  and exits the desktop pet. Hooks return immediately without starting CodePet.
  The small bar cell remains so one click can turn it back on.
- **Gentle by default.** Neglect can make your pet faint; matching care revives it.
  Optional Hardcore lives in Help and requires typing the pet's name. Existing
  pets, growth and memories survive the upgrade; no offline time is charged.
- **Memories** of arrivals, growth, new looks, recovery and past pets.
- **No resident Rust daemon**, notifications, sounds or streaks.

## Made for small moments

<p align="center">
  <img src="docs/screenshots/codepet-v0.3-home.png" alt="The v0.3 Home screen: Ginger helping Claude Code and Grok, with Feed, Water, Play and Bath buttons" width="364">
</p>

Big picture buttons keep care a tap away. Your pet shows which coding friends are
busy, while Helpers keeps the connection details out of the way. The image above
is the real v0.3 interface rendered with a sample two-app activity state.

## Meet your companion

![The fifteen CodePet companions at their three stages](docs/screenshots/codepet-v0.2-roster.webp)

From a tiny cat to a crowned capybara: **15 companions, three stages each**.
Change your pet's look whenever you like without losing its name, care or growth.

![A CodePet cat growing through three stages](docs/screenshots/codepet-evolution.png)

## Requirements

- Omarchy 4 on Hyprland for the bar plugin (developed against the 4.0.4 plugin API).
- Quickshell 0.3.1 (`qs`, or `CODEPET_QS`) for the desktop pet.
- Qt WebP support (`qt6-imageformats` on Arch).
- Release archive: x86_64 Linux with glibc 2.34 or later.
- Source build: Rust 1.85 or later, Cargo and a C compiler for bundled SQLite.

Other Linux desktops remain a portability goal. The standalone QML preview is a
fixture window. See [validation](docs/validation.md) for checks actually completed.

## Quick start

Download **`codepet-0.3.0-x86_64-linux.tar.gz`** and **`SHA256SUMS`** from
[the latest release](https://github.com/GreyforgeLabs/codepet/releases/latest),
then run these commands in the download directory:

```sh
sha256sum -c SHA256SUMS --ignore-missing
tar xzf codepet-0.3.0-x86_64-linux.tar.gz
cd codepet-0.3.0-x86_64-linux
packaging/omarchy/install.sh --dry-run
packaging/omarchy/install.sh
```

The archive includes the binary; nothing is compiled. The installer adds the
bar plugin and desktop pet, asks Omarchy to rescan its plugins, and upgrades
saved state with a backup. It never restarts your shell or starts your pet's life.
Use the installer: the plugin needs the bundled binary and shared runtime too.

<details>
<summary>Build and install from source</summary>

Requires Rust 1.85 or later, Cargo, and a C compiler.

```sh
git clone https://github.com/GreyforgeLabs/codepet
cd codepet
packaging/omarchy/install.sh --dry-run
packaging/omarchy/install.sh
```

</details>

### First run

1. Click your pet on the bar. The cat is already selected; choose another if you like.
2. Tap **Let's go!** Your pet appears on the desktop with gentle rules.
3. Optionally tap **Connect** for the coding friends found on your computer.
4. Later, use **Helpers** to connect more apps. Codex has a separate picture guide
   for reviewing CodePet's entries in Codex's own `/hooks` screen.

An upgraded, unstarted Ginger keeps her name, look and growth. Let's go! starts
her in gentle mode. A pet whose life was already started keeps its chosen rules.

Left-click the bar for Home; right-click for the power switch, desktop-pet
switch, Helpers and More. Clicking the desktop pet opens Home. **More** contains
Calm mode, bedtime, Help and Memories. Change chooses a different look without
resetting care or growth.

## Connect your coding apps

Connect is always an explicit action. A ready check verifies the local hook path;
then the row says it is waiting for your app. A real event confirms that connection.
The ready check makes no model call and earns no treat.

| App | Connection |
|---|---|
| Claude Code | Installs CodePet's local plugin through Claude's CLI; existing sessions may need `/reload-plugins`. |
| Grok CLI | Adds CodePet's own hook file. |
| OpenCode | Adds CodePet's plugin file. |
| Hermes | Adds CodePet's plugin and enables it through Hermes's CLI. |
| Codex | Adds hook groups while preserving other groups; review them yourself in `/hooks`. CodePet never grants trust. |

Claude and Hermes record their plugin settings using their own CLIs. CodePet
checks that unrelated hooks and permission settings were preserved. **Stop
watching** disconnects one app. See [security](docs/security.md) for exact scope,
privacy, retained earning limits and safe hook tombstones.

Codex starts its own login shell before the hook. Off cannot eliminate that
shell's cost; disconnect Codex to remove CodePet's hooks from its event path.

### Uninstall

```sh
codepet-overlay quit
packaging/omarchy/uninstall.sh --dry-run
packaging/omarchy/uninstall.sh
```

The uninstaller stops watching supported apps, disables and backs up the plugin
and desktop runtime, and preserves your pet's data. A silent shim may remain where
another app still references it. Do not remove a referenced shim by hand.

## Development

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked --features legacy-tasks
cargo clippy --workspace --all-targets --locked --features legacy-tasks -- -D warnings
python3 docs/v0.3/contracts/check_contracts.py
```

QML, integration and asset checks are in [.github/workflows/qml.yml](.github/workflows/qml.yml).
Set a throwaway `HOME` and `XDG_STATE_HOME` for experiments. The installed pet is
never a test fixture. `CODEPET_TEST_STATE=1` refuses Connect/Disconnect/test jobs.

The old mock task pipeline is unrouted in the default build, retained behind
`legacy-tasks` for v0.3.0 only. Its historical data stays intact. v0.3 does not
provide task execution, project editing or provider authentication.

## Pet art and source-art catalog

Pets use 192×192 WebP frames extracted from GPT Image 2 source sheets. The fifteen
original concept sheets are preserved in the [art library](docs/art-library/README.md).
The extraction sources are the historical release asset
[`codepet-art-sources-v0.2.tar`](https://github.com/GreyforgeLabs/codepet/releases/download/v0.3.0/codepet-art-sources-v0.2.tar).
See [reproduction instructions](docs/pets.md#reproducing-the-frames),
[provenance](qml/assets/pets/PROVENANCE.md) and the
[raster-art decision](docs/decisions/0002-v0.2-raster-pet-art.md).

## Documentation

- [Companions](docs/pets.md) and [lifecycle](docs/lifecycle.md)
- [Architecture](docs/architecture.md) and [security](docs/security.md)
- [Bar plugin](docs/omarchy-plugin.md) and [desktop overlay](docs/desktop-overlay.md)
- [Validation and remaining checks](docs/validation.md)
- [v0.3 frozen contracts](docs/v0.3/contracts/README.md)
- [Licences](docs/licenses.md) and [third-party notices](THIRD-PARTY-LICENSES.md)

The [original implementation specification](docs/implementation-spec.md) and
[v0.2 lifecycle decisions](docs/decisions/0001-v0.2-lifecycle.md) preserve design
history. Their old task and default-mode descriptions do not describe v0.3.
