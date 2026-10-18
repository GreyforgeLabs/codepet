# CodePet 0.5 release checklist

Status: unreleased candidate. No tag, public push, release upload, automatic update
offer or announcement is authorized by this preparation record.

## Audit decision

Keep the Rust core/controller and native Qt Quick interface. Persistence, reward
transactions, process supervision and filesystem boundaries already live in Rust.
A UI rewrite would add migration and interaction risk without a measured benefit.
The useful-work loop is coherent: bounded scout, explicit Fix, reviewed Apply,
exactly-once reward, and free recovery care. Keep that loop and qualify its edge cases.

The candidate audit fixes concrete boundary and presentation defects, preserving
the earlier delayed-completion, nonblocking-read and controller-split work:

- Refuse a project that contains, equals or lies inside CodePet's private state,
  including a symlinked XDG state ancestor
- Reject hard-linked source files both while enumerating and when opening them,
  including replacement between baseline capture and copying
- Wait for proven provider-stream EOF within a five-second final drain budget;
  refuse a stuck stream rather than accepting incomplete output
- Keep sprite decode dimensions finite using the supported screen DPR, including
  fractional scaling
- Keep failed Apply feedback beside its review controls; interrupted application
  waits for an explicit retry without fast status polling
- Align the installer series and unreleased changelogs with the 0.5.0 metadata;
  retain the verified 0.4 public download rather than advertising unpublished assets

See [validation](validation.md) for executed checks and remaining limitations.

## Before a release decision

- [ ] Run default and `legacy-tasks` workspace tests, formatting and strict Clippy
  on the exact commit proposed for release; resolve every failure or record an
  explicitly accepted qualification exception
- [ ] Run containment gates where Bubblewrap, Linux namespaces and user-systemd
  limits actually work; check cancellation, deadline, detached children, memory,
  credential aliases, source exclusions and interrupted application recovery
- [ ] Run Quickshell host tests and real Omarchy/Hyprland interaction checks:
  show/hide, drag/pin, pointer pass-through, focus, keyboard navigation, repeated
  care, client choice, review, failed Apply, recovery, Off and restart
- [ ] Test a disposable upgrade with an existing companion: preserve life identity,
  name, birth timestamp, XP, appearance, care tempo and observer connections; verify
  schema-6 backup/recovery and refusal by older binaries
- [ ] Review common, narrow and scaled UI captures; measure actual fractional-DPR
  rendering, monitor hotplug and desktop resource use separately from offscreen tests
- [ ] Recheck supported adapter versions with native offline dummy-provider gates.
  Any paid/live inference requires its own authorization and evidence; a provider's
  claimed tests are not independent verification
- [ ] Revalidate all source-art inputs and extraction, including source sheets that
  are separate from the committed runtime frames; do not infer this from frame hashes
- [ ] Build the candidate archive from a clean exact commit, verify checksums,
  third-party notices, ABI, relative links, and extracted install/uninstall/rollback

## Publication handoff

Only after the release decision, update the candidate wording and download/demo
links to assets that actually exist. Confirm the public repository and release
targets explicitly. Preserve the private recovery history and all previous fixes.
Recover and review the previously approved announcement separately; the development
README is not a substitute for that text. Do not announce before the final artifact,
installation and download checks pass.
