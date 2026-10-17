# Desktop pet (v0.4)

The desktop pet is a small transparent Quickshell surface. Clicking it opens a
compact card with work status, level, exact XP and age. A suspected bug offers
**Fix**; a candidate repair offers **Review**. **Care** opens Home on the bar.
Research/custom tasks and full before/after review use a separate native QML window.

Drag the pet in either direction to move it. Right-click to pin/unpin; **More →
Pin pet in place** offers the same switch. Pinning stops wandering while still
allowing deliberate dragging. Position and monitor are saved across restarts.
The activity bubble distinguishes owned work from separately observed coding apps.

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

The desktop surface requests no keyboard focus; the card's controls and the pet
form input regions, while the remainder is intended to click through. Text entry
uses the bar popup or native work window. Isolated Qt/Quickshell tests cover drag,
geometry and input regions. Actual compositor pointer/focus behavior for the new
card remains unqualified; installed hide/show and placement preservation were
checked separately. See [validation](validation.md).

The overlay reads `bar-state.json` and the redacted `work-state.json`. Opening a
card loads private work details through a bounded RPC. An eligible one-shot timer
requests automatic scouting; owned jobs are supervised outside QML. Closing a
window does not abandon or duplicate the bounded job. Stop active execution before
turning Off or sleeping. Interrupted application remains reachable through Review.

The overlay does not record attended presence or connect observer hooks. With no
bar presence recorder the pet's care clock remains frozen. Automatic lock/suspend/
fullscreen detection is not implemented; those are explicit integration inputs.

The launcher uses `~/.local/share/codepet/current`, `qs` (or `CODEPET_QS`) and
`codepet` (or `CODEPET_BIN`). Its status is scoped to that runtime. No compositor
rules or screen-lock hooks are installed. See [host details](../qml/omarchy/README.md)
and [work scope](work.md).
