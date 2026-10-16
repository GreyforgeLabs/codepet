# CodePet bar widget (`greyforge.codepet`)

Your CodePet on the Omarchy bar: a pet to feed and play with that cheers on your coding apps
(Claude Code, Codex, OpenCode, Hermes, Grok) and grows from their finished work.

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
| hand (pulses) | a coding app needs you |
| bowl / drop / ball / tub | a need is at "now" |
| working picture | a coding app is working |
| star | just finished, treats waiting, or just grew up |

Clicks: **left** opens the popup at Home; **right** opens a small menu (CodePet on/off,
Pet on my desktop, Helpers, More). There are no tooltips. Only the "needs you" hand pulses,
and never in Calm mode.

The popup has one big power switch on every screen. Off freezes everything (no needs drop,
no growth, the desktop pet and the coding-app hooks stop); turning it on resumes exactly
where it was.

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
The desktop pet opens Home with `omarchy-shell greyforge.codepet openHome`.
See `docs/omarchy-plugin.md` in the [CodePet repository](https://github.com/GreyforgeLabs/codepet)
for the full contract.

MIT licensed.
