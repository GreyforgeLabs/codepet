# Desktop pet (v0.4)

The desktop pet is a small transparent Quickshell surface. Clicking it opens a
compact card with work status, level, exact XP and age. A suspected bug offers
**Fix**; a candidate repair offers **Review**. **Care** opens Home on the bar.
Research/custom tasks and full before/after review use a separate native QML window.

Drag the pet in either direction to move it. Right-click for a small care wheel: **Pet, Snack, Drink, Play, Bath, Nap/Wake,
Pin/Roam**. Pinning stops wandering while retaining occasional awake expressions
and deliberate dragging. **More → Pin pet in place** offers the same switch. Position and monitor are saved across restarts.
The resting artwork does not determine care actions: actual naps offer **Wake**,
paused pets offer **Resume**, and fainted pets show **Snack + Drink to recover**
with Nap disabled until recovered (or Wake when an actual nap must first end).
Pet gives a brief happy reaction without changing care or XP. Snack and Drink are
free care; Play restores mood and Bath restores hygiene. Reactions wait for confirmed care responses. The
finite earned jar is separate: Home shows **Treats N/10** and confirmed **+5 XP**.
Needs appear at 60% and become urgent at 30%, even while work is active. Growth
text takes priority over generic work status, with exact XP and age still visible.
An observed coding-session bubble names its client, such as **Codex working** or
**Codex needs you**. It watches a separate session and clears when that activity
finishes or its existing stale deadline expires; it is not an owned pet job.
Home marks new growth with a small accent beside the pet for 1.6 seconds. It does
not replay on reopening the same result, and Calm hides the accent.

```sh
codepet-overlay show
codepet-overlay hide
codepet-overlay toggle
codepet-overlay card
codepet-overlay refresh
codepet-overlay status
codepet-overlay quit
codepet-work compact
```

`card` opens the existing desktop card or a compact fallback window when the
pet is hidden/unavailable. It does not silently enable a hidden desktop pet.
`codepet-work bug`, `research` or `build` opens the advanced form. Retired `tasks`,
`care`, `companion`, `life` and `panel` overlay verbs remain usage errors.

Show/toggle while Off starts nothing. Turning off **Pet on my desktop** exits
the running pet; the bar remains. Let's go! enables the desktop pet. Hide/show
preserves position and pinning. Calm mode follows the widget's live setting.

The passive desktop surface requests no keyboard focus. An explicitly opened card
or care wheel temporarily requests **OnDemand** keyboard focus, never Exclusive;
closing it or hiding the surface restores **None**. The card initially focuses its
close button, and the wheel focuses Pet. Tab moves among controls; Enter or Space
activates them. Escape first collapses an expanded client chooser, then closes the
card; Escape also closes the wheel. The wheel's × and another right-click also close it.
The visible card, care wheel and pet form separate input regions; decorative feedback
remains click-through and never takes focus. Care status is exposed to accessibility
and announced politely; each message expires after six seconds, including pending
feedback. A late reply can show a fresh message without reopening the wheel.
Text entry uses the bar popup or native work window. Isolated Qt/Quickshell tests
cover component keyboard navigation, drag, geometry and input regions. Actual compositor pointer/focus
behavior for the card and wheel remains unqualified; installed hide/show and placement preservation were
checked separately. See [validation](validation.md).

The overlay reads `bar-state.json` and the redacted `work-state.json`. Opening a
card loads private work details through a bounded RPC. An eligible one-shot timer
requests automatic scouting; owned jobs are supervised outside QML. Closing a
window does not abandon or duplicate the bounded job. Stop active execution before
turning Off or sleeping. Interrupted application remains reachable through Review.

The overlay does not record attended presence or connect observer hooks. With no
bar presence recorder the pet's care clock remains frozen. The bar settles needs
once after each successful attended-minute write; file notifications never start
a refresh loop. Automatic lock/suspend/
fullscreen detection is not implemented; those are explicit integration inputs.

The launcher uses `~/.local/share/codepet/current`, `qs` (or `CODEPET_QS`) and
`codepet` (or `CODEPET_BIN`). Its status is scoped to that runtime. No compositor
rules or screen-lock hooks are installed. See [host details](../qml/omarchy/README.md)
and [work scope](work.md).
