#!/usr/bin/env python3
"""Source contract + pure JS policy tests, plus real Quickshell load smokes when `qs` is
installed. NOT Wayland runtime proof: the load smokes run offscreen with no compositor.
Also covers the passive bar-state.json reader (BarState.js, BarStateWatcher.qml), the
v0.3 overlay lifecycle (exit on Off and when "Pet on my desktop" is off, click -> Home
through a stub `omarchy-shell`) and the packaging/omarchy/codepet-overlay verb launcher
(static checks, a fake `qs`, and, when Quickshell is installed, a real isolated instance).
Nothing here reaches the live desktop: every Quickshell run uses a private HOME, XDG
and runtime directory, no display, and stub `codepet` / `omarchy-shell` programs.
Run: python qml/tests/test_quickshell_host.py (Node required for policy tests).
"""
from pathlib import Path
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

QML = Path(__file__).resolve().parents[1]
REPO = QML.parent
SHELL = (QML / "shell.qml").read_text()
CODE = re.sub(r"//[^\n]*", "", SHELL)
POLICY = (QML / "omarchy/OverlayPolicy.js").read_text().replace(".pragma library", "")
BAR_STATE_JS = (QML / "omarchy/BarState.js").read_text().replace(".pragma library", "")
WATCHER = re.sub(r"//[^\n]*", "", (QML / "omarchy/BarStateWatcher.qml").read_text())
ADAPTER = re.sub(r"//[^\n]*", "", (QML / "omarchy/HostAdapter.qml").read_text())
PET = re.sub(r"//[^\n]*", "", (QML / "shared/DesktopPet.qml").read_text())
LAUNCHER_PATH = REPO / "packaging/omarchy/codepet-overlay"
LAUNCHER = LAUNCHER_PATH.read_text()


class HostContract(unittest.TestCase):
    def test_surface_is_transparent_nonexclusive_and_never_focusable(self):
        for token in ('color: "transparent"', 'exclusiveZone: 0',
                      'WlrLayershell.layer: WlrLayer.Top',
                      'WlrLayershell.keyboardFocus: WlrKeyboardFocus.None',
                      'anchors { left: true; right: true; top: true; bottom: true }'):
            self.assertIn(token, CODE)
        self.assertNotIn('WlrKeyboardFocus.OnDemand', CODE)
        self.assertNotIn('WlrKeyboardFocus.Exclusive', CODE)

    def test_hit_mask_is_only_the_pet(self):
        mask = block(CODE, 'mask: Region {')
        self.assertEqual(mask.count('Region {'), 1)
        self.assertNotIn('item:', mask)
        self.assertNotIn('mask: null', CODE)
        self.assertIn('OverlayPolicy.clipHit(pet.petHit, width, height, shell.surfaceActive)', CODE)
        for axis in ('x', 'y', 'width', 'height'):
            self.assertIn(f'{axis}: overlay.petInput.{axis}', mask)
        # The thought bubble is decorative: no menu or prop region would be a dead click zone.
        for gone in ('menuHit', 'menuInput', 'propsHit', 'propsInput'):
            self.assertNotIn(gone, CODE)
        bubble = block(PET, 'Rectangle {')
        self.assertIn('objectName: "desktopPet.bubble"', bubble)
        self.assertNotIn('MouseArea', bubble)
        self.assertEqual(PET.count('MouseArea {'), 1, 'one input area: the pet hit box')

    def test_single_screen_one_window_no_task_window(self):
        self.assertEqual(CODE.count('PanelWindow {'), 1)
        for gone in ('FloatingWindow', 'TabBar', 'Panel {', 'taskWindow', 'views:', 'Variants {', 'Loader {'):
            self.assertNotIn(gone, CODE)
        self.assertEqual(CODE.count('HostAdapter {'), 1)
        self.assertNotIn('host: hostAdapter', CODE, 'the pet has no transport')
        self.assertLess(CODE.index('HostAdapter {'), CODE.index('PanelWindow {'))
        self.assertEqual(CODE.count('screen: shell.selectedScreen'), 1)
        # One timer only: the single-shot settle of a "Pet on my desktop" off (no polling).
        self.assertEqual(CODE.count('Timer {'), 1)
        settle = block(CODE, 'Timer {')
        self.assertIn('repeat: false', settle)
        self.assertIn('interval: OverlayFlow.SETTLE_MS', settle)
        self.assertNotIn('running:', settle)
        for forbidden in ('task.', 'pet.care', 'pet.events_seen', 'destroy(', 'Qt.exit('):
            self.assertNotIn(forbidden, CODE)
        self.assertNotIn('parentWindow:', CODE)

    def test_exits_only_on_off_or_desktop_pet_off(self):
        self.assertEqual(CODE.count('Qt.quit()'), 1)
        self.assertIn('Qt.quit()', block(CODE, 'function leave() {'))
        self.assertIn('readonly property string exitReason: OverlayFlow.exitReason(shell.barState, shell.desktopPet)',
                      CODE)
        self.assertIn('onExitReasonChanged: shell.checkExit()', CODE)
        check = block(CODE, 'function checkExit() {')
        self.assertIn('OverlayFlow.exitsAtOnce(shell.exitReason, shell.sawDesktopPetOn)', check)
        self.assertIn('settleTimer.restart()', check)
        self.assertIn('settleTimer.stop()', check)
        self.assertIn('onDesktopPetChanged: if (shell.desktopPet === true) shell.sawDesktopPetOn = true', CODE)
        # "Pet on my desktop" and Calm mode are the bar plugin's settings: watched, never written,
        # in the file the Omarchy shell itself uses ($HOME/.config, not XDG_CONFIG_HOME).
        settings = block(CODE, 'FileView {')
        self.assertIn('OverlayFlow.settingsPath(Quickshell.env("HOME"))', settings)
        self.assertNotIn('XDG_CONFIG_HOME', CODE)
        self.assertIn('watchChanges: true', settings)
        self.assertIn('shell.readSettings(text())', settings)
        read = block(CODE, 'function readSettings(text) {')
        self.assertIn('OverlayFlow.desktopPetSetting(text)', read)
        self.assertIn('OverlayFlow.calmSetting(text)', read)
        for write in ('setText', 'writeAdapter', 'updateEntryInline'):
            self.assertNotIn(write, CODE)

    def test_click_opens_home_through_the_omarchy_shell(self):
        self.assertIn('onHomeRequested: shell.openHome()', CODE)
        home = block(CODE, 'function openHome() {')
        self.assertIn('Quickshell.execDetached(OverlayFlow.openHomeArgv(Quickshell.env("OMARCHY_PATH")))', home)
        self.assertIn('if (!OverlayFlow.homeDue(shell.lastHomeMs, now))', home)
        self.assertLess(home.index('homeDue'), home.index('execDetached'))
        self.assertNotIn('Process', CODE)

    def test_suppression_and_explicit_restore(self):
        for name in ('sessionLocked', 'sessionSuspended', 'selectedScreenFullscreen'):
            self.assertIn(f'property bool {name}: false', CODE)
        self.assertIn('!explicitlyHidden && !sessionBlocked', CODE)
        self.assertIn('!selectedScreenFullscreen', CODE)
        self.assertIn('&& exitReason === ""', CODE)
        self.assertIn('surfaceActive: shell.surfaceActive && overlay.visible', CODE)
        self.assertIn('function show(): void { shell.explicitlyHidden = false }', CODE)
        self.assertNotIn('WlSessionLock', CODE)
        self.assertIn('reducedMotion: shell.reducedMotion', block(CODE, 'DesktopPet {'))
        self.assertIn('OverlayFlow.reducedMotion(shell.reducedMotionOverride, shell.calmSetting,', CODE)
        self.assertIn('onCalmSettingChanged: shell.reducedMotionOverride = null', CODE)

    @unittest.skipUnless(shutil.which('node'), 'Node unavailable; pure-JS policy untested')
    def test_monitor_and_hit_region_policy(self):
        script = POLICY + r'''
const assert = require('assert');
const a = {name: 'eDP-1'}, b = {name: 'DP-1'};
assert.equal(selectScreen([], 'DP-1'), null);
assert.equal(selectScreen([a,b], 'DP-1'), b);
assert.equal(selectScreen([a], 'DP-1'), a); // preferred unplugged
assert.equal(selectScreen([a,b], 'missing'), a);
assert.equal(selectScreen([b], ''), b);
const hit = {x: 10.5, y: 20.25, width: 50, height: 40};
assert.deepEqual(clipHit(hit, 100, 100, true), {x:10,y:20,width:51,height:41});
assert.equal(clipHit(hit, 100, 100, false).width, 0);
assert.deepEqual(clipHit({x:-5,y:-9,width:20,height:20}, 10, 8, true),
                 {x:0,y:0,width:10,height:8});
assert.equal(clipHit(hit, 0, 0, true).width, 0); // no output geometry
assert.equal(clipHit({x:NaN,y:0,width:10,height:10},100,100,true).width,0);
assert.equal(clipHit({x:0,y:0,width:Infinity,height:10},100,100,true).width,0);
assert.equal(clipHit({x:120,y:0,width:10,height:10},100,100,true).width,0);
assert.equal(clipHit({x:0,y:0,width:0,height:10},100,100,true).width,0);
assert.equal(clipHit(null,100,100,true).width,0);
console.log('monitor selection, hotplug fallback, clipping and inactive masks passed');
'''
        result = subprocess.run(['node', '-e', script], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)


FAKE_CODEPET = """#!{python} -I
# Test-only stand-in for `codepet rpc`: answers one request with a typed error.
import json, sys
req = json.loads(sys.stdin.readline())
print(json.dumps({{"v": 1, "id": req["id"], "ok": False, "error": {{
    "code": "STORAGE_UNAVAILABLE", "message": "load smoke stub", "retryable": False}}}}))
"""
FAKE_OMARCHY_SHELL = """#!{python} -I
# Test-only stand-in for omarchy-shell: logs its argv to $HOME/omarchy-shell.log.
import json, os, sys
with open(os.path.join(os.environ["HOME"], "omarchy-shell.log"), "a") as f:
    f.write(json.dumps(sys.argv[1:]) + "\\n")
with open(os.path.join(os.environ["HOME"], "omarchy-shell.env.log"), "a") as f:
    f.write(json.dumps(os.environ.get("OMARCHY_PATH")) + "\\n")
"""
LOAD_ERRORS = re.compile(r"outside of the config folder|is not a type|is not installed"
                         r"|module \S+ not found|Failed to load configuration")


def isolated_env(root):
    """A Quickshell environment that cannot reach the live desktop: no Wayland/X11
    display, offscreen Qt, and private HOME/XDG/runtime directories."""
    run = root / "run"
    bin_dir = root / "bin"
    for d in (root / "home", run, bin_dir):
        d.mkdir(mode=0o700)
    fake = bin_dir / "codepet"
    fake.write_text(FAKE_CODEPET.format(python=sys.executable))
    fake.chmod(0o700)
    # The overlay opens Home with `omarchy-shell -q shell summon …`: this stub only logs.
    shell = bin_dir / "omarchy-shell"
    shell.write_text(FAKE_OMARCHY_SHELL.format(python=sys.executable))
    shell.chmod(0o700)
    home = root / "home"
    return {
        "PATH": f"{bin_dir}:/usr/bin:/bin",
        "HOME": str(home),
        "XDG_RUNTIME_DIR": str(run),
        # Elsewhere on purpose: the Omarchy shell (and so the overlay) reads
        # $HOME/.config/omarchy/shell.json whatever XDG_CONFIG_HOME says.
        "XDG_CONFIG_HOME": str(root / "xdg-config"),
        "XDG_CACHE_HOME": str(home / ".cache"),
        "XDG_DATA_HOME": str(home / ".local/share"),
        "XDG_STATE_HOME": str(home / ".local/state"),
        "QT_QPA_PLATFORM": "offscreen",
    }


def stage_without_layer_shell(root):
    """Copy the entry's config folder, replacing the layer-shell PanelWindow (which
    needs a Wayland compositor) with a FloatingWindow, so the rest of the real entry
    can run offscreen. Every substitution is asserted, so the copy cannot drift."""
    stage = root / "stage"
    stage.mkdir()
    for name in ("shared", "omarchy", "assets"):
        shutil.copytree(QML / name, stage / name)
    text = SHELL
    assert text.count("PanelWindow {") == 1
    text = text.replace("PanelWindow {", "FloatingWindow {")
    for line in ("anchors { left: true; right: true; top: true; bottom: true }",
                 "exclusiveZone: 0", "WlrLayershell.namespace: \"codepet-overlay\"",
                 "WlrLayershell.layer: WlrLayer.Top",
                 "WlrLayershell.keyboardFocus: WlrKeyboardFocus.None"):
        assert text.count(line) == 1, line
        text = text.replace(line, "")
    (stage / "shell.qml").write_text(text)
    return stage / "shell.qml"


@unittest.skipUnless(shutil.which("qs"), "Quickshell (qs) unavailable; entry load untested")
class QuickshellLoad(unittest.TestCase):
    def test_real_entry_gets_past_the_config_scanner(self):
        with tempfile.TemporaryDirectory() as tmp:
            env = isolated_env(Path(tmp))
            result = subprocess.run(["qs", "-p", str(QML / "shell.qml")], env=env,
                                    capture_output=True, text=True, timeout=30)
        out = result.stdout + result.stderr
        # Offscreen there is no layer-shell backend. Reaching the PanelWindow means every
        # import, shared component, asset path and JS module resolved.
        self.assertIn("No PanelWindow backend loaded", out, out)
        self.assertIsNone(re.search(r"outside of the config folder|is not a type"
                                    r"|is not installed|module \S+ not found", out), out)

    def test_staged_entry_loads_and_serves_ipc(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = isolated_env(root)
            entry = stage_without_layer_shell(root)
            log = (root / "qs.log").open("w+")
            proc = subprocess.Popen(["qs", "-p", str(entry)], env=env, stdout=log,
                                    stderr=subprocess.STDOUT)
            try:
                deadline = time.monotonic() + 20
                listing = ""
                while time.monotonic() < deadline and proc.poll() is None:
                    listing = subprocess.run(["qs", "ipc", "-p", str(entry), "show"], env=env,
                                             capture_output=True, text=True, timeout=10).stdout
                    if "target codepet" in listing:
                        break
                    time.sleep(0.2)
                self.assertIn("target codepet", listing)
                # `call --`: without it qs 0.3.1 parses the `show` function as the
                # `qs ipc show` subcommand, prints the target list and exits 0.
                def ipc(*call):
                    return subprocess.run(["qs", "ipc", "-p", str(entry), "call", "--", "codepet",
                                           *call], env=env, capture_output=True, text=True,
                                          timeout=10)
                for call in (["show"], ["hide"], ["panel"], ["tasks"], ["openHome"],
                             ["setReducedMotion", "true"], ["setFullscreen", "false"]):
                    r = ipc(*call)
                    self.assertEqual(r.returncode, 0, (call, r.stdout, r.stderr))
                    self.assertNotIn("target codepet", r.stdout, call)
                    self.assertNotIn("Function not found", r.stdout, call)
                st = json.loads(ipc("status").stdout)
                # panel, tasks and openHome within a second: one Omarchy call (coalesced).
                self.assertEqual((st["supervisor"], st["home_requests"], st["calm"]), (False, 1, True), st)
                self.assertNotIn("window", st)
                ipc("hide")
                self.assertEqual(json.loads(ipc("status").stdout)["pet"], "hidden")
                ipc("show")
                self.assertEqual(json.loads(ipc("status").stdout)["pet"], "shown")
                self.assertIsNone(proc.poll(), "shell exited during IPC calls")
            finally:
                proc.terminate()
                try:
                    proc.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
                log.seek(0)
                out = log.read()
                log.close()
        self.assertIn("Configuration Loaded", out, out)
        self.assertIsNone(LOAD_ERRORS.search(out), out)



def block(text, opener):
    """The brace-balanced block that starts at `opener` (which ends with '{')."""
    start = text.index(opener)
    depth = 0
    for i in range(start + len(opener) - 1, len(text)):
        if text[i] == '{':
            depth += 1
        elif text[i] == '}':
            depth -= 1
            if depth == 0:
                return text[start:i + 1]
    raise AssertionError(f"unbalanced block after {opener!r}")


class HostContractV03(unittest.TestCase):
    """v0.3 host entry (source contracts; SPEC-v0.3 §3.8, §8)."""

    def test_buttons_that_show_a_name_render_plain_text(self):
        # Button labels default to AutoText: a pet named "<b>Rex</b>" or "<img src=...>" would
        # render as rich text (and fetch the image). Any button whose label includes a
        # name must replace its contentItem with a PlainText Text.
        found = 0
        for path in sorted((QML / "shared").glob("*.qml")):
            text = path.read_text()
            for m in re.finditer(r'\b(?:Button|RadioButton|CheckBox) \{', text):
                blk = block(text[m.start():], m.group(0))
                label = re.search(r'(?m)^\s*text: (.*)$', blk)
                if label and re.search(r'\b(?:root|life|snap)\.name\b', label.group(1)):
                    found += 1
                    self.assertIn('textFormat: Text.PlainText', blk, f'{path.name}: {label.group(1)}')
        self.assertGreaterEqual(found, 3)

    def test_ipc_functions_are_typed_and_complete(self):
        ipc = block(CODE, 'IpcHandler {')
        declared = re.findall(r'function (\w+)\(([^)]*)\)\s*(?::\s*(\w+))?', ipc)
        self.assertTrue(declared)
        for name, _args, ret in declared:
            self.assertTrue(ret, f'IPC function {name} needs a return type for qs ipc')
        names = {name: ret for name, _args, ret in declared}
        self.assertEqual(names.get('ping'), 'string')
        self.assertEqual(names.get('status'), 'string')
        for verb in ('show', 'hide', 'toggle', 'openHome', 'panel', 'tasks', 'care', 'companion', 'life',
                     'refresh', 'setReducedMotion', 'selectScreen', 'setSessionState', 'setFullscreen'):
            self.assertEqual(names.get(verb), 'void', verb)
        # Every verb the launcher can send exists (an older verb opens Home).
        verbs = re.search(r'case "\$VERB" in\n\s+([a-z|]+)\) ;;', LAUNCHER).group(1).split('|')
        for verb in verbs:
            if verb != 'quit':
                self.assertIn(verb, names, verb)
        for verb in ('panel', 'tasks', 'care', 'companion', 'life', 'openHome'):
            self.assertIn('shell.openHome()', block(ipc, f'function {verb}(): void {{'), verb)
        # ping's fixed prefix is what the launcher's liveness check matches.
        self.assertIn('"codepet-overlay " +', ipc)
        self.assertIn('TOKEN="codepet-overlay"', LAUNCHER)
        # The launcher's quit refuses unless status says no task runs; v0.3 has none.
        self.assertIn('"supervisor": false', block(ipc, 'function status(): string {'))
        # IPC properties would make IpcHandler expose change signals; status is a function.
        self.assertNotIn('property', ipc)

    def test_codepet_bin_and_open_are_honoured(self):
        self.assertIn('command: [Quickshell.env("CODEPET_BIN") || "codepet", "rpc"]', CODE)
        self.assertIn('Component.onCompleted: barWatcher.reload()', CODE, 'no RPC at startup when the file is there')
        # The launcher sends its verb after a start; reading CODEPET_OPEN too would open Home twice.
        self.assertNotIn('CODEPET_OPEN', CODE)
        start = block(CODE, 'function startupRead() {')
        self.assertEqual(start.count('request('), 1, 'at most one startup read')
        self.assertIn('if (shell.startupReadDone || shell.barState !== null)', start)
        self.assertIn('onUnavailable: shell.startupRead()', CODE)

    def test_refresh_is_one_explicit_read(self):
        fn = block(CODE, 'function refresh() {')
        self.assertIn('if (!hostAdapter.busy)', fn)
        self.assertEqual(fn.count('request('), 1)
        self.assertIn('function refresh(): void { shell.refresh() }', CODE)
        self.assertEqual(CODE.count('request('), 2, 'only startupRead and refresh talk to codepet')

    def test_bar_state_watcher_is_passive(self):
        self.assertIn('watchChanges: true', WATCHER)
        self.assertIn('onFileChanged: reload()', WATCHER)
        self.assertIn('FileView {', WATCHER)
        for forbidden in ('request(', 'host', 'Host', 'Process', 'Timer', 'execDetached',
                          'setText', 'Quickshell.execDetached', 'callLater'):
            self.assertNotIn(forbidden, WATCHER)
        self.assertIn('BarStateWatcher {', CODE)
        self.assertIn('onUpdated: state => shell.barState = state', CODE)
        self.assertIn('BarState.statePath(Quickshell.env("XDG_STATE_HOME"), Quickshell.env("HOME"))',
                      CODE)
        self.assertIn('barState: shell.barState', block(CODE, 'DesktopPet {'))
        # An answer may have (re)created the file: re-read it (a read, not RPC).
        self.assertIn('onAnswered: barWatcher.reload()', CODE)

    def test_host_adapter_is_a_plain_transport(self):
        for token in ('property string clientTag: "qs"', 'signal answered(string method, var response)',
                      'return root.clientTag + "-" +', 'property int responseTimeoutMs: 15000'):
            self.assertIn(token, ADAPTER)
        self.assertIn('clientTag: "overlay"', CODE)
        for gone in ('snapshot', 'applyBarState', 'BarState', 'supervisor', 'Supervisor', 'task.',
                     'taskAccepted', 'presentationLocks', 'openCompanion', 'openLife'):
            self.assertNotIn(gone, ADAPTER)


BAR_STATE_NODE = r"""
const assert = require('assert');
// A v0.2-era v1 file (no v0.3 blocks), as an older writer exported it.
function bar(over) {
  const b = {schema: 'codepet.bar.v1', state_version: 812, generated_at_ms: 0, app_version: '0.2.0',
    ruleset: 'lifecycle-v1',
    life: {life_id: 'life-aaaaaaaaaaaaaaaaaaaaaaaa', ordinal: 1, name: 'Ginger', appearance: 'cat',
           stage: 2, level: 2, xp: 120, next_stage_xp: 400, status: 'warning', mode: 'permadeath',
           armed: true, pace: 'steady', paused: false, fainted: false, death_pending: false},
    meters: {hunger: 6100, thirst: 2800, mental_health: 8000, hygiene: 9000},
    needs: [{need: 'thirst', action: 'water', state: 'low', until_warning_ms: 0, until_critical_ms: 15840000,
             until_end_ms: 37440000}],
    critical: null,
    care: {sleeping: false, sleep_remaining_ms: 120000, sleep_started_at_ms: null, sleep_due: false,
           bathroom_due: false, ready_to_work: false, readiness_reasons: ['Needs water']},
    task: {state: 'running', category: 'hunger'},
    presence: {wanted: true, life_id: 'life-aaaaaaaaaaaaaaaaaaaaaaaa', tick_ms: 60000, idle_timeout_s: 180},
    unseen_events: [{seq: 88, stage: 2}],
    memorial: {count: 1, latest_name: 'Biscuit'}};
  return Object.assign(b, over || {});
}
const text = (o) => JSON.stringify(o);
const clone = (o) => JSON.parse(JSON.stringify(o));
const v2 = (over) => Object.assign(clone(V2_FIXTURES['waiting']), over || {});

// statePath: XDG rules.
assert.equal(statePath('/x/state', '/home/u'), '/x/state/codepet/bar-state.json');
assert.equal(statePath('/x/state/', '/home/u'), '/x/state/codepet/bar-state.json');
assert.equal(statePath('', '/home/u'), '/home/u/.local/state/codepet/bar-state.json');
assert.equal(statePath(undefined, '/home/u/'), '/home/u/.local/state/codepet/bar-state.json');
assert.equal(statePath('relative/state', '/home/u'), '/home/u/.local/state/codepet/bar-state.json');
assert.equal(statePath('', ''), '');
assert.equal(statePath(null, 'relative'), '');

// Every frozen v2 fixture parses; a v1-labelled copy of it too (the v0.3.0 writer keeps
// the v1 label until both readers accept v2).
for (const [name, doc] of Object.entries(V2_FIXTURES)) {
  assert.ok(parse(text(doc)), 'v2 fixture ' + name);
  assert.ok(parse(text(Object.assign(clone(doc), {schema: 'codepet.bar.v1'}))), 'v1-labelled ' + name);
}

// parse: bounds, schema and shape; anything off is ignored as a whole.
assert.equal(parse(text(bar())).state_version, 812, 'a v0.2 v1 file still parses');
assert.equal(parse(''), null);
assert.equal(parse(null), null);
assert.equal(parse('{not json'), null);
assert.equal(parse(text(bar({schema: 'codepet.bar.v3'}))), null);
assert.equal(parse(text(bar({schema: 'codepet.bar.v2'}))), null, 'a v2 label needs the v0.3 blocks');
assert.equal(parse(text(bar({state_version: -1}))), null);
assert.equal(parse(text(bar({state_version: 1.5}))), null);
assert.equal(parse(text(bar({state_version: '812'}))), null);
assert.equal(parse('[1,2]'), null);
const big = bar(); big.pad = 'x'.repeat(16384);
assert.equal(parse(text(big)), null, 'over 16 KiB');
const wide = bar(); wide.pad = 'é'.repeat(8000);  // < 16384 UTF-16 units, > 16 KiB UTF-8
assert.ok(text(wide).length <= 16384);
assert.equal(parse(text(wide)), null, 'byte bound is UTF-8');
assert.equal(parse(text(bar({meters: {hunger: 10001, thirst: 0, mental_health: 0, hygiene: 0}}))), null);
assert.equal(parse(text(bar({life: Object.assign(bar().life, {appearance: '../x'})}))), null);
assert.equal(parse(text(bar({life: Object.assign(bar().life, {stage: 4})}))), null);
assert.equal(parse(text(bar({life: Object.assign(bar().life, {paused: 'no'})}))), null);
assert.equal(parse(text(bar({life: Object.assign(bar().life, {name: 'n'.repeat(97)})}))), null);
assert.equal(parse(text(bar({life: null}))).life, null);
assert.equal(parse(text(bar({care: Object.assign(bar().care, {readiness_reasons: Array(9).fill('a')})}))), null);
assert.equal(parse(text(bar({care: Object.assign(bar().care, {sleeping: 1})}))), null);
const noCompat = bar(); delete noCompat.care.ready_to_work; delete noCompat.care.readiness_reasons;
assert.ok(parse(text(noCompat)), 'the v0.3.0 compatibility care keys are optional (dropped in v0.3.1)');
assert.equal(parse(text(bar({task: {state: 'Running!'}}))), null);
assert.equal(parse(text(bar({needs: [{need: 'mood', action: 'play', state: 'low'}]}))), null);
assert.equal(parse(text(bar({unseen_events: [{seq: 1, stage: 7}]}))), null);
assert.equal(parse(text(bar({memorial: {count: -1}}))), null);
assert.equal(parse(text(bar({life: Object.assign(bar().life, {status: 'dead'})}))).life.status, 'dead');

// The v0.3 blocks: all four or none, each typed and bounded.
assert.equal(v2().enabled, true);
assert.ok(parse(text(v2())));
for (const k of ['enabled', 'agent', 'treats', 'clients']) {
  const d = v2(); delete d[k];
  assert.equal(parse(text(d)), null, 'v2 without ' + k);
  d.schema = 'codepet.bar.v1';
  assert.equal(parse(text(d)), null, 'a partial set of v0.3 blocks under v1 too: ' + k);
}
assert.equal(parse(text(v2({enabled: 'yes'}))), null);
assert.equal(parse(text(v2({enabled: false}))).enabled, false);
const agent = (o) => v2({agent: Object.assign(clone(v2().agent), o)});
assert.equal(parse(text(agent({state: 'busy'}))), null);
assert.equal(parse(text(agent({apps: Array(6).fill(v2().agent.apps[0])}))), null, 'at most five apps');
assert.equal(parse(text(agent({apps: [{id: 'gemini', state: 'working', since_ms: 1, stale_at_ms: 2}]}))), null,
             'unsupported client id');
assert.equal(parse(text(agent({apps: [{id: 'codex', state: 'idle', since_ms: 1, stale_at_ms: 2}]}))), null);
assert.equal(parse(text(agent({apps: [{id: 'codex', state: 'waiting', since_ms: -1, stale_at_ms: 2}]}))), null);
assert.equal(parse(text(agent({finished_seq: 1.5}))), null);
assert.equal(parse(text(agent({finished_app: 'nope'}))), null);
assert.equal(parse(text(agent({finished_treat: 1}))), null);
assert.equal(parse(text(agent({hello_at_ms: 'now'}))), null);
assert.equal(parse(text(agent({test: {state: 'sleeping', at_ms: 1, client: null}}))), null);
assert.ok(parse(text(agent({test: {state: 'running', at_ms: 1, client: 'grok-cli'}}))));
assert.equal(parse(text(agent({helpers_busy: null}))), null);
assert.equal(parse(text(v2({treats: {jar: 11, full: true}}))), null, 'the jar holds ten');
assert.equal(parse(text(v2({treats: {jar: 3}}))), null);
assert.equal(parse(text(v2({clients: {gemini: {}}}))), null);
assert.equal(parse(text(v2({clients: []}))), null);

// Database identity.
const A = 'db-' + 'a'.repeat(24), B = 'db-' + 'b'.repeat(24);
assert.equal(parse(text(bar({db_id: A}))).db_id, A);
assert.equal(parse(text(bar({db_id: 'db-XYZ'}))), null, 'malformed db_id');
assert.equal(parse(text(bar({db_id: 7}))), null);
assert.ok(sameFileDatabase(parse(text(bar({db_id: A}))), parse(text(bar({db_id: A})))));
assert.ok(!sameFileDatabase(parse(text(bar({db_id: A}))), parse(text(bar({db_id: B})))));
assert.ok(!sameFileDatabase(parse(text(bar())), parse(text(bar({db_id: B})))));
assert.ok(sameFileDatabase(parse(text(bar())), parse(text(bar()))));
// The v0.2 merge into a task snapshot is gone with the window.
for (const gone of ['merge', 'isNewer', 'replacesDatabase'])
  assert.equal(typeof globalThis[gone], 'undefined', gone);
console.log('bar-state v1/v2 parse, v0.3 blocks, XDG path and database identity passed');
"""


def v2_fixtures_js():
    """The frozen bar-v2 fixtures as a JS object literal (for the Node checks)."""
    fixtures = {p.stem: json.loads(p.read_text())
                for p in sorted((REPO / "docs/v0.3/contracts/fixtures/bar-v2").glob("*.json"))}
    return "const V2_FIXTURES = " + json.dumps(fixtures) + ";\n"


class BarStateJs(unittest.TestCase):
    @unittest.skipUnless(shutil.which('node'), 'Node unavailable; BarState.js untested')
    def test_parse_and_paths(self):
        result = subprocess.run(['node', '-e', BAR_STATE_JS + v2_fixtures_js() + BAR_STATE_NODE],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)


def unquoted_expansions(text):
    """(line, snippet) for every `$` expansion outside quotes in a POSIX sh script,
    except `$?`, `$#`, arithmetic `$((...))` and the right side of an assignment."""
    found = []
    stack = ['N']
    i = 0
    line_no = 1
    line_start = 0
    while i < len(text):
        c = text[i]
        top = stack[-1]
        if c == '\n':
            line_no += 1
            line_start = i + 1
        if top == 'S':
            if c == "'":
                stack.pop()
            i += 1
            continue
        if c == '\\':
            i += 2
            continue
        if top == 'D':
            if c == '"':
                stack.pop()
            elif text.startswith('$((', i):
                i = text.index('))', i) + 2
                continue
            elif text.startswith('$(', i):
                stack.append('C')
                i += 2
                continue
            i += 1
            continue
        # N or C (command substitution): unquoted shell context.
        if c == '#' and (i == line_start or text[i - 1] in ' \t;('):
            i = text.index('\n', i)
            continue
        if c == "'":
            stack.append('S')
        elif c == '"':
            stack.append('D')
        elif c == ')' and top == 'C':
            stack.pop()
        elif text.startswith('$((', i):
            i = text.index('))', i) + 2
            continue
        elif text.startswith('$(', i):
            stack.append('C')
            i += 2
            continue
        elif c == '$' and i + 1 < len(text) and text[i + 1] not in '?#':
            prefix = text[line_start:i]
            if not re.search(r'(^|[\s;])[A-Za-z_][A-Za-z0-9_]*=$', prefix):
                found.append((line_no, text[line_start:text.index('\n', i)].strip()))
        i += 1
    return found


class LauncherStatic(unittest.TestCase):
    def test_posix_sh_without_eval(self):
        self.assertTrue(LAUNCHER.startswith('#!/bin/sh\n'))
        self.assertTrue(os.access(LAUNCHER_PATH, os.X_OK), 'launcher must be executable')
        code = '\n'.join(l for l in LAUNCHER.splitlines() if not l.lstrip().startswith('#'))
        self.assertIsNone(re.search(r'\beval\b', code))
        self.assertNotIn('`', code)
        for bashism in ('[[', 'function ', 'local ', '<<<', 'source ', '${!', '$\'', 'declare '):
            self.assertNotIn(bashism, code)
        r = subprocess.run(['sh', '-n', str(LAUNCHER_PATH)], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        for shell in ('dash', 'busybox'):
            exe = shutil.which(shell)
            if exe:
                argv = [exe, 'sh', '-n'] if shell == 'busybox' else [exe, '-n']
                r = subprocess.run([*argv, str(LAUNCHER_PATH)], capture_output=True, text=True)
                self.assertEqual(r.returncode, 0, (shell, r.stderr))

    def test_every_expansion_is_quoted(self):
        self.assertEqual(unquoted_expansions(LAUNCHER), [])
        # The checker itself catches the mistakes it is meant to catch.
        self.assertEqual(len(unquoted_expansions('qs ipc -p $QML call\n')), 1)
        self.assertEqual(len(unquoted_expansions('x=$(qs list $A)\n')), 1)
        self.assertEqual(unquoted_expansions('x=$y\nn=$((n + 1))\nexit $?\necho "$a $(b "$c")"\n'), [])
        self.assertEqual(unquoted_expansions("awk '{ print $0 }'\n"), [])

    def test_bounded_wait_and_single_instance_launch(self):
        self.assertIn('while [ "$n" -lt 30 ]; do', LAUNCHER)
        self.assertIn('sleep 0.1', LAUNCHER)
        self.assertEqual(LAUNCHER.count('" -n -d -p "$QML"'), 1)
        self.assertIn('CODEPET_BIN="$BIN" CODEPET_OPEN="$open" "$QS" -n -d -p "$QML"', LAUNCHER)
        self.assertIn('"$QS" list --all --json', LAUNCHER)
        # `call --` keeps qs 0.3.1 from parsing the `show` verb as `qs ipc show`.
        self.assertEqual(len(re.findall(r'call -- codepet', LAUNCHER)), 4)
        self.assertNotRegex(re.sub(r'#[^\n]*', '', LAUNCHER), r'call codepet')
        self.assertIn('/current/qml/shell.qml', LAUNCHER)
        self.assertIn('${XDG_DATA_HOME:-}', LAUNCHER)
        # hide/refresh/status branches never reach start_overlay.
        dispatch = LAUNCHER[LAUNCHER.index('case "$VERB" in\n    status)'):]
        for verb in ('status)', 'hide|refresh)', 'quit)'):
            branch = dispatch[dispatch.index(verb):]
            branch = branch[:branch.index('    hide|refresh)' if verb == 'quit)' else ';;')]
            self.assertNotIn('start_overlay', branch, verb)
        # `qs kill` appears only in the quit branch, after the running-task check.
        code = re.sub(r'#[^\n]*', '', LAUNCHER)
        self.assertEqual(code.count('kill'), 2, "one `$QS kill` call and its error message")
        quit_branch = dispatch[dispatch.index('quit)'):dispatch.index('    hide|refresh)')]
        self.assertIn('"$QS" kill "$TARGET_FLAG" "$TARGET_VALUE"', quit_branch)
        self.assertLess(quit_branch.index('"supervisor":true'), quit_branch.index('"$QS" kill'))
        # Fail closed: an unknown status needs an explicit --force before the kill.
        self.assertLess(quit_branch.index('"supervisor":false'), quit_branch.index('"$QS" kill'))
        self.assertLess(quit_branch.index('"$FORCE" -ne 1'), quit_branch.index('"$QS" kill'))
        self.assertIn('if [ "$1" = quit ] && [ "$2" = --force ]; then FORCE=1', LAUNCHER)

    @unittest.skipUnless(shutil.which('shellcheck'), 'shellcheck unavailable')
    def test_shellcheck(self):
        r = subprocess.run(['shellcheck', '-s', 'sh', str(LAUNCHER_PATH)],
                           capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)


FAKE_QS = r"""#!{python} -I
# Test double for the Quickshell CLI, driven by a JSON state file. Logs argv and
# the overlay-relevant environment of every call.
import json, os, sys
state_path = os.environ["FAKEQS_STATE"]
with open(state_path) as f:
    st = json.load(f)
args = sys.argv[1:]
with open(os.environ["FAKEQS_LOG"], "a") as f:
    f.write(json.dumps({{"argv": args, "open": os.environ.get("CODEPET_OPEN"),
                         "bin": os.environ.get("CODEPET_BIN")}}) + "\n")
def save():
    with open(state_path, "w") as f:
        json.dump(st, f)
if args[:1] == ["list"]:
    if st["listed"]:
        print(json.dumps(st["listed"], indent=4))
    else:
        print("No running instances.")
    sys.exit(0)
if args[:1] == ["kill"]:
    st["kills"] = st.get("kills", 0) + 1
    st["running"] = False
    st["listed"] = []
    save()
    sys.exit(0)
if args[:1] == ["ipc"]:
    if args[3:5] != ["call", "--"]:
        print("target codepet")  # what real qs 0.3.1 does for `call codepet show`
        sys.exit(0)
    flag, value, fn = args[1], args[2], args[6]
    if flag == "-p":
        if not st["running"]:
            print('No running instances for "%s"' % value); sys.exit(255)
        ok_fns = st["functions"]
    else:
        if int(value) not in [i["pid"] for i in st["listed"]]:
            print("No instance found for pid %s" % value); sys.exit(255)
        ok_fns = st["pid_functions"]
    if fn not in ok_fns:
        print("Function not found."); sys.exit(0)
    if fn == "ping":
        print("codepet-overlay shown")
    elif fn == "status":
        if "status_reply" in st:  # an older overlay's reply, or none at all
            print(st["status_reply"])
        else:
            print('{{"pet":"shown","supervisor":%s}}' % ("true" if st.get("supervisor") else "false"))
    sys.exit(0)
if args[:3] == ["-n", "-d", "-p"]:
    st["launches"] += 1
    if st["ready_after_launch"]:
        st["running"] = True
    save()
    sys.exit(0)
print("unexpected qs call", args, file=sys.stderr)
sys.exit(99)
"""

ALL_FUNCTIONS = ["ping", "status", "show", "hide", "toggle", "tasks", "care", "companion",
                 "life", "refresh"]


class LauncherBehaviour(unittest.TestCase):
    """The real launcher against a fake `qs`: never a second copy, verbs routed."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        root = Path(self.tmp.name)
        self.home = root / "home"
        self.data = self.home / ".local/share"
        self.qml = self.data / "codepet/current/qml/shell.qml"
        self.qml.parent.mkdir(parents=True)
        self.qml.write_text("// stub entry\n")
        self.state = root / "qs-state.json"
        self.log = root / "qs-log.jsonl"
        fake = root / "qs"
        fake.write_text(FAKE_QS.format(python=sys.executable))
        fake.chmod(0o700)
        self.env = {"PATH": "/usr/bin:/bin", "HOME": str(self.home),
                    "XDG_DATA_HOME": str(self.data), "CODEPET_QS": str(fake),
                    "FAKEQS_STATE": str(self.state), "FAKEQS_LOG": str(self.log)}
        self.set_state()

    def tearDown(self):
        self.tmp.cleanup()

    def set_state(self, **over):
        st = {"running": False, "functions": ALL_FUNCTIONS, "listed": [],
              "pid_functions": ALL_FUNCTIONS, "ready_after_launch": True, "launches": 0}
        st.update(over)
        self.state.write_text(json.dumps(st))

    def run_launcher(self, *args, timeout=15):
        if self.log.exists():
            self.log.unlink()
        r = subprocess.run([str(LAUNCHER_PATH), *args], env=self.env, capture_output=True,
                           text=True, timeout=timeout)
        calls = [json.loads(l) for l in self.log.read_text().splitlines()] if self.log.exists() else []
        return r, calls

    def launches(self, calls):
        return [c for c in calls if c["argv"][:3] == ["-n", "-d", "-p"]]

    def ipc_calls(self, calls):
        return [c["argv"][1:3] + [c["argv"][6]] for c in calls
                if c["argv"][:1] == ["ipc"] and c["argv"][6] != "ping"]

    def test_usage_errors(self):
        for argv in ([], ["stop"], ["show", "extra"], ["--pid"], ["show", "--force"],
                     ["--force", "quit"], ["quit", "--now"], ["quit", "--force", "x"]):
            r, calls = self.run_launcher(*argv)
            self.assertEqual(r.returncode, 2, argv)
            self.assertEqual(calls, [], argv)

    def test_not_running_status_hide_refresh_never_start(self):
        for verb, code in (("status", 1), ("hide", 0), ("refresh", 0), ("quit", 0)):
            r, calls = self.run_launcher(verb)
            self.assertEqual(r.returncode, code, (verb, r.stdout, r.stderr))
            self.assertEqual(self.launches(calls), [], verb)
            self.assertEqual(self.ipc_calls(calls), [], verb)
        r, _ = self.run_launcher("status")
        self.assertEqual(r.stdout.strip(), "not running")

    def test_show_starts_once_and_sends_the_verb(self):
        r, calls = self.run_launcher("show")
        self.assertEqual(r.returncode, 0, r.stderr)
        starts = self.launches(calls)
        self.assertEqual(len(starts), 1)
        self.assertEqual(starts[0]["argv"], ["-n", "-d", "-p", str(self.qml)])
        self.assertEqual(starts[0]["open"], "")
        self.assertEqual(starts[0]["bin"], str(self.home / ".local/bin/codepet"))
        self.assertEqual(self.ipc_calls(calls), [["-p", str(self.qml), "show"]])
        # Now running: no further launch.
        r, calls = self.run_launcher("show")
        self.assertEqual(self.launches(calls), [])
        self.assertEqual(self.ipc_calls(calls), [["-p", str(self.qml), "show"]])

    def test_view_verbs_open_through_codepet_open(self):
        for verb in ("tasks", "care", "companion", "life"):
            self.set_state()
            r, calls = self.run_launcher(verb)
            self.assertEqual(r.returncode, 0, (verb, r.stderr))
            starts = self.launches(calls)
            self.assertEqual([s["open"] for s in starts], [verb])
            self.assertEqual(self.ipc_calls(calls), [["-p", str(self.qml), verb]])

    def test_toggle_from_stopped_means_show(self):
        r, calls = self.run_launcher("toggle")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(len(self.launches(calls)), 1)
        self.assertEqual(self.ipc_calls(calls), [["-p", str(self.qml), "show"]])
        r, calls = self.run_launcher("toggle")
        self.assertEqual(self.launches(calls), [])
        self.assertEqual(self.ipc_calls(calls), [["-p", str(self.qml), "toggle"]])

    def test_running_instance_gets_every_verb_without_a_launch(self):
        self.set_state(running=True)
        for verb in ("show", "hide", "toggle", "tasks", "care", "companion", "life", "refresh"):
            r, calls = self.run_launcher(verb)
            self.assertEqual(r.returncode, 0, (verb, r.stderr))
            self.assertEqual(self.launches(calls), [], verb)
            self.assertEqual(self.ipc_calls(calls), [["-p", str(self.qml), verb]], verb)
        r, calls = self.run_launcher("status")
        self.assertEqual(r.returncode, 0)
        self.assertIn("running", r.stdout)
        self.assertIn('{"pet":"shown","supervisor":false}', r.stdout)

    def test_quit_ends_a_running_overlay_but_not_a_running_task(self):
        self.set_state(running=True, supervisor=True)
        r, calls = self.run_launcher("quit")
        self.assertEqual(r.returncode, 1)
        self.assertIn("simulation task is executing", r.stderr)
        self.assertFalse([c for c in calls if c["argv"][:1] == ["kill"]])
        self.set_state(running=True)
        r, calls = self.run_launcher("quit")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual([c["argv"] for c in calls if c["argv"][:1] == ["kill"]],
                         [["kill", "-p", str(self.qml)]])
        self.assertEqual(self.launches(calls), [])
        r, _ = self.run_launcher("status")
        self.assertEqual(r.returncode, 1, "stopped")

    def kills(self, calls):
        return [c["argv"] for c in calls if c["argv"][:1] == ["kill"]]

    def test_quit_refuses_an_overlay_that_does_not_report_its_task(self):
        # 0.2.0's status has no "supervisor" field; an empty reply says nothing either.
        for reply, functions in (('{"pet":"shown","busy":false}', ALL_FUNCTIONS),
                                 ("", ALL_FUNCTIONS),
                                 (None, [f for f in ALL_FUNCTIONS if f != "status"])):
            over = {"running": True, "functions": functions}
            if reply is not None:
                over["status_reply"] = reply
            self.set_state(**over)
            r, calls = self.run_launcher("quit")
            self.assertEqual(r.returncode, 1, (reply, r.stderr))
            self.assertIn("did not report whether a simulation task is executing", r.stderr)
            self.assertIn("quit --force", r.stderr)
            self.assertEqual(self.kills(calls), [], reply)
            # --force: the user has checked that no task runs.
            r, calls = self.run_launcher("quit", "--force")
            self.assertEqual(r.returncode, 0, (reply, r.stderr))
            self.assertEqual(self.kills(calls), [["kill", "-p", str(self.qml)]], reply)
            self.assertEqual(self.launches(calls), [])

    def test_quit_force_never_ends_a_running_task(self):
        self.set_state(running=True, supervisor=True)
        r, calls = self.run_launcher("quit", "--force")
        self.assertEqual(r.returncode, 1)
        self.assertIn("simulation task is executing", r.stderr)
        self.assertEqual(self.kills(calls), [])
        # Not running: nothing to quit, forced or not.
        self.set_state()
        r, calls = self.run_launcher("quit", "--force")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.kills(calls), [])
        self.assertEqual(self.launches(calls), [])

    def test_older_release_instance_is_found_by_pid(self):
        older = str(self.data / "codepet/releases/0.1.9/qml/shell.qml")
        self.set_state(listed=[{"config_path": "/elsewhere/shell.qml", "id": "a", "pid": 111,
                                "launch_time": "t", "shell_id": "s"},
                               {"config_path": older, "id": "b", "pid": 4242,
                                "launch_time": "t", "shell_id": "s"}])
        for verb, sent in (("show", "show"), ("toggle", "toggle"), ("hide", "hide"),
                           ("tasks", "tasks")):
            r, calls = self.run_launcher(verb)
            self.assertEqual(r.returncode, 0, (verb, r.stderr))
            self.assertEqual(self.launches(calls), [], verb)
            self.assertEqual(self.ipc_calls(calls), [["--pid", "4242", sent]], verb)
        r, _ = self.run_launcher("status")
        self.assertEqual(r.returncode, 0)
        self.assertIn("pid 4242", r.stdout)

    def test_current_path_instance_without_ping_is_still_running(self):
        # An overlay older than the ping verb, launched from the `current` path.
        self.set_state(running=True, functions=["show", "hide", "tasks"],
                       pid_functions=["show", "hide", "tasks"],
                       listed=[{"config_path": str(self.qml), "pid": 77}])
        r, calls = self.run_launcher("show")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.launches(calls), [])
        self.assertEqual(self.ipc_calls(calls), [["--pid", "77", "show"]])
        r, calls = self.run_launcher("companion")
        self.assertEqual(r.returncode, 1)
        self.assertIn("older CodePet release", r.stderr)
        self.assertEqual(self.launches(calls), [])

    def test_unrelated_instances_do_not_count(self):
        self.set_state(listed=[{"config_path": str(self.home / ".config/quickshell/shell.qml"),
                                "pid": 5},
                               {"config_path": str(self.data / "codepet-other/current/x.qml"),
                                "pid": 6}])
        r, calls = self.run_launcher("hide")
        self.assertEqual(r.returncode, 0)
        self.assertEqual(self.ipc_calls(calls), [])
        r, calls = self.run_launcher("show")
        self.assertEqual(len(self.launches(calls)), 1)

    def test_readiness_wait_is_bounded(self):
        self.set_state(ready_after_launch=False)
        start = time.monotonic()
        r, calls = self.run_launcher("show")
        elapsed = time.monotonic() - start
        self.assertEqual(r.returncode, 1)
        self.assertIn("did not answer within 3 s", r.stderr)
        self.assertEqual(len(self.launches(calls)), 1)
        self.assertEqual(self.ipc_calls(calls), [])
        self.assertLess(elapsed, 10)
        pings = [c for c in calls if c["argv"][:1] == ["ipc"] and c["argv"][6] == "ping"]
        self.assertLessEqual(len(pings), 31)

    def test_missing_entry_or_qs(self):
        self.qml.unlink()
        r, calls = self.run_launcher("show")
        self.assertEqual(r.returncode, 1)
        self.assertIn("not installed", r.stderr)
        self.assertEqual(self.launches(calls), [])
        env = dict(self.env, CODEPET_QS=str(Path(self.tmp.name) / "no-qs"))
        r = subprocess.run([str(LAUNCHER_PATH), "show"], env=env, capture_output=True, text=True)
        self.assertEqual(r.returncode, 1)
        self.assertIn("Quickshell", r.stderr)

    def test_overrides_and_relative_xdg_data_home(self):
        alt = Path(self.tmp.name) / "alt/shell.qml"
        alt.parent.mkdir()
        alt.write_text("// alt\n")
        self.env.update(CODEPET_OVERLAY_QML=str(alt), CODEPET_BIN="/opt/codepet")
        r, calls = self.run_launcher("show")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(self.launches(calls)[0]["argv"][-1], str(alt))
        self.assertEqual(self.launches(calls)[0]["bin"], "/opt/codepet")
        # A relative XDG_DATA_HOME is ignored in favour of ~/.local/share.
        self.set_state()
        del self.env["CODEPET_OVERLAY_QML"]
        self.env["XDG_DATA_HOME"] = "relative/share"
        r, calls = self.run_launcher("show")
        self.assertEqual(self.launches(calls)[0]["argv"][-1], str(self.qml))


FAKE_EXPORT_CODEPET = """#!{python} -I
# Test-only `codepet rpc`: logs each method to $HOME/rpc.log; state.get "exports" the
# bar-state.json in $HOME/export.json (as Rust would: temp + rename) and answers ok.
import json, os, sys
req = json.loads(sys.stdin.readline())
home = os.environ["HOME"]
with open(os.path.join(home, "rpc.log"), "a") as f:
    f.write(req["method"] + "\\n")
if req["method"] == "state.get":
    src = os.path.join(home, "export.json")
    if os.path.exists(src):
        d = os.path.join(os.environ["XDG_STATE_HOME"], "codepet")
        os.makedirs(d, exist_ok=True)
        tmp = os.path.join(d, ".bar-state.json.tmp")
        with open(src) as f, open(tmp, "w") as g:
            g.write(f.read())
        os.rename(tmp, os.path.join(d, "bar-state.json"))
    print(json.dumps({{"v": 1, "id": req["id"], "ok": True, "result": {{}}}}))
else:
    print(json.dumps({{"v": 1, "id": req["id"], "ok": False, "error": {{
        "code": "STORAGE_UNAVAILABLE", "message": "stub", "retryable": False}}}}))
"""

DB_A = "db-" + "a" * 24
FIXTURES = REPO / "docs/v0.3/contracts/fixtures/bar-v2"


def bar_state_text(version, appearance, db_id=DB_A, fixture="idle-happy", schema="codepet.bar.v2", **over):
    """A frozen v0.3 export (bar-v2 fixture) with this version, look and database."""
    doc = json.loads((FIXTURES / f"{fixture}.json").read_text())
    doc.update(schema=schema, state_version=version, db_id=db_id, **over)
    if doc["life"] is not None:
        doc["life"]["appearance"] = appearance
    return json.dumps(doc)


def shell_json(overlay_on_load, calm=None):
    """~/.config/omarchy/shell.json with the CodePet cell; None = key absent."""
    entry = {"id": "greyforge.codepet", "label": "name"}
    if overlay_on_load is not None:
        entry["overlayOnLoad"] = overlay_on_load
    if calm is not None:
        entry["calm"] = calm
    return json.dumps({"version": 1, "bar": {"layout": {"left": [], "center": [],
                                                        "right": [{"id": "omarchy.clock"}, entry]}}})


def write_atomically(path, text):
    """The writer's pattern (SPEC-v0.3 §8 rule 2): a temp file in the same directory, then rename."""
    tmp = path.with_name(f"{path.name}.tmp-{os.getpid()}-0badc0de")
    tmp.write_text(text)
    os.rename(tmp, path)


class QsSession:
    """One isolated `qs` instance of a staged entry, stopped and reaped on exit."""

    def __init__(self, root, env, entry):
        self.env, self.entry = env, entry
        self.log = (root / "qs.log").open("w+")
        self.proc = subprocess.Popen(["qs", "-p", str(entry)], env=env, stdout=self.log,
                                     stderr=subprocess.STDOUT)

    def call(self, *args):
        return subprocess.run(["qs", "ipc", "-p", str(self.entry), "call", "--", "codepet", *args],
                              env=self.env, capture_output=True, text=True, timeout=10)

    def status(self):
        if self.proc.poll() is not None:
            return {}
        r = self.call("status")
        try:
            return json.loads(r.stdout)
        except ValueError:
            return {}

    def wait_for(self, predicate, seconds=10):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline and self.proc.poll() is None:
            st = self.status()
            if predicate(st):
                return st
            time.sleep(0.1)
        return self.status()

    def wait_exit(self, seconds=10):
        try:
            return self.proc.wait(timeout=seconds)
        except subprocess.TimeoutExpired:
            return None

    def close(self):
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()
        self.log.seek(0)
        out = self.log.read()
        self.log.close()
        return out


@unittest.skipUnless(shutil.which("qs"), "Quickshell (qs) unavailable; overlay runtime untested")
class QuickshellOverlayRuntime(unittest.TestCase):
    """Real Quickshell 0.3.1, offscreen and isolated (see isolated_env). Not Wayland proof."""

    def export_env(self, root, export=None):
        env = isolated_env(root)
        fake = root / "bin/codepet"
        fake.write_text(FAKE_EXPORT_CODEPET.format(python=sys.executable))
        if export is not None:
            (Path(env["HOME"]) / "export.json").write_text(export)
        return env

    def state_file(self, env):
        d = Path(env["XDG_STATE_HOME"]) / "codepet"
        d.mkdir(parents=True, exist_ok=True)
        return d / "bar-state.json"

    def settings_file(self, env):
        d = Path(env["HOME"]) / ".config/omarchy"
        d.mkdir(parents=True, exist_ok=True)
        return d / "shell.json"

    def rpc_log(self, env):
        path = Path(env["HOME"]) / "rpc.log"
        return path.read_text().split() if path.exists() else []

    def shell_calls(self, env):
        path = Path(env["HOME"]) / "omarchy-shell.log"
        return [json.loads(l) for l in path.read_text().splitlines()] if path.exists() else []

    def test_startup_with_a_file_reads_it_and_sends_no_rpc(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = self.export_env(root)
            write_atomically(self.state_file(env), bar_state_text(9, "axolotl"))
            env["CODEPET_OPEN"] = "tasks"
            session = QsSession(root, env, stage_without_layer_shell(root))
            try:
                st = session.wait_for(lambda s: s.get("state_version") == 9)
                self.assertEqual((st.get("appearance"), st.get("enabled"), st.get("drawn")), ("axolotl", True, True), st)
                self.assertEqual(st.get("home_requests"), 0, "CODEPET_OPEN alone opens nothing")
                time.sleep(0.5)
                self.assertEqual(self.rpc_log(env), [], "a readable file needs no state.get")
                self.assertTrue(session.call("ping").stdout.startswith("codepet-overlay shown"))
                # A burst (a child mashing the pet) is one Omarchy call; a later one is another.
                for verb in ("openHome", "panel", "care", "openHome", "openHome"):
                    self.assertEqual(session.call(verb).returncode, 0, verb)
                st = session.wait_for(lambda s: s.get("home_requests") == 1)
                self.assertEqual(st.get("home_requests"), 1, st)
                time.sleep(1.1)
                self.assertEqual(session.call("panel").returncode, 0)
                st = session.wait_for(lambda s: s.get("home_requests") == 2)
                self.assertEqual(st.get("home_requests"), 2, st)
                time.sleep(0.3)
                self.assertEqual(self.shell_calls(env), [["-q", "shell", "summon", "greyforge.codepet"]] * 2,
                                 "each opens Home through the Omarchy shell")
                env_log = (Path(env["HOME"]) / "omarchy-shell.env.log").read_text().split()
                self.assertEqual(env_log, ['"/usr/share/omarchy"'] * 2,
                                 "without OMARCHY_PATH the packaged one is passed (omarchy-shell needs it)")
                session.call("toggle")
                self.assertEqual(session.wait_for(lambda s: s.get("pet") == "hidden").get("pet"), "hidden")
                self.assertTrue(session.call("ping").stdout.startswith("codepet-overlay hidden"))
                session.call("toggle")
                self.assertEqual(session.wait_for(lambda s: s.get("pet") == "shown").get("pet"), "shown")
                session.call("refresh")
                session.wait_for(lambda s: not s.get("busy"))
                time.sleep(0.3)
                self.assertEqual(self.rpc_log(env), ["state.get"], "refresh is one explicit state.get")
            finally:
                out = session.close()
        self.assertIn("Configuration Loaded", out, out)
        self.assertIsNone(LOAD_ERRORS.search(out), out)
        self.assertNotRegex(out, r"TypeError|ReferenceError|Cannot assign|Binding loop", out)

    def test_startup_without_a_file_reads_once(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = self.export_env(root, export=bar_state_text(5, "cat"))
            session = QsSession(root, env, stage_without_layer_shell(root))
            try:
                st = session.wait_for(lambda s: s.get("state_version") == 5)
                self.assertEqual((st.get("state_version"), st.get("appearance")), (5, "cat"), st)
                time.sleep(0.5)
                self.assertEqual(self.rpc_log(env), ["state.get"], "exactly one startup read")
            finally:
                out = session.close()
        self.assertIsNone(LOAD_ERRORS.search(out), out)

    def test_bar_state_reaches_the_pet_without_rpc(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = self.export_env(root)
            bar = self.state_file(env)
            write_atomically(bar, bar_state_text(6, "robot"))
            session = QsSession(root, env, stage_without_layer_shell(root))
            try:
                st = session.wait_for(lambda s: s.get("state_version") == 6)
                self.assertEqual((st.get("state_version"), st.get("appearance")), (6, "robot"), st)
                for version, look in ((7, "owl"), (8, "frog")):
                    write_atomically(bar, bar_state_text(version, look))
                    st = session.wait_for(lambda s, v=version: s.get("state_version") == v)
                    self.assertEqual((st.get("state_version"), st.get("appearance")), (version, look))
                # A v1-labelled export (the v0.3.0 writer) and a v0.3 face from the file.
                write_atomically(bar, bar_state_text(9, "cat", schema="codepet.bar.v1", fixture="waiting"))
                st = session.wait_for(lambda s: s.get("state_version") == 9)
                self.assertEqual(st.get("appearance"), "cat", st)
                # Older, malformed and oversized files are ignored.
                write_atomically(bar, bar_state_text(3, "dog"))
                write_atomically(bar, "{broken")
                write_atomically(bar, bar_state_text(10, "dog").replace('"codepet.bar.v2"', '"x"'))
                write_atomically(bar, bar_state_text(10, "dog")[:-1] + ', "pad": "' + "x" * 17000 + '"}')
                time.sleep(1.0)
                st = session.status()
                self.assertEqual((st.get("state_version"), st.get("appearance")), (9, "cat"), st)
                # A restored or recreated database restarts its counter.
                write_atomically(bar, bar_state_text(2, "penguin", db_id="db-" + "b" * 24))
                st = session.wait_for(lambda s: s.get("appearance") == "penguin")
                self.assertEqual((st.get("state_version"), st.get("appearance")), (2, "penguin"), st)
                write_atomically(bar, bar_state_text(1, "dog", db_id="db-" + "b" * 24))
                time.sleep(1.0)
                self.assertEqual(session.status().get("appearance"), "penguin")
                # No pet: nothing drawn, the process stays (Let's go! comes from the popup).
                write_atomically(bar, bar_state_text(3, "cat", db_id="db-" + "b" * 24, fixture="fresh-no-pet"))
                st = session.wait_for(lambda s: s.get("state_version") == 3)
                self.assertEqual((st.get("face"), st.get("drawn")), ("no_pet", False), st)
                self.assertEqual(self.rpc_log(env), [], "file events never launch codepet")
            finally:
                out = session.close()
        self.assertIsNone(LOAD_ERRORS.search(out), out)
        self.assertNotRegex(out, r"TypeError|ReferenceError|Binding loop", out)

    def test_off_ends_the_process(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = self.export_env(root)
            bar = self.state_file(env)
            write_atomically(bar, bar_state_text(4, "cat"))
            entry = stage_without_layer_shell(root)
            session = QsSession(root, env, entry)
            try:
                self.assertEqual(session.wait_for(lambda s: s.get("state_version") == 4).get("drawn"), True)
                write_atomically(bar, bar_state_text(5, "cat", fixture="off"))
                self.assertEqual(session.wait_exit(), 0, "Off: the overlay process exits")
            finally:
                out = session.close()
            self.assertIn("exiting (off)", out)
            self.assertEqual(self.rpc_log(env), [])
            # Started while Off: it reads the file and exits at once.
            session = QsSession(root, env, entry)
            try:
                self.assertEqual(session.wait_exit(), 0)
            finally:
                session.close()
            self.assertEqual(self.rpc_log(env), [], "and never runs codepet")

    def test_pet_on_my_desktop_off_ends_the_process(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = self.export_env(root)
            write_atomically(self.state_file(env), bar_state_text(4, "cat"))
            settings = self.settings_file(env)
            write_atomically(settings, shell_json(True))
            entry = stage_without_layer_shell(root)
            session = QsSession(root, env, entry)
            try:
                st = session.wait_for(lambda s: s.get("desktop_pet") is True)
                self.assertEqual((st.get("desktop_pet"), st.get("drawn")), (True, True), st)
                write_atomically(settings, shell_json(False))
                self.assertEqual(session.wait_exit(), 0, '"Pet on my desktop" off: the process exits')
            finally:
                out = session.close()
            self.assertIn("exiting (desktop_pet_off)", out)
            # The v0.2 default (no key) is off too, and so is a bar without the CodePet cell
            # (removed in Omarchy's bar settings without the uninstall): both exit after the
            # settle time.
            for text in (shell_json(None), json.dumps({"version": 1, "bar": {"layout": {"right": []}}})):
                write_atomically(settings, text)
                session = QsSession(root, env, entry)
                try:
                    st = session.wait_for(lambda s: s.get("exit_reason") == "desktop_pet_off")
                    self.assertEqual((st.get("desktop_pet"), st.get("drawn")), (False, False), st)
                    self.assertEqual(session.wait_exit(), 0, text)
                finally:
                    session.close()
            # No usable shell.json (a standalone overlay): unknown, it keeps running.
            for text in ("{broken", json.dumps({"bar": {"layout": {"right": []}}})):
                write_atomically(settings, text)
                session = QsSession(root, env, entry)
                try:
                    st = session.wait_for(lambda s: s.get("state_version") == 4)
                    time.sleep(2.0)
                    st = session.status()
                    self.assertEqual((st.get("desktop_pet"), st.get("drawn"), st.get("exit_reason")),
                                     (None, True, ""), (text, st))
                finally:
                    session.close()
            # A shell.json under XDG_CONFIG_HOME is not the one Omarchy uses: ignored.
            settings.unlink()
            other = Path(env["XDG_CONFIG_HOME"]) / "omarchy/shell.json"
            other.parent.mkdir(parents=True, exist_ok=True)
            write_atomically(other, shell_json(False))
            session = QsSession(root, env, entry)
            try:
                session.wait_for(lambda s: s.get("state_version") == 4)
                time.sleep(2.0)
                st = session.status()
                self.assertEqual((st.get("desktop_pet"), st.get("drawn")), (None, True), st)
            finally:
                session.close()

    def test_a_launch_racing_the_switch_write_keeps_the_pet(self):
        """Let's go! writes overlayOnLoad=true and launches `show`; Omarchy's write does not
        block, so the new process can read the old false first. It must not exit on it."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = self.export_env(root)
            write_atomically(self.state_file(env), bar_state_text(4, "cat"))
            settings = self.settings_file(env)
            write_atomically(settings, shell_json(False))
            entry = stage_without_layer_shell(root)
            session = QsSession(root, env, entry)
            try:
                st = session.wait_for(lambda s: s.get("exit_reason") == "desktop_pet_off")
                self.assertEqual(st.get("desktop_pet"), False, st)
                time.sleep(0.2)
                write_atomically(settings, shell_json(True))
                st = session.wait_for(lambda s: s.get("desktop_pet") is True)
                self.assertEqual((st.get("exit_reason"), st.get("drawn")), ("", True), st)
                time.sleep(2.0)
                self.assertIsNone(session.proc.poll(), "the settled switch is on: still running")
                # Now seen on: switching it off exits at once (no settle wait).
                write_atomically(settings, shell_json(False))
                self.assertEqual(session.wait_exit(1.2), 0)
            finally:
                out = session.close()
            self.assertEqual(out.count("exiting (desktop_pet_off)"), 1, out)

    def test_calm_mode_follows_the_bar_setting_live(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = self.export_env(root)
            write_atomically(self.state_file(env), bar_state_text(4, "cat"))
            settings = self.settings_file(env)
            write_atomically(settings, shell_json(True))
            entry = stage_without_layer_shell(root)
            env["CODEPET_REDUCED_MOTION"] = "1"
            session = QsSession(root, env, entry)
            try:
                st = session.wait_for(lambda s: s.get("desktop_pet") is True)
                self.assertEqual(st.get("calm"), False, "the live setting (missing = off) beats the launch env")
                write_atomically(settings, shell_json(True, calm=True))
                self.assertEqual(session.wait_for(lambda s: s.get("calm") is True).get("calm"), True)
                session.call("setReducedMotion", "false")
                self.assertEqual(session.wait_for(lambda s: s.get("calm") is False).get("calm"), False,
                                 "an explicit call holds")
                write_atomically(settings, shell_json(True, calm=False))
                time.sleep(0.5)
                write_atomically(settings, shell_json(True, calm=True))
                self.assertEqual(session.wait_for(lambda s: s.get("calm") is True).get("calm"), True,
                                 "until the setting changes")
                self.assertEqual(session.status().get("drawn"), True)
            finally:
                out = session.close()
            self.assertNotRegex(out, r"TypeError|ReferenceError|Binding loop", out)
            # No usable shell.json: CODEPET_REDUCED_MOTION=1 from the launch.
            settings.unlink()
            session = QsSession(root, env, entry)
            try:
                st = session.wait_for(lambda s: s.get("state_version") == 4)
                self.assertEqual((st.get("desktop_pet"), st.get("calm")), (None, True), st)
            finally:
                session.close()

    def test_host_adapter_harness(self):
        """qml/tests/quickshell/shell.qml: the explicit transport and its failures."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = isolated_env(root)
            stage = root / "harness"
            stage.mkdir()
            shutil.copytree(QML / "omarchy", stage / "omarchy")
            shutil.copy(QML / "tests/quickshell/shell.qml", stage / "shell.qml")
            shutil.copy(QML / "tests/quickshell/fake_rpc.py", stage / "fake_rpc.py")
            try:
                r = subprocess.run(["qs", "-p", str(stage / "shell.qml")], env=env,
                                   capture_output=True, text=True, timeout=60)
                out = r.stdout + r.stderr
            except subprocess.TimeoutExpired as e:
                out = (e.stdout or b"").decode() + (e.stderr or b"").decode()
                self.fail("harness hung:\n" + out[-4000:])
        self.assertIn("DONE failures=0", out, out[-4000:])
        self.assertNotIn("FAIL ", out)
        self.assertEqual(r.returncode, 0, out[-4000:])

    @unittest.skipUnless(os.environ.get("CODEPET_TEST_BIN"),
                         "set CODEPET_TEST_BIN to a built codepet to run real_rpc.qml")
    def test_real_rpc_harness(self):
        """qml/tests/quickshell/real_rpc.qml: the real `codepet rpc` in a scratch state home."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = isolated_env(root)
            stage = root / "real"
            stage.mkdir()
            shutil.copytree(QML / "omarchy", stage / "omarchy")
            shutil.copytree(QML / "shared", stage / "shared")
            shutil.copy(QML / "tests/quickshell/real_rpc.qml", stage / "shell.qml")
            test_root = root / "rpcroot"
            (test_root / "home").mkdir(parents=True)
            env.update(CODEPET_BIN=os.environ["CODEPET_TEST_BIN"], CODEPET_TEST_ROOT=str(test_root))
            r = subprocess.run(["qs", "-p", str(stage / "shell.qml")], env=env,
                               capture_output=True, text=True, timeout=120)
        out = r.stdout + r.stderr
        self.assertIn("DONE failures=0", out, out[-4000:])
        self.assertNotIn("FAIL ", out, out[-4000:])

    def test_launcher_drives_one_real_instance(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = self.export_env(root)
            write_atomically(self.state_file(env), bar_state_text(5, "cat"))
            entry = stage_without_layer_shell(root)
            data = Path(env["XDG_DATA_HOME"]) / "codepet"
            for version in ("0.2.0", "0.3.0"):
                shutil.copytree(entry.parent, data / "releases" / version / "qml")
            current = data / "current"
            current.symlink_to(data / "releases/0.2.0")
            env["PATH"] = f"{root / 'bin'}:/usr/bin:/bin"

            def launcher(*args):
                return subprocess.run([str(LAUNCHER_PATH), *args], env=env, capture_output=True,
                                      text=True, timeout=30)

            def instances():
                out = subprocess.run(["qs", "list", "--all", "--json"], env=env,
                                     capture_output=True, text=True).stdout
                return json.loads(out) if out.lstrip().startswith("[") else []

            def overlay_status():
                r = launcher("status")
                tail = r.stdout.strip().splitlines()[-1] if r.stdout.strip() else ""
                try:
                    return r.returncode, json.loads(tail)
                except ValueError:
                    return r.returncode, {}

            try:
                self.assertEqual(launcher("status").returncode, 1)
                self.assertEqual(launcher("hide").returncode, 0)
                self.assertEqual(instances(), [], "hide never starts the overlay")
                r = launcher("tasks")
                self.assertEqual(r.returncode, 0, r.stderr)
                self.assertEqual(len(instances()), 1)
                code, st = overlay_status()
                self.assertEqual(code, 0)
                # A v0.2 window verb opens the bar popup, once.
                self.assertEqual(st.get("home_requests"), 1, st)
                time.sleep(1.1)  # Home requests within a second are coalesced
                for verb in ("show", "toggle", "care", "show"):
                    r = launcher(verb)
                    self.assertEqual(r.returncode, 0, (verb, r.stderr))
                self.assertEqual(len(instances()), 1, "verbs never start a second copy")
                code, st = overlay_status()
                self.assertEqual((st.get("pet"), st.get("home_requests")), ("shown", 2), st)
                # An install re-points `current`; the running overlay is still the one.
                current.unlink()
                current.symlink_to(data / "releases/0.3.0")
                r = launcher("hide")
                self.assertEqual(r.returncode, 0, r.stderr)
                self.assertEqual(overlay_status()[1].get("pet"), "hidden")
                self.assertEqual(len(instances()), 1)
                # An overlay started from a releases/ path (older launcher or manual
                # start) is found through `qs list` and addressed by pid.
                first = instances()[0]["pid"]
                os.kill(first, 15)
                deadline = time.monotonic() + 10
                while instances() and time.monotonic() < deadline:
                    time.sleep(0.1)
                old_entry = data / "releases/0.2.0/qml/shell.qml"
                subprocess.run(["qs", "-n", "-d", "-p", str(old_entry)], env=env,
                               capture_output=True, text=True, timeout=30)
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    ping = subprocess.run(["qs", "ipc", "-p", str(old_entry), "call", "--",
                                           "codepet", "ping"], env=env, capture_output=True,
                                          text=True)
                    if ping.stdout.startswith("codepet-overlay"):
                        break
                    time.sleep(0.1)
                r = launcher("status")
                self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
                self.assertIn("pid", r.stdout)
                r = launcher("show")
                self.assertEqual(r.returncode, 0, r.stderr)
                listed = instances()
                self.assertEqual(len(listed), 1, listed)
                self.assertTrue(listed[0]["config_path"].startswith(str(data / "releases/")))
            finally:
                for inst in instances():
                    try:
                        os.kill(inst["pid"], 15)
                    except ProcessLookupError:
                        pass
                deadline = time.monotonic() + 10
                while instances() and time.monotonic() < deadline:
                    time.sleep(0.1)

if __name__ == '__main__':
    unittest.main(verbosity=2)
