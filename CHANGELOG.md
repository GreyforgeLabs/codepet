# Changelog

All notable changes to CodePet. Real providers stay disabled; every task is a mock
(simulation) task.

## 0.2.0 - 2026-10-15 (testing prerelease, private)

Closes issues #5 (15 companions with evolution) and #6 (lifecycle, evolution and
permadeath). Decisions: [docs/decisions/0001-v0.2-lifecycle.md](docs/decisions/0001-v0.2-lifecycle.md).

### Pull requests included
- #1 Invalidate core preflight approvals after settings updates.
- #2 Unblock explicit abandonment of damaged saved reviews (review-metadata recovery).
- #3 Interactive care, mandatory naps and isolated demo progression; acceptance no longer
  refills care.
- #4 Transparent desktop pet overlay replacing the garden UI.
- #7 Generated source-art library and provenance catalog (`docs/art-library/`), with its
  review findings fixed: `verify.py` uses explicit checks that also fail under `-O`, plus
  bit-depth, grid, path, inventory, C2PA and alpha-254 checks and a licence statement.

### Companions and evolution (#5)
- 15 companions, each with three stages (young, grown at level 2, mature at level 4) and
  15 animated states, shipped as WebP frames extracted from the GPT Image 2 art sheets by
  `tools/petart` ([docs/pets.md](docs/pets.md), [PROVENANCE](qml/assets/pets/PROVENANCE.md)).
- Looks are cosmetic: switching never changes needs, rest, level, XP or countdowns.
- Evolution follows the life's accepted-task XP; each stage is journaled once and
  celebrated once (by the UI that accepted, or on the next click elsewhere).

### Lifecycle (#6)
- Per-life permadeath (preselected) or gentle mode, armed only after the rules are
  acknowledged; the migrated pet starts unarmed, so an upgrade never costs a pet.
- Needs change only during attended time recorded by the bar plugin and settled by Rust
  (CLOCK_BOOTTIME-capped, no offline catch-up), never while paused, napping or working.
- Critical periods with a frozen grace window; retirement by typed name; memorial; explicit
  level-1 adoption. Death never touches tasks, artifacts, receipts, settings or projects.
- Storage schema v4 (additive lifecycle tables), migration with a pre-upgrade backup.

### Omarchy integration
- Bar plugin `greyforge.codepet` (read-mostly bar-state.json, glyph badges, popup care,
  presence recording) and the separate desktop overlay with Care, Tasks, Companion and
  Life tabs.
- `packaging/omarchy/install.sh` / `uninstall.sh` and `tools/measure-overlay.sh`.
- `codepet --version`; the CLI republishes `bar-state.json` after every successful mutating
  request and reports a failed export on one stderr line without changing the response.

### Fixes and polish
- A companion whose grace has run out while a task is open is described the same way in
  the bar and the life panel: it will pass away once the current task is finished.
- The review screen keeps Accept, Discard and Abandon disabled for a saved review digest
  outside the protocol's 1 to 128 bytes and points to "Check interrupted tasks"; an
  `INVALID_PARAMS` refusal shows the same hint.
- The life UI's built-in roster now uses the registry's stage names for owl, red panda and
  axolotl, so the overlay and the bar name stages alike.
- CI runs the source-art verifier and its regression test.

### Known limits
- Every companion plays its own drawn poses through one shared state timing table (same
  frame order, frame time and mode); per-species idle and care timing, asked for in #5, is
  deferred ([docs/pets.md](docs/pets.md), Limits).
- Live desktop QA, resource measurements and the contact-sheet human review are recorded in
  [docs/validation.md](docs/validation.md); anything not yet run there is marked untested.
- Presence is not tamper-proof; neglect accrues only while the bar plugin runs on Omarchy.
