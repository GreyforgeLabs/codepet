# CodePet 0.5 cloud qualification — 2026-10-18

**Unreleased candidate; not release-qualified.** Results extend merged private
[PR #1](https://github.com/GreyforgeLabs/codepet-wip/pull/1), based on recovery
`802c6cee29600fdc6b05f96e3f91b6745f1f132b`. The qualification follow-up PR records its final exact commit,
source tree, CI runs and archive digest, avoiding a self-referential commit hash
in this report. Only the candidate's code and fixtures were used, with disposable
HOME/XDG paths and synthetic records. There were no live model requests.

## Terminal results

| Gate | Result | Qualification boundary |
|---|---|---|
| Rust workspace, default | 479 passed, 0 failed, 3 ignored, 3 native gates explicitly filtered | Real sandbox tests executed in the loader-layout namespace below |
| Rust workspace, legacy-tasks | 632 passed, 0 failed, 3 ignored, 3 native gates explicitly filtered | Same boundary; ignored latency/resource tests are not benchmarks |
| Rust format / strict Clippy | Pass, both configurations | Rust/Cargo 1.85.0; warnings remain errors |
| Node models/controllers | 193 passed | Includes Apply/recovery feedback and no-fast-poll regression |
| Qt offscreen | 19 suites, 3,475 passing checks | PySide6/Qt 6.8.3; not compositor acceptance |
| Native offscreen life / card | 114 life checks; four card states and seven-client interactions pass | Synthetic state, pointer/keyboard/pending/failure/reopen cases |
| Sprite DPR | 23 checks at each of 1×, 1.5×, 2× | Regression failed before the fix; physical-display gate remains |
| QML lint | Exit 0 | One unchanged test-only FirstRunSmoke unqualified-access warning |
| Installer fixtures | 32 tests, one shellcheck skip | Real installer; isolated HOME and shell stubs |
| Candidate archive updater | 30 tests, two user-systemd skips | Actual archive and real candidate binary; local release/network/shell fixtures |
| Previous-release upgrade | Pass from actual 0.3.0 and 0.4.0 binaries | Seven tables' old columns preserved; schema 5→6, backup contents, XP and old-binary refusal checked |
| Failed-install recovery | Pass | Unsupported synthetic schema; all three printed recovery commands executed; details below |
| Pet source/extraction | Pass: all 1,147 generated files byte-identical | Official source archive, exact codec environment below |
| Runtime icons | Pass: 49 icons, seven sheets, identical shipped copies | Nine source PNGs unavailable; source reconstruction blocked |
| Contract/copy/plugin checks | Pass | 267 agent vectors, 37 bar fixtures, four client fixtures; no live provider claim |

### Actual Quickshell loading and IPC boundary

Official Debian Quickshell **0.3.1-1+b1** and its Qt **6.11.2** libraries were
extracted into a private runtime, without replacing system libraries. This is a
separate runtime from the portable PySide6/Qt 6.8.3 checks above.

- QuickshellLoad + QuickshellOverlayRuntime: **12 tests, three passed, nine failed,
  zero skips**. Actual-entry scanning, fixture transport and real release-binary
  RPC pass, including a repeat with WebP support. The nine IPC cases fail
- Overlay host exits 1 at IPC timeout; work host has one IPC error; update host
  has one IPC failure. These are recorded failures, not successful skips
- Original bar-plugin suite: 31 passed, 20 skipped of 51 (18 fixed host-path
  cases, unavailable qmlformat and plugin CLI). Separately, the actual Omarchy
  4.0.4 harness loads and decodes WebP successfully from verified official source

A minimal independent `socket(AF_UNIX)` call returns EPERM before bind. The
approved alternate execution request failed before launch at the runtime's
read-only mount setup. No security setting was changed or bypass used. Thus actual
Quickshell/Omarchy QML loading is established, but IPC interaction and compositor
behavior are not.

[Debian Quickshell package](https://snapshot.debian.org/file/44e5ccc01360f556a3d26471701f392023001099)
SHA-256: `7c2c6504584220b349922c63d9891934eb2ad0f5c12477b9d6eecf4883b7b479`.
Qt core package: 6.11.2+dfsg-6; QML/Quick: 6.11.2+dfsg-3; image formats: 6.11.2-2.
Official Omarchy v4.0.4 commit: `c668141e9c42b13c80c9ca4ea108e11708c5e8a5`.

### Real kernel containment

The direct Arch-target sandbox cannot find its dynamic loader on Debian. Rather
than changing production policy, an outer test-only Bubblewrap namespace binds
installed `/usr` entries read-only and adds the private alias
`/usr/lib/ld-linux-x86-64.so.2 -> x86_64-linux-gnu/ld-linux-x86-64.so.2`.
The unchanged production sandbox and tests then execute. Native Codex additionally
gets the test-only `/usr/bin/python -> python3` alias in that outer namespace.
Neither alias is created on the host. This is a filesystem-layout test harness,
not a new Debian support claim or a replacement for Arch-host acceptance.

Independent probes confirmed private fresh HOME, hidden host paths, cleared
inherited environment, read-only `/usr`, 64 MiB `/tmp`, 16 MiB `/pet-home`, and
actual ENOSPC after filling a one-MiB test tmpfs. Detached `setsid` descendants
stop after namespace termination and supervisor death. Observed rlimits are
NPROC 4096, NOFILE 256, FSIZE 64 MiB and CPU 1200 seconds; observed values are not
stress-test proof of every limit.

**Blocked:** this host has no mounted cgroup tree or user-systemd session. It cannot
qualify the real MemoryMax=1 GiB, MemorySwapMax=0, TasksMax=128 or 350/1250-second
job scope, full worker startup, or the two user-service updater lifecycle tests.
A normal NETLINK_ROUTE socket and minimal `bwrap --unshare-net` both fail with
EPERM, independently of CodePet. No security setting, scope reply or cgroup file
was changed or fabricated.

### Native owned adapters

Official pinned binaries were downloaded and publisher hashes and `--version`
were independently verified. Tests use local dummy providers and synthetic
credentials only. Both default and legacy builds were run separately.

- **Claude Code 2.1.294: pass.** 18 tool calls, 15 denials, allowed read/write/edit;
  no hook execution, credential escape or refresh-token use
- **OpenCode 1.19.0: pass.** Actual isolated bash and two local dummy requests;
  no private auth, host-canary visibility, project plugins or MCP execution
- **Codex 0.163.0: blocked.** The real binary starts, but its isolated-network
  Bubblewrap fails at the host NETLINK_ROUTE restriction before credential
  assertions. This is neither a pass nor evidence of a credential leak

Verified downloaded payloads:

| Official source | SHA-256 |
|---|---|
| [Codex archive](https://github.com/openai/codex/releases/download/rust-v0.163.0/codex-x86_64-unknown-linux-musl.tar.gz) | `beb339e8ecd9ba057a6bb2dc81ad8ca474812edfbf0fc5056ca818797648dde4` |
| [Claude binary](https://downloads.claude.ai/claude-code-releases/2.1.294/linux-x64/claude) | `27122ca7b624f537546fbef35b80c66370d974ff258f3d9b10ac50bb8771f262` |
| [OpenCode archive](https://github.com/anomalyco/opencode/releases/download/v1.19.0/opencode-linux-x64.tar.gz) | `7bb9e61ad6feacf2e89c10a6a2c1840e3de4220bb50ba1fc7110f31806076aec` |

Hermes/Grok/Muse/pi are unavailable owned adapters, not additional native release
gates. Their observer/fixture checks are a separate feature boundary.

### Archive, upgrade and failure recovery

`tools/release/package.sh` passed from a clean exact private candidate checkout:
locked release build, third-party notices, version/stripping/path checks, root-owned
sorted archive, rewritten/resolving relative links, extracted dry-run/full install,
real migrate/doctor, and uninstall hook tombstoning. Maximum required glibc is 2.39.
A second package using the same qualified binary reproduced identical archive
bytes. The exact final candidate digest and repeat-run result are recorded in that follow-up PR.
These are local artifact checks, not a public download or real Omarchy-shell test.

The additive updater test accepts `CODEPET_UPDATE_TEST_ARCHIVE`,
`CODEPET_UPDATE_TEST_BINARY` and `CODEPET_UPDATE_TEST_PREVIOUS_BINARY`; all network
and shell commands remain fixtures. Actual published 0.3.0/0.4.0 binaries create
the pre-upgrade companion, including name, life identity, birth timestamp, species,
vitals/care, events, five synthetic XP and a synthetic observer connection. Every
old column in seven tables is compared after migration and in the single backup.
The older binary refuses schema 6 without changing any database byte.

Failure recovery used an actual installed 0.4.0 archive and an intentionally
unsupported synthetic schema. The updater's real top-level error handler persisted
an error; all three installer-printed recovery commands restored the exact old
binary, plugin, runtime tree and current link, with unchanged database bytes.
`codepet-overlay` and `codepet-update` remain newer helpers: this is component/data
recovery, **not byte-for-byte rollback of the entire installation**.

Reproduce archive updater coverage with absolute verified paths:

```sh
CODEPET_UPDATE_TEST_ARCHIVE="$ARCHIVE" \
CODEPET_UPDATE_TEST_BINARY="$BIN" \
CODEPET_UPDATE_TEST_PREVIOUS_BINARY="$PREVIOUS_040_BIN" \
  python3 qml/tests/test_update.py
CODEPET_UPDATE_TEST_ARCHIVE="$ARCHIVE" \
CODEPET_UPDATE_TEST_PREVIOUS_BINARY="$PREVIOUS_030_BIN" \
  python3 qml/tests/test_update.py \
  InstallerIntegration.test_previous_release_upgrade_preserves_companion_and_backup
```

### Art-source reproducibility

The official [pet source archive](https://github.com/GreyforgeLabs/codepet/releases/download/v0.3.0/codepet-art-sources-v0.2.tar)
is 207,974,400 bytes, SHA-256
`6871c112a36c8e66588ede30ace1fe1e09bcc5dc04b438e547cd6f2cd10f0599`, matching both
GitHub metadata and the release's SHA256SUMS. Its 204 members were checked for safe
paths/types before extracting only 90 final PNGs and 15 provenance records. All
15 species passed source readiness.

`CODEPET_ART_SRC="$SOURCE/v0.2" python3 tools/petart/extract.py --check` passes with
Python 3.12.14, Pillow 12.3.0, numpy 2.3.5, scipy 1.17.0 and **libwebp 1.6.0**:
1,147 generated files are byte-identical. Pillow 11.1.0/libwebp 1.5.0 instead gives
119 WebP compressed-alpha differences and ten manifest hash differences. Every
differing image decodes to identical RGBA; no art or geometry change was found.
Do not regenerate committed images merely to accommodate an older codec.

All runtime images match public 0.4.0. The intended icon-source archive is absent
from the public asset lists checked on 2026-10-18; the nine source PNGs are also
absent from the recovery snapshot. Both icon provenance copies now state that
limitation honestly. Providing the nine original files pinned in
`tools/petart/icon_sheets.json` is sufficient to run `icons.py --check` privately.

## Remaining qualification

See [the minimal native handoff](qualification-0.5-native.md). Host IPC,
user-systemd/cgroups and native Codex can be completed on a suitably capable cloud
Linux host. Actual compositor input/focus, physical scale/hotplug and desktop
resource measurements need the supported desktop. Missing icon sources are a
separate artifact-recovery dependency, not a desktop test or new visual regression.
