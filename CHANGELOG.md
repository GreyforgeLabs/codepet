# Changelog

All notable changes to CodePet. Real providers stay disabled; every task is a mock
(simulation) task.

## 0.2.0 (testing prerelease, private)

Closes issues #5 and #6. Decisions: [docs/decisions/0001-v0.2-lifecycle.md](docs/decisions/0001-v0.2-lifecycle.md).

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
- `codepet --version`.

### Known limits
- Live desktop QA, resource measurements and the contact-sheet human review are recorded in
  [docs/validation.md](docs/validation.md); anything not yet run there is marked untested.
- Presence is not tamper-proof; neglect accrues only while the bar plugin runs on Omarchy.
