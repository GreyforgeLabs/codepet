# Desktop pet (v0.3)

The desktop pet is a small transparent Quickshell surface. It reads the same
`bar-state.json` as the Omarchy widget and shares its state-priority rules. It
reacts to coding activity, needs and growth. A click makes it hop and opens Home
on the bar through Omarchy's `greyforge.codepet` panel contract. It does not open
a separate task, care or life window.

```sh
codepet-overlay show
codepet-overlay hide
codepet-overlay toggle
codepet-overlay refresh
codepet-overlay status
codepet-overlay quit
```

These are the only launcher verbs. Retired `tasks`, `care`, `companion`, `life`
and `panel` verbs are usage errors. Show/toggle while Off reports that CodePet
is off and starts nothing. Turning off **Pet on my desktop** makes the running
pet exit; the bar remains. Let's go! enables the desktop pet automatically.

The visible pet and its interaction region are meant to preserve click-through
outside the pet. The overlay takes no keyboard focus; typing lives in the bar
popup. Calm mode follows the widget's live setting. A celebration is acknowledged
locally once shown, with persisted growth acknowledgement through the bar.

The overlay does not record presence, connect coding clients, or launch coding
apps. With no bar presence recorder the pet's care clock stays frozen. It can
read exports but cannot migrate storage.

The launcher uses the installed `~/.local/share/codepet/current` runtime, `qs`
(or `CODEPET_QS`) and `codepet` (or `CODEPET_BIN`). Its status is scoped to that
runtime. All tests use isolated state and offscreen or nested surfaces; see
[validation](validation.md) for the actual compositor and input evidence.
