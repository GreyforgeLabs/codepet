# CodePet 0.6 release record

## Decision

The always-on desktop pet moves from a Quickshell (Qt Quick) process to a native Rust
program, `codepet-pet`. Resident memory and idle wakeups were the measured costs of the
0.5 pet; the rest of CodePet (Rust core, short-lived RPC, Qt Quick bar plugin and Work
window) already ran only when needed. The rewrite keeps the pet's behaviour: its rules
are a port of the QML/JavaScript scene and are checked against it by a differential test.
The Quickshell pet stays available as `CODEPET_PET_ENGINE=quickshell`.

0.6 also ships the HIDE pill and show/hide key, clearer care icons, softer care sounds
and the 0.5.1 Home/Helpers fixes. Schema 6 and the work loop are unchanged.

## Checklist

- [x] Workspace format, tests and strict Clippy, default and `legacy-tasks`
- [x] v0.3 contract checks and the offscreen QML suites, including the Quickshell host
  tests (IPC, launcher, real instances) under Quickshell 0.3.2 / Qt 6.12
- [x] Native pet: 1,920-case parity with the JavaScript face rules; nested Hyprland
  pointer scenarios (wheel, Pin, Snack, drag, click/card, HIDE, show); resource
  measurement against the 0.5 Quickshell pet (see [validation](validation.md))
- [x] Icons rebuilt and byte-checked by `tools/petart/icons.py --check`
- [x] Release archive built from the exact commit with both binaries; extracted
  install/uninstall gate in a throwaway HOME (`tools/release/package.sh`)
- [x] Installed on the development workstation as an upgrade of an existing companion;
  data, placement and settings preserved
- [ ] Physical multi-monitor hotplug and fractional-scale displays (integer scale only)

The exact commit, tag and archive checksum are recorded on the GitHub release.

## Publication

The release is published from the tagged commit with the archive and `SHA256SUMS`.
The announcement is drafted separately and posted only after explicit approval.
