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

See the [cloud qualification report](qualification-0.5-cloud.md) for terminal
results and the [minimal host handoff](qualification-0.5-native.md) for the remaining
commands. The exact commit, archive checksum and CI links are recorded in the
private PR; repeat affected checks if that commit changes.

- [x] Default and `legacy-tasks` workspace tests, format and strict Clippy pass;
  the three native gates are explicitly separated, not counted as silent passes
- [x] Real namespace/tmpfs/read-only mounts/environment/detached-child checks pass
  with the documented test-only loader alias; production containment is unchanged
- [x] Actual existing companion upgrade from published 0.3.0 and 0.4.0 preserves
  data and backup contents; old binaries refuse the migrated database
- [x] Common/narrow/offscreen scaled UI and Apply-recovery captures reviewed
- [x] Native offline Claude and OpenCode gates pass in both feature builds
- [x] Pet source integrity and all 1,147 generated files reproduce byte-for-byte
  with the recorded libwebp 1.6.0 environment
- [x] Clean exact-commit archive, checksum repeatability, notices, ABI, links,
  extracted install/uninstall and component/data recovery exercised
- [ ] Complete IPC-dependent Quickshell host tests on a host allowing Unix sockets
- [ ] Complete user-systemd/cgroup limits and two updater service-lifecycle tests
- [ ] Complete native Codex 0.163.0 credential gate with working isolated-network
  Bubblewrap; do not relax its isolation to make the test pass
- [ ] Complete one isolated supported Omarchy/Hyprland session for pointer/focus,
  placement/care/recovery/Off, physical scale/hotplug and resource measurements
- [ ] Recover nine original icon sheets and re-extract, or explicitly accept the
  existing source-reproducibility exception; runtime pixels are unchanged

The first three outstanding gates can run on a capable cloud Linux host. Physical
compositor/display acceptance needs the supported desktop; missing icon sources
need artifact recovery, not access to the installed companion. No tag is qualified
by an unchecked item or by a historical pass elsewhere in the documentation.

## Publication handoff

Only after the release decision, update the candidate wording and download/demo
links to assets that actually exist. Confirm the public repository and release
targets explicitly. Preserve the private recovery history and all previous fixes.
Recover and review the previously approved announcement separately; the development
README is not a substitute for that text. Do not announce before the final artifact,
installation and download checks pass.
