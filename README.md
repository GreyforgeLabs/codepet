<p align="center">
  <img src="docs/screenshots/codepet-cat-states.gif" alt="A CodePet cat eating, playing and resting" width="220">
</p>
<h1 align="center">CodePet</h1>
<p align="center"><strong>Good work. Better company.</strong><br>A little desktop companion that grows with the code you care for.</p>
<p align="center">
  <a href="https://github.com/GreyforgeLabs/codepet/releases/latest"><strong>Download CodePet 0.6</strong></a> ·
  <a href="#quick-start">Get started</a> ·
  <a href="#see-it-work">See a real repair</a> ·
  <a href="docs/work.md">How it works</a> · <a href="LICENSE">MIT</a>
</p>

CodePet lives beside your work on **Omarchy + Hyprland**. It finds local repositories,
checks for a suspected bug, and offers a fix. You review the changes and choose what
to apply. Accepted work feeds your companion and earns XP. Between jobs, give it
some attention, let it wander, or tuck it in for a nap.

**Find → Fix → Review → Grow.** Fifteen companions. Three growth stages.

**CodePet 0.6: the pet goes native.** The desktop pet is now a small Rust program
drawn straight through the Wayland layer shell. On our desktop it used **about 4 MB**
of memory and under 0.1 % of one core while it roamed, where the 0.5 Quickshell pet
used 38 to 168 MB. See the [changelog](CHANGELOG.md) and the
[0.6 qualification record](docs/validation.md).

## New in 0.6

| A lighter companion | A friendlier companion |
| --- | --- |
| Native Rust desktop pet: about 4 MB, sleeps until something changes | **HIDE** pill on hover, and a show/hide key |
| One timer at a time, no polling, only the current pet's pictures loaded | Clearer care icons: meat on a bone, a droplet, bubbles, a hand, a pin |
| Same roaming, drag, Pin, care wheel and placement as before | Softer care sounds |

The core is **Rust**, the desktop pet is **native Rust (Wayland layer shell)**, and
the bar plugin and Work window are **Qt Quick/QML**. There is no resident Rust daemon.
See the [changelog](CHANGELOG.md) and [qualification](docs/validation.md).

## See it work

![A real CodePet repair: inspect the complete before-and-after change](docs/screenshots/codepet-repair-demo.png)

This still shows a deliberately seeded cart-total bug, a real prepared repair, and
three independent tests passing after Apply on the preceding development build.
The saved review was replayed for the capture; Apply happened separately. It is
historical demonstration evidence from an earlier development build.

## Meet your companion

![All fifteen companions at their three growth stages](docs/screenshots/codepet-v0.2-roster.webp)

Cat, dog, capybara, penguin, robot, iguana, spider, snake, goldfish, rabbit, turtle,
frog, owl, red panda and axolotl. Aquatic pets bring their own habitat. Changing
appearance keeps your companion's name, age, care and growth.

Accepted fixes fill **food**, research fills **water**, and builds lift **mood**.
Each accepted job grants **20 XP once**. Snacks, drinks, Play and Bath are free
and grant no XP. Scouts and unsuccessful jobs grant nothing.

A happy, awake pet with energy can work; food and water do not need to be full.
After three accepted jobs or 45 minutes of accepted work, a two-minute nap restores
energy. Gentle mode is the default. **Help → Care tempo** offers Steady and Lively;
existing companions keep their tempo, while new Gentle starts use Lively.
Needs change with attended desktop time, without offline catch-up.

## Quick start

Download **`codepet-0.6.1-x86_64-linux.tar.gz`** and **`SHA256SUMS`** from
[the latest release](https://github.com/GreyforgeLabs/codepet/releases/latest).
In the download directory:

```sh
sha256sum -c SHA256SUMS --ignore-missing
tar xzf codepet-0.6.1-x86_64-linux.tar.gz
cd codepet-0.6.1-x86_64-linux
packaging/omarchy/install.sh --dry-run
packaging/omarchy/install.sh
```

The archive includes both binaries. Installation adds the bar plugin, desktop pet and
review window, rescans plugins, and upgrades saved state with recovery backups.
It does not restart your shell or start your pet's life. Use the installer to keep
binary and UI versions together. `codepet --version` prints `codepet 0.6.1`.

### First run

1. Click CodePet on the bar, choose your companion, and tap **Let's go!**
2. Left-click the desktop pet for its status, care numbers and **Client** chooser.
3. A suspected bug offers **Fix**; a finished candidate offers **Review**. Only
   explicit reviewed Apply changes your original project.
4. Right-click for quick care. Drag to move; **Pin/Roam** controls wandering.
   **Care** opens full meters, and **Details** opens longer work status.
5. Hover the pet and click **HIDE** to tuck it away. Bring it back with the
   show/hide key below, or `codepet-overlay show`.

**Show/hide key.** `codepet-overlay toggle` shows or hides the desktop pet (and starts
it when it is not running). Omarchy's defaults leave **Super + Alt + P** free; add it
to `~/.config/hypr/bindings.lua`:

```lua
o.bind("SUPER + ALT + P", "CodePet show/hide", os.getenv("HOME") .. "/.local/bin/codepet-overlay toggle")
```

Hiding lasts until you show the pet again or log in again. It is not **Off**: care
and work continue as usual. To keep the pet off the desktop, turn off
**Pet on my desktop** in the bar's right-click menu.

**Automatic checks are enabled after Let's go!** Bounded scouts search conventional
development folders such as `~/Projects`, `~/Code` and `~/Work/workspaces` when the
pet is ready. Selected source context can reach your provider, and jobs can use
the network. Pause future checks with **Details → Checks off**. There are no
automatic fixes, applications, commits or pushes. [Scope and limits](docs/work.md).

### Choose a client

![Inline client selection, with illustrative availability](docs/screenshots/codepet-client-chooser.png)

*Real native UI with synthetic data for the availability example. A catalog entry
does not mean its executor is configured or qualified.*

| Client for pet-owned work | Current adapter |
| --- | --- |
| **Codex 0.163.0** | Existing ChatGPT subscription; isolated commands and reviewed edits |
| **Claude Code 2.1.294** | Pro/Max access token; restricted file tools and WebSearch; no command-based tests |
| **OpenCode 1.19.0** | Pinned `opencode/big-pickle` route; isolated commands and reviewed edits |
| Muse, Grok, Hermes, pi | Detected choices with explanations; execution unavailable |

Selection is saved for the **next** job. It never starts work, changes an existing
job, resets cooldowns or falls back to another client. See [provider support](docs/providers.md).

### Keep it up to date

Open **Updates** on the pet or **More → Updates** on the bar. CodePet checks GitHub,
verifies the download, and installs the newer stable release while preserving age,
XP, settings and saved work. Finish or discard active work and pending review,
then close Work before upgrading. [Update and recovery details](docs/updates.md).

## Requirements

- **Omarchy 4 + Hyprland**, developed against Omarchy 4.0.4's plugin API.
- **Desktop pet:** a Wayland compositor with `wlr-layer-shell` and subsurfaces (Hyprland),
  and fontconfig for its labels. `CODEPET_PET_ENGINE=quickshell` uses the previous
  Quickshell pet instead.
- **Quickshell 0.3.1 or later** (`qs`, or `CODEPET_QS`) for the bar and Work window, and
  Qt WebP support (`qt6-imageformats` on Arch).
- **Pet-owned jobs:** a supported coding client, Linux user systemd with enforced
  memory/process limits, Bubblewrap and `prlimit`.
- Release archive: x86_64 Linux with **glibc 2.39 or later**.
- **Updates and Work launcher:** Python 3.11 or later; updates need user systemd.
- Source build: Rust 1.86 or later, Cargo and a C compiler for bundled SQLite.

<details>
<summary>Build and install from source</summary>

```sh
git clone https://github.com/GreyforgeLabs/codepet
cd codepet
packaging/omarchy/install.sh --dry-run
packaging/omarchy/install.sh
```

</details>

## Coding friends and treats

**Helpers** connects to Claude Code, Codex, OpenCode, Hermes and Grok sessions you
start separately. These observers are independent of the **Client** chosen for
pet-owned jobs. Qualifying finished turns earn treats; giving one grants **5 XP**.
The finite jar holds ten.

Connect is explicit. A local ready check makes no model call and earns no treat.
Hooks retain bounded metadata, never chats or code. Claude and Hermes manage
plugin settings through their CLIs. Codex hook entries require your review in
`/hooks`; CodePet never grants trust. [Connection and privacy details](docs/security.md).

**Off** freezes care and earning and exits the desktop pet. Stop active owned work
before turning Off or taking a nap. Codex starts its own login shell before its
observer hook; disconnect that Helpers connection to remove the hook path entirely.

### Uninstall

Finish or cancel owned work, resolve pending application recovery, and close Work.
Closing its window alone does not stop a job. Then run:

```sh
codepet-overlay quit
packaging/omarchy/uninstall.sh --dry-run
packaging/omarchy/uninstall.sh
```

Uninstall disconnects supported apps, backs up the plugin/runtime, and preserves
pet data. A silent shim may remain while a client still references it.

## Development

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked --features legacy-tasks
cargo clippy --workspace --all-targets --locked --features legacy-tasks -- -D warnings
python3 docs/v0.3/contracts/check_contracts.py
```

[QML CI](.github/workflows/qml.yml) covers offscreen interactions, models, integrations
and assets. Use disposable `HOME` and `XDG_STATE_HOME`; the installed pet is never a
test fixture. Schema 6 preserves lives/history with a migration backup; older
binaries refuse it. The retired `legacy-tasks` pipeline is non-default compatibility
code; real jobs use the current work controller.

## Art and documentation

Pet frames come from GPT Image 2 source sheets with reproducible extraction and
recorded provenance. Explore the [art library](docs/art-library/README.md),
[raster-art decision](docs/decisions/0002-v0.2-raster-pet-art.md), and
[source archive](https://github.com/GreyforgeLabs/codepet/releases/download/v0.3.0/codepet-art-sources-v0.2.tar).

[Companions](docs/pets.md) · [Lifecycle](docs/lifecycle.md) ·
[Architecture](docs/architecture.md) · [Security and privacy](docs/security.md) ·
[Desktop controls](docs/desktop-overlay.md) · [Validation](docs/validation.md) ·
[Licences](docs/licenses.md) · [Third-party notices](THIRD-PARTY-LICENSES.md)

Other Linux desktops remain a portability goal. The [original specification](docs/implementation-spec.md)
and [v0.3 contracts](docs/v0.3/contracts/README.md) preserve design history;
[the current work loop](docs/work.md) describes today's behavior.
