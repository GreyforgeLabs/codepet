# Desktop pet (v0.6)

The desktop pet is `codepet-pet`, a small native Rust program (CodePet 0.6). It draws
on a transparent, click-through Wayland layer surface (layer `top`, namespace
`codepet-overlay`): a one-pixel buffer stretched over the output, plus one small
subsurface around the pet that is the only part taking pointer input, and only over
the pet, the HIDE pill and the care wheel. It is single-threaded and event-driven: it
reads `bar-state.json`, `work-state.json` and the bar entry's settings through
inotify, keeps one timer for the next moment anything can change, and loads only the
current pet's frames. Hidden or suppressed, it gives its buffers and images back.

Clicking the pet opens the compact card window with work status, level, exact XP and
age; a second click closes it. A suspected bug offers
**Fix**; a candidate repair offers **Review**. **Care** opens Home on the bar.
Research/custom tasks and full before/after review use a separate native QML window.

Drag the pet in either direction to move it. Right-click for a small care wheel: **Pet, Snack, Drink, Play, Bath, Nap/Wake,
Pin/Roam**. Hover the pet for a **HIDE** pill; a click hides the pet and a toast names the
show/hide key (`codepet-overlay toggle`, suggested binding Super + Alt + P).
Pinning stops wandering while retaining occasional awake expressions
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
Text entry uses the bar popup or native work window. The native pet's rules are a
Rust port of the QML scene (`qml/shared/*.js`), checked against the JavaScript on 1,920
corpus cases (`crates/codepet-pet/src/model/parity.rs`). Pointer scenarios (hover,
HIDE, right-click wheel, Pin, Snack, drag, click) were exercised in a nested Hyprland
on a headless output with a virtual pointer. Isolated Qt/Quickshell tests still cover
the QML components used by the fallback engine and the bar. Actual compositor pointer/focus
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

The launcher runs `codepet-pet` beside `codepet` (or `CODEPET_PET`) and talks to it
over `$XDG_RUNTIME_DIR/codepet-pet.sock`; `codepet-pet ctl status` prints the same
status JSON. `CODEPET_PET_ENGINE=quickshell` uses the previous Quickshell overlay
(`~/.local/share/codepet/current/qml/shell.qml`, `qs` or `CODEPET_QS`) instead. No compositor
rules or screen-lock hooks are installed. See [host details](../qml/omarchy/README.md)
and [work scope](work.md).
