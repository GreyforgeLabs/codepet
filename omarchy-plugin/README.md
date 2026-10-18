# CodePet bar widget (`greyforge.codepet`)

Your coding companion on the Omarchy bar. Home shows care meters, level, exact XP,
growth progress and current-life age. Work opens a compact pet card for automatic
read-only bug checks, explicit Fix and reviewed Apply. Accepted work fills a need
and earns XP; optional external coding-app observations keep their own treat loop.

Install it with the CodePet installer, which stages this folder (plus `LifeText.js` from
`qml/shared/`), validates it and enables it on the right of the bar:

    packaging/omarchy/install.sh

## What it shows

The bar cell is the pet's picture, a painted badge and its name:

| Badge | Meaning (highest first) |
|---|---|
| moon, picture dimmed | CodePet is off (a left click turns it on) |
| egg + sparkle, "Hi!" | no pet yet, or your pet is waiting for Let's go! |
| sleeping picture (+ bandage) | asleep (or fainted) |
| bowl / drop / ball / tub | a need is at 60% or lower; urgent at 30% |
| star | unseen growth before generic activity |
| hand (pulses) | a coding app needs you, while care is healthy |
| working picture | a coding app is working |
| star | just finished, treats waiting, or just grew up |

Clicks: **left** opens the popup at Home; **right** opens a small menu (CodePet on/off,
Pet on my desktop, Helpers, More). There are no tooltips. Only the "needs you" hand pulses,
and never in Calm mode.

The popup has one big power switch on every screen. Off freezes everything (no needs drop,
no growth, the desktop pet and the coding-app hooks stop); turning it on resumes exactly
where it was. Active owned execution must be stopped, and application recovery
finished, before turning Off.

## Settings

Stored in this widget's entry in `~/.config/omarchy/shell.json`:

| Key | Default | Meaning |
|---|---|---|
| `label` | `name` | `name` (picture and name) or `none` (picture only) |
| `presence` | `true` | count active time while this widget is on the bar |
| `overlayOnLoad` | `false` | Pet on my desktop: show the desktop pet when the bar loads (Let's go! turns it on) |
| `calm` | `false` | Calm mode: no pulsing, no animation loops in the popup |
| `binary` | `""` | absolute path of `codepet` (default `~/.local/bin/codepet`); the desktop pet uses it too |
| `stateHome` | `""` | absolute test state home; every request and launcher call then uses it |

Active time counts only while the CodePet bar widget is on your Omarchy bar and you are at
the computer. The desktop pet is always started through `~/.local/bin/codepet-overlay`.
Clicking the desktop pet opens its compact work card; Care opens Home through the
bar panel. `codepet-overlay card` preserves a hidden pet and uses a compact fallback
window. Automatic checks can be paused from Details; see [work and provider
requirements](../docs/work.md).
See `docs/omarchy-plugin.md` in the [CodePet repository](https://github.com/GreyforgeLabs/codepet)
for the full contract.

MIT licensed.

Home keeps finite Treats N/10 visible, including zero. Snack/Drink are free recovery;
Play restores mood, and no care button grants XP. Reactions wait for a confirmed
response. Click the Home pet (or focus it and press Enter) for a cosmetic happy face.
Help → Care tempo offers Steady or Lively and shows the effective care rates.
The desktop right-click wheel has Pet, Snack, Drink, Play, Bath, Nap/Wake and Pin/Roam.
