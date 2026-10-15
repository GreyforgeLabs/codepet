# Changelog

## 0.2.1

- Published with CodePet 0.2.1, the first public release. The manifest names the project
  homepage.
- The `binary` setting also reaches the desktop pet (passed to `codepet-overlay` as
  `CODEPET_BIN`).
- Right and middle click open the card when `codepet-overlay` is not installed, instead of
  doing nothing.
- A `codepet rpc` child whose answer the bar refused (oversize or not the protocol) is
  ended, and no new request starts until it has exited.
- The glyph table in this README lists `✿` correctly (grace has run out) and follows the
  bar's precedence.

## 0.2.0

First release of the CodePet bar widget for Omarchy 4.

- Bar cell: the companion's thumbnail, a text glyph for its status (colour is never the only cue)
  and an optional label (name, name plus status, or nothing).
- Popup card: life status, four needs with active-time windows, readiness, the current
  simulation task, quick care (species care labels), bed or wake, pause, and the desktop-pet
  launcher verbs.
- Attended presence: the bar leader counts active minutes (not idle for 180 s, no fullscreen
  window, presence wanted by CodePet) into `presence.json` for the CLI to settle.
- Read-mostly: the cell renders `bar-state.json`; file events never launch the CLI. Requests
  happen only on the leader's startup read, opening the popup, Refresh and explicit clicks.
- Optional `stateHome` setting for test runs against a throwaway state directory.
