# CodePet bar widget (`greyforge.codepet`)

Your CodePet companion on the Omarchy bar: needs, level and evolution at a glance, quick care,
and the desktop pet. Tasks are simulations (mock provider only).

Install it with the CodePet installer, which stages this folder (plus `LifeText.js` from
`qml/shared/`), validates it and enables it on the right of the bar:

    packaging/omarchy/install.sh

## What it shows

| Glyph | Meaning |
|---|---|
| `!` | warning: a need is low |
| `!!` | urgent (or fainted) |
| `z` | napping |
| `‖` | paused (Pause/Travel) |
| `•` | a simulation task is working |
| `?` | a result is waiting for review |
| `★` | your companion grew (shown once in the popup) |
| `◦` | not set up yet: choose the life rules in CodePet |
| `+` | no companion: adopt one when you're ready |
| `✿` | memorial |

A `≈` in the label or popup marks an estimate from active time counted since CodePet last
saved its state. Estimates can only raise a warning; they never predict an ending.

Clicks: **left** opens the card, **right** shows or hides the desktop pet, **middle** opens
the task window.

## Settings

Stored in this widget's entry in `~/.config/omarchy/shell.json`:

| Key | Default | Meaning |
|---|---|---|
| `label` | `name` | `name`, `status` (name plus the most urgent need) or `none` |
| `presence` | `true` | count active time while this widget is on the bar |
| `overlayOnLoad` | `false` | show the desktop pet when the bar loads |
| `binary` | `""` | absolute path of `codepet` (default `~/.local/bin/codepet`) |
| `stateHome` | `""` | absolute test state home; every request and launcher call then uses it |

Active time counts only while the CodePet bar widget is on your Omarchy bar and you are at
the computer. See `docs/omarchy-plugin.md` in the CodePet repository for the full contract.

MIT licensed.
