# Changelog

## Unreleased (v0.3)

The "a 5-year-old can use it" popup (SPEC-v0.3 §3).

- One popup with a shared header on every screen: ‹ Back, the title or the pet's name
  button, and a big power switch. Off freezes CodePet; a left click on the Off bar cell
  turns it on again.
- Home: a big animated pet with a picture thought bubble, one sentence, four picture care
  tiles (per-habitat words for fish and the robot), Treats when the jar has some, and the
  round buttons Change, Helpers and More. Every tap reacts at once.
- First run: the cat is preselected; Let's go! starts a gentle pet and the desktop pet, then
  offers to connect the coding apps it found (never Codex). Welcome back for an existing pet.
- Helpers: one row per coding app in plain words, the "open a new window" hint at once,
  Connect all, row sheets with Stop watching, and a picture guide for Codex's one more step.
- More (desktop pet, Bedtime, Calm mode, How it works, Memories, Say goodbye) and How it
  works (the rules, Hardcore behind the typed name, background helpers, Forget coding
  history, apps, privacy).
- The bar cell shows painted badges from the shared priority; only the "needs you" hand
  pulses. No tooltips, no middle click; the right click opens a small menu. The desktop pet
  opens Home through the IPC target `greyforge.codepet` (`openHome`).
- Every word comes from strings.json; sizes follow the font base size.
- Removed: the v0.2 card (meters, readiness, simulation tasks, Refresh, the sleep countdown).

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
