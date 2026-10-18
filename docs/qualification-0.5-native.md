# CodePet 0.5: minimal remaining host qualification

Use the exact head of the qualification follow-up to merged private
[PR #1](https://github.com/GreyforgeLabs/codepet-wip/pull/1)
and its matching verified binary. Record commit/tree, binary SHA-256, Rust, pinned
client versions, Quickshell/Qt, Omarchy, Hyprland and monitor configuration. Keep
pass/fail/skip counts and raw logs. This handoff does not authorize an installed-user
upgrade, account connection, live model request or publication.

The [cloud report](qualification-0.5-cloud.md) already covers the broad Rust/Qt,
archive/upgrade, source-pet extraction and offline Claude/OpenCode checks. Do not
repeat every historical v0.2/v0.3 validation matrix. Remaining checks are below.

## 1. Capable Linux host: no physical desktop needed

The cloud container lacks usable Unix IPC, isolated-network NETLINK_ROUTE and a
user-systemd/cgroup session. These are infrastructure prerequisites, not inherently
GUI requirements. Do not loosen security to bypass a failure.

With Quickshell 0.3.1 available and `BIN` an absolute candidate binary path:

```sh
CODEPET_TEST_BIN="$BIN" python3 qml/tests/test_quickshell_host.py \
  QuickshellLoad QuickshellOverlayRuntime
python3 qml/tests/test_overlay_host.py
python3 qml/tests/test_work_host.py
python3 qml/tests/test_update_host.py
```

These tests create private HOME/XDG state and fixture programs. Verify all expected
cases executed: exit-zero skips do not qualify. They cover show/hide/placement,
IPC and failed-Apply recovery, but offscreen windows do not establish compositor
pass-through or focus.

Run the two outstanding user-service cases with the real session's
XDG_RUNTIME_DIR/DBUS_SESSION_BUS_ADDRESS. They use unique units and fixture-only
shell tools, not the installed companion:

```sh
systemctl --user show-environment >/dev/null
CODEPET_UPDATE_TEST_SYSTEMD=1 python3 qml/tests/test_update.py \
  UpdaterTests.test_real_service_restart_worker_with_only_fixture_shell_tools \
  InstallerIntegration.test_restored_overlay_outlives_updater_service
```

Verify the real work-scope settings and teardown under user-systemd: MemoryMax
1073741824, MemorySwapMax 0, TasksMax 128, RuntimeMaxSec 350 for scout and 1250 for
other work. Use synthetic work/fixtures. Record actual cgroup behavior and descendant
termination on cancel/deadline, rather than only successful `systemd-run` launch
or configured values. Do not call configured limits stress-test evidence.

For the remaining native Codex gate, verify an actual pinned ELF executable and
`codex-cli 0.163.0` first; the test can silently return when no executable exists.
With that binary's directory first on PATH, run both commands:

```sh
cargo test -p codepet --locked \
  work::tests::native_codex_tool_policy_denies_dummy_credentials_and_aliases \
  -- --exact --nocapture
cargo test -p codepet --locked --features legacy-tasks \
  work::tests::native_codex_tool_policy_denies_dummy_credentials_and_aliases \
  -- --exact --nocapture
```

These are offline dummy-credential tests. On the supported Arch target, rerun both
full workspace suites without the cloud loader-layout wrapper as the distro check.
No additional native Hermes/Grok/Muse/pi owned-adapter gate is implied.

## 2. One isolated Omarchy/Hyprland session

Keep the installed pet, database, observer connections and shell untouched. Stage
the unchanged candidate QML and use synthetic companion state, private HOME and
all XDG paths, `CODEPET_TEST_STATE=1`, `scout.enabled=false`, and a fixture backend
for **all work-rpc requests** adapted from `qml/tests/test_work_host.py`. A private
real backend may handle ordinary companion care RPC only. Do not copy live data.

**Isolate Omarchy IPC as well:** private HOME alone is insufficient.
`OverlayFlow.placementArgv()` addresses `$OMARCHY_PATH/shell`; its default could
reach the real installed shell. Use a distinct staged Omarchy instance/path or an
exact-PID fixture proxy. A proxy does not qualify real bar coupling: test that with
the isolated actual Omarchy shell. Address only verified staged paths/PIDs for
IPC and cleanup. A nested rig may use `scripts/rig-qa.sh`'s guards, but its virtual
outputs do not qualify physical scaling/hotplug.

Use the unchanged `qml/shell.qml` PanelWindow, not the offscreen FloatingWindow
substitute. In one bounded pass, record:

1. **Pointer/focus:** transparent space and decorative feedback pass clicks/typing
   to a test window behind; passive pet does not steal focus. Card/wheel Tab,
   Shift-Tab, Enter, Space and Escape work; closing/hiding releases focus
2. **Placement/lifecycle:** drag both axes without opening Home on release; pin,
   drag while pinned, hide/show and restart retain position/monitor/pin. Repeated
   show does not duplicate windows. Off exits and show while Off starts nothing
3. **Care/choice:** repeated care and Nap/Wake preserve pending/confirmed/error
   feedback without duplicate XP. Refused client choice retains the old choice;
   confirmed choice persists on reopen without launching work. Check real staged
   Omarchy Care → Home coupling
4. **Review/recovery:** use the existing fixture's applying job and reject-accept
   response (`Destination changed: a.txt`). Inspect at 820×820 and 480×400. The
   error remains beside Apply after reads/reopen; only explicit retry clears it.
   Fixture call logs must show no continuing fast get loop once recovery settles
5. **Physical outputs:** inspect actual 1× and an available fractional scale
   (prefer 1.5×; also 2× if supported), card/wheel open and closed. Check decode
   sizes, clipping, input masks and hit targets. Move to a secondary monitor,
   disconnect/reconnect it, and check visible fallback. Save/restore the real
   display configuration through the approved desktop flow

Keep common/narrow/recovery captures, actual monitor scales, staged qs logs and
before/after focus evidence (`hyprctl -j activewindow`). Explicit suppression IPC
is not automatic lock/suspend/fullscreen detection; those detectors are not claimed.

Measure matched 60-second idle and active windows for the overlay and isolated
Omarchy shell with an overlay-disabled baseline and the same workload:

```sh
tools/measure-overlay.sh --seconds 60 --label candidate-idle "$OVERLAY_PID" "$SHELL_PID"
tools/measure-overlay.sh --seconds 60 --label candidate-active "$OVERLAY_PID" "$SHELL_PID"
```

PSS/CPU/context switches are measured, not hardware power or first-switch latency.
Do not invent universal pass thresholds. Preserve logs until review.

## 3. Missing source artifact, not a desktop test

Recover the nine original icon PNGs pinned in `tools/petart/icon_sheets.json` and
run `python3 tools/petart/icons.py --check` in the documented matching codec
runtime. Public release upload is unnecessary; private inputs suffice. If originals
cannot be recovered, explicitly decide whether to accept that existing source
reproducibility exception. Runtime icon integrity and unchanged released pixels
are already verified, but do not prove source reconstruction.

Stop when these scoped results are recorded or a specific missing capability/input
blocks them. A passing test does not itself authorize merge, tag or publication.
