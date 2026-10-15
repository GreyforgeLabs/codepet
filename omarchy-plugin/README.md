# CodePet bar widget (`greyforge.codepet`)

Your CodePet companion on the Omarchy bar: needs, level and evolution at a glance, quick care,
and the desktop pet. Tasks are simulations (mock provider only).

Install it with the CodePet installer, which stages this folder (plus `LifeText.js` from
`qml/shared/`), validates it and enables it on the right of the bar:

    packaging/omarchy/install.sh

## What it shows

| Glyph | Meaning (highest precedence first) |
|---|---|
| `+` | no living companion: adopt one when you're ready (a past companion is in Life & memorial) |
| `✿` | grace has run out: your companion will pass away when the current task is finished; your work is kept |
| `!!` | urgent (or fainted) |
| `◦` | not set up yet: choose the life rules in CodePet |
| `!` | warning: a need is low |
| `?` | a result is waiting for review |
| `z` | napping |
| `‖` | paused (Pause/Travel) |
| `•` | a simulation task is working |
| `★` | your companion grew (shown once in the popup) |

A `≈` in the label or popup marks an estimate from active time counted since CodePet last
saved its state. Estimates can only raise a warning; they never predict an ending.

Clicks: **left** opens the card, **right** shows or hides the desktop pet, **middle** opens
the task window. When the desktop-pet launcher (`~/.local/bin/codepet-overlay`) is not
installed, right and middle click open the card instead, which says so.

## Settings

Stored in this widget's entry in `~/.config/omarchy/shell.json`:

| Key | Default | Meaning |
|---|---|---|
| `label` | `name` | `name`, `status` (name plus the most urgent need) or `none` |
| `presence` | `true` | count active time while this widget is on the bar |
| `overlayOnLoad` | `false` | show the desktop pet when the bar loads |
| `binary` | `""` | absolute path of `codepet` (default `~/.local/bin/codepet`); the desktop pet uses it too |
| `stateHome` | `""` | absolute test state home; every request and launcher call then uses it |

Active time counts only while the CodePet bar widget is on your Omarchy bar and you are at
the computer. The desktop pet is always started through `~/.local/bin/codepet-overlay`.
See `docs/omarchy-plugin.md` in the [CodePet repository](https://github.com/GreyforgeLabs/codepet)
for the full contract.

MIT licensed.
