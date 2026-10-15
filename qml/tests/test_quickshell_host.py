#!/usr/bin/env python3
"""Source contract + pure JS policy tests, plus real Quickshell load smokes when `qs` is
installed. NOT Wayland runtime proof: the load smokes run offscreen with no compositor.
Also covers the passive bar-state.json reader (BarState.js, BarStateWatcher.qml) and the
packaging/omarchy/codepet-overlay verb launcher (static checks, a fake `qs`, and, when
Quickshell is installed, a real isolated instance).
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

    def test_hit_mask_has_only_the_interactive_bounded_regions(self):
        mask = CODE.split('mask: Region {', 1)[1].split('DesktopPet {', 1)[0]
        self.assertEqual(mask.count('Region {'), 2)
        self.assertEqual(mask.count('Intersection.Combine'), 3)
        self.assertNotIn('item:', mask)
        self.assertNotIn('mask: null', CODE)
        for name in ('pet', 'menu'):
            self.assertIn(f'OverlayPolicy.clipHit(pet.{name}Hit, width, height, shell.surfaceActive)', CODE)
            self.assertIn(f'width: overlay.{name}Input.width', mask)
        # Decorative props have no handler; a prop region would be a dead click zone.
        self.assertNotIn('propsHit', CODE)
        self.assertNotIn('propsInput', CODE)

    def test_single_screen_and_shared_transport(self):
        self.assertEqual(CODE.count('PanelWindow {'), 1)
        self.assertNotIn('Variants {', CODE)
        self.assertNotIn('Loader {', CODE)
        self.assertEqual(CODE.count('HostAdapter {'), 1)
        self.assertEqual(CODE.count('host: hostAdapter'), 2)
        self.assertLess(CODE.index('HostAdapter {'), CODE.index('PanelWindow {'))
        self.assertEqual(CODE.count('screen: shell.selectedScreen'), 2)

    def test_task_window_closing_does_not_terminate_work(self):
        self.assertIn('property bool taskWindowOpen: false', CODE)
        self.assertIn('onClosed: shell.taskWindowOpen = false', CODE)
        self.assertIn('showPetCare: true', CODE)
        for forbidden in ('Qt.quit(', 'Qt.exit(', 'destroy(', 'task.cancel', 'task.start', 'Timer {'):
            self.assertNotIn(forbidden, CODE)
        self.assertNotIn('parentWindow:', CODE)

    def test_suppression_and_explicit_restore(self):
        for name in ('sessionLocked', 'sessionSuspended', 'selectedScreenFullscreen'):
            self.assertIn(f'property bool {name}: false', CODE)
        self.assertIn('!explicitlyHidden && !sessionBlocked', CODE)
        self.assertIn('!selectedScreenFullscreen', CODE)
        self.assertIn('surfaceActive: shell.surfaceActive && overlay.visible', CODE)
        self.assertIn('function show(): void { shell.explicitlyHidden = false }', CODE)
        self.assertNotIn('WlSessionLock', CODE)
        self.assertIn('onReducedMotionRequested: enabled => shell.reducedMotion = enabled', CODE)

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
    home = root / "home"
    return {
        "PATH": f"{bin_dir}:/usr/bin:/bin",
        "HOME": str(home),
        "XDG_RUNTIME_DIR": str(run),
        "XDG_CONFIG_HOME": str(home / ".config"),
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
                for call in (["show"], ["hide"], ["tasks"], ["closeTasks"],
                             ["setReducedMotion", "true"], ["setFullscreen", "false"]):
                    r = ipc(*call)
                    self.assertEqual(r.returncode, 0, (call, r.stdout, r.stderr))
                    self.assertNotIn("target codepet", r.stdout, call)
                    self.assertNotIn("Function not found", r.stdout, call)
                self.assertEqual(json.loads(ipc("status").stdout)["window"], False)
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


class HostContractV02(unittest.TestCase):
    """SPEC-v0.2 §13 host entry and §12 host-contract additions (source contracts)."""

    def test_ipc_functions_are_typed_and_complete(self):
        ipc = block(CODE, 'IpcHandler {')
        declared = re.findall(r'function (\w+)\(([^)]*)\)\s*(?::\s*(\w+))?', ipc)
        self.assertTrue(declared)
        for name, _args, ret in declared:
            self.assertTrue(ret, f'IPC function {name} needs a return type for qs ipc')
        names = {name: ret for name, _args, ret in declared}
        self.assertEqual(names.get('ping'), 'string')
        self.assertEqual(names.get('status'), 'string')
        for verb in ('show', 'hide', 'toggle', 'tasks', 'care', 'companion', 'life', 'refresh',
                     'closeTasks', 'setReducedMotion', 'selectScreen', 'setSessionState',
                     'setFullscreen'):
            self.assertEqual(names.get(verb), 'void', verb)
        # ping's fixed prefix is what the launcher's liveness check matches.
        self.assertIn('"codepet-overlay " +', ipc)
        self.assertIn('TOKEN="codepet-overlay"', LAUNCHER)
        # IPC properties would make IpcHandler expose change signals; status is a function.
        self.assertNotIn('property', ipc)

    def test_codepet_bin_and_open_are_honoured(self):
        self.assertIn('command: [Quickshell.env("CODEPET_BIN") || "codepet", "rpc"]', CODE)
        self.assertIn('readonly property var views: ["care", "tasks", "companion", "life"]', CODE)
        onc = block(CODE, 'Component.onCompleted: {')
        self.assertEqual(onc.count('request('), 1, 'exactly one startup read')
        self.assertIn('Quickshell.env("CODEPET_OPEN")', onc)
        self.assertIn('shell.views.indexOf(open) >= 0', onc)
        # The read starts first, so opening the window cannot add a second state.get.
        self.assertLess(onc.index('request("state.get"'), onc.index('shell.openView(open)'))
        view = block(CODE, 'function openView(name) {')
        self.assertIn('shell.views.indexOf(name) < 0', view)
        self.assertIn('if (!hostAdapter.busy)', view)

    def test_reduced_motion_reaches_the_task_window(self):
        # CODEPET_REDUCED_MOTION, the setReducedMotion IPC verb and the pet menu's
        # "Less motion" all set shell.reducedMotion; the Care tab must honour it too.
        panel = block(CODE, 'Panel {')
        self.assertIn('reducedMotion: shell.reducedMotion', panel)
        self.assertIn('reducedMotion: shell.reducedMotion', block(CODE, 'DesktopPet {'))
        panel_qml = (QML / "shared/Panel.qml").read_text()
        self.assertIn('reducedMotion: root.reducedMotion', block(panel_qml, 'PetCare {'))
        preview = (QML / "standalone/OverlayPreview.qml").read_text()
        self.assertIn('reducedMotion: root.reducedMotion', block(preview, 'Panel {'))

    def test_window_has_the_four_tabs(self):
        tabs = block(CODE, 'TabBar {')
        self.assertIn('model: ["Care", "Tasks", "Companion", "Life"]', tabs)
        self.assertIn('shell.panelView = shell.views[index]', tabs)
        self.assertIn('title: "CodePet"', CODE)
        # Shared components can ask for views; DesktopPet's chooser opens Companion.
        self.assertIn('onCompanionRequested: shell.openView("companion")', CODE)
        self.assertIn('onLifeRequested: shell.openView("life")', CODE)
        self.assertIn('function onChooserRequested() { shell.openView("companion") }', CODE)
        self.assertIn('ignoreUnknownSignals: true', CODE)

    def test_refresh_is_one_explicit_read(self):
        fn = block(CODE, 'function refresh() {')
        self.assertIn('if (!hostAdapter.busy)', fn)
        self.assertEqual(fn.count('request('), 1)
        self.assertIn('function refresh(): void { shell.refresh() }', CODE)

    def test_bar_state_watcher_is_passive(self):
        self.assertIn('watchChanges: true', WATCHER)
        self.assertIn('onFileChanged: reload()', WATCHER)
        self.assertIn('FileView {', WATCHER)
        for forbidden in ('request(', 'host', 'Host', 'Process', 'Timer', 'execDetached',
                          'setText', 'Quickshell.execDetached', 'callLater'):
            self.assertNotIn(forbidden, WATCHER)
        self.assertIn('BarStateWatcher {', CODE)
        self.assertIn('onUpdated: state => hostAdapter.applyBarState(state)', CODE)
        self.assertIn('BarState.statePath(Quickshell.env("XDG_STATE_HOME"), Quickshell.env("HOME"))',
                      CODE)
        # File events lead only to parsing and merging; never to a request.
        self.assertNotIn('request(', block(ADAPTER, 'function applyBarState(bar) {'))
        self.assertIn('onSnapshotReplaced: barWatcher.reload()', CODE)

    def test_host_contract_additions(self):
        for token in ('property string clientTag: "qs"', 'property int presentationLocks: 0',
                      'signal companionRequested()', 'signal lifeRequested()',
                      'signal snapshotReplaced()', 'signal taskAccepted(var outcome)',
                      'function openCompanion() {',
                      'function openLife() {', 'return root.clientTag + "-" +'):
            self.assertIn(token, ADAPTER)
        self.assertIn('clientTag: "overlay"', CODE)
        for fn in ('function openCompanion() {', 'function openLife() {'):
            self.assertNotIn('request(', block(ADAPTER, fn))
        # The accepting UI announces its own acceptance through the host (spec D11), after
        # the follow-up state.get, with the real task.accept_artifact result.
        review = (QML / "shared/TaskReview.qml").read_text()
        accept = block(review, 'function acceptClicked() {')
        self.assertIn('root.host.taskAccepted(outcome)', accept)
        self.assertLess(accept.index('"state.get"'), accept.index('root.host.taskAccepted(outcome)'))
        pet = (QML / "shared/DesktopPet.qml").read_text()
        self.assertIn('function onTaskAccepted(outcome) { root.celebrateFromAcceptance(outcome) }', pet)
        self.assertIn('receipt.evolved_to', block(pet, 'function celebrateFromAcceptance(outcome) {'))


BAR_STATE_NODE = r"""
const assert = require('assert');
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
function snap() {
  return {schema_version: 4, state_version: 800,
    meters: {hunger: 7000, thirst: 7000, mental_health: 7000, hygiene: 7000},
    care: {revision: 3, sleeping: false, sleep_due: false, bathroom_due: true, sleep_remaining_ms: 0,
           sleep_started_at_ms: null, ready_to_work: true, readiness_reasons: [], work_sessions: 2,
           welcome_care_available: false},
    decay: {enabled: true, paused: true},
    life: {life_id: 'life-aaaaaaaaaaaaaaaaaaaaaaaa', ordinal: 1, name: 'Ginger', species: 'cat',
           appearance: 'cat', stage: 1, level: 1, xp: 60, status: 'healthy', revision: 4,
           origin: 'migrated', next_stage: {stage: 2, min_level: 2, xp: 80}},
    lifecycle: {ruleset: 'lifecycle-v1', frozen_reason: 'none',
      needs: [{need: 'hunger', action: 'food', lethal: true, meter: 7000, state: 'ok'},
              {need: 'thirst', action: 'water', lethal: true, meter: 7000, state: 'ok',
               until_warning_ms: 5, until_critical_ms: 1, until_end_ms: 2}],
      critical: null},
    active_task: {task_id: 't-1', revision: 2, state: 'preparing', category: 'hunger',
                  instructions_excerpt: 'kept'},
    recent: [{task_id: 't-0'}], unseen_events: [],
    memorial: {count: 1, latest: {name: 'Biscuit', ordinal: 0}}};
}
const text = (o) => JSON.stringify(o);

// statePath: XDG rules.
assert.equal(statePath('/x/state', '/home/u'), '/x/state/codepet/bar-state.json');
assert.equal(statePath('/x/state/', '/home/u'), '/x/state/codepet/bar-state.json');
assert.equal(statePath('', '/home/u'), '/home/u/.local/state/codepet/bar-state.json');
assert.equal(statePath(undefined, '/home/u/'), '/home/u/.local/state/codepet/bar-state.json');
assert.equal(statePath('relative/state', '/home/u'), '/home/u/.local/state/codepet/bar-state.json');
assert.equal(statePath('', ''), '');
assert.equal(statePath(null, 'relative'), '');

// parse: bounds, schema and shape; anything off is ignored as a whole.
assert.equal(parse(text(bar())).state_version, 812);
assert.equal(parse(''), null);
assert.equal(parse(null), null);
assert.equal(parse('{not json'), null);
assert.equal(parse(text(bar({schema: 'codepet.bar.v2'}))), null);
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
assert.equal(parse(text(bar({meters: {hunger: 1, thirst: 0, mental_health: 0}}))), null);
assert.equal(parse(text(bar({life: Object.assign(bar().life, {appearance: '../x'})}))), null);
assert.equal(parse(text(bar({life: Object.assign(bar().life, {appearance: 'Cat'})}))), null);
assert.equal(parse(text(bar({life: Object.assign(bar().life, {stage: 4})}))), null);
assert.equal(parse(text(bar({life: Object.assign(bar().life, {stage: 0})}))), null);
assert.equal(parse(text(bar({life: Object.assign(bar().life, {paused: 'no'})}))), null);
assert.equal(parse(text(bar({life: Object.assign(bar().life, {status: 'Not A Token'})}))), null);
assert.equal(parse(text(bar({life: Object.assign(bar().life, {name: 'n'.repeat(97)})}))), null);
assert.equal(parse(text(bar({life: null}))).life, null);
assert.equal(parse(text(bar({care: Object.assign(bar().care, {readiness_reasons: Array(9).fill('a')})}))), null);
assert.equal(parse(text(bar({care: Object.assign(bar().care, {readiness_reasons: ['r'.repeat(161)]})}))), null);
assert.equal(parse(text(bar({care: Object.assign(bar().care, {sleeping: 1})}))), null);
assert.equal(parse(text(bar({task: {state: 'Running!'}}))), null);
assert.equal(parse(text(bar({task: null}))).task, null);
assert.equal(parse(text(bar({needs: [{need: 'mood', action: 'play', state: 'low'}]}))), null);
assert.equal(parse(text(bar({unseen_events: [{seq: 1, stage: 7}]}))), null);
assert.equal(parse(text(bar({memorial: {count: -1}}))), null);
assert.equal(parse(text(bar({critical: 'yes'}))), null);
// New enum values from Rust only need the token shape.
assert.equal(parse(text(bar({life: Object.assign(bar().life, {status: 'dead'})}))).life.status, 'dead');

// isNewer: strictly greater; a versionless (older backend) snapshot is older.
assert.ok(isNewer(snap(), bar()));
assert.ok(!isNewer(snap(), bar({state_version: 800})));
assert.ok(!isNewer(snap(), bar({state_version: 799})));
assert.ok(!isNewer({}, bar()), 'versionless snapshot: order unknown, snapshot wins');
assert.strictEqual(merge({meters: {}}, parse(text(bar()))).meters.hunger, undefined);
assert.ok(!isNewer(snap(), null));

// merge: copies, never mutates; keeps details the bar subset does not carry.
const s0 = snap(); const before = JSON.stringify(s0);
const m = merge(s0, parse(text(bar())));
assert.equal(JSON.stringify(s0), before, 'input snapshot untouched');
assert.notStrictEqual(m, s0);
assert.equal(m.state_version, 812);
assert.deepEqual(m.meters, {hunger: 6100, thirst: 2800, mental_health: 8000, hygiene: 9000});
assert.equal(m.life.appearance, 'cat'); assert.equal(m.life.stage, 2); assert.equal(m.life.xp, 120);
assert.equal(m.life.status, 'warning'); assert.equal(m.life.armed, true);
assert.equal(m.life.revision, 4, 'same life keeps fields the bar does not carry');
assert.equal(m.life.origin, 'migrated');
assert.equal(m.decay.paused, false, 'Pause/Travel mirrors life.paused');
assert.equal(m.decay.enabled, true);
assert.equal(m.care.bathroom_due, false); assert.equal(m.care.ready_to_work, false);
assert.deepEqual(m.care.readiness_reasons, ['Needs water']);
assert.equal(m.care.revision, 3); assert.equal(m.care.work_sessions, 2);
assert.equal(m.active_task.task_id, 't-1'); assert.equal(m.active_task.state, 'running');
assert.equal(m.active_task.instructions_excerpt, 'kept');
assert.deepEqual(m.unseen_events, [{seq: 88, kind: 'evolved', stage: 2}]);
assert.equal(m.lifecycle.needs[1].state, 'low');
assert.equal(m.lifecycle.needs[1].meter, 2800);
assert.equal(m.lifecycle.needs[1].until_critical_ms, 15840000);
assert.equal(m.lifecycle.needs[1].until_warning_ms, 0, 'the warning window is refreshed from the file too');
assert.equal(parse(text(bar({needs: [{need: 'thirst', action: 'water', state: 'low', until_warning_ms: 'soon'}]}))), null);
assert.equal(m.lifecycle.needs[0].meter, 6100);
assert.equal(m.lifecycle.needs[0].state, 'ok');
assert.equal(m.lifecycle.frozen_reason, 'none');
assert.equal(m.memorial.count, 1); assert.equal(m.memorial.latest.ordinal, 0, 'same memorial kept');
assert.deepEqual(m.recent, [{task_id: 't-0'}]);
// Not newer: unchanged object.
assert.strictEqual(merge(s0, parse(text(bar({state_version: 800})))), s0);
assert.equal(merge(null, bar()), null);
// Slot freed elsewhere: active task cleared. A task the snapshot does not know is not invented.
assert.equal(merge(s0, parse(text(bar({task: null})))).active_task, null);
const idle = snap(); idle.active_task = null;
assert.equal(merge(idle, parse(text(bar()))).active_task, null);
// A different life replaces the life record; no living companion means life null.
const other = merge(s0, parse(text(bar({life: Object.assign(bar().life,
  {life_id: 'life-bbbbbbbbbbbbbbbbbbbbbbbb', name: 'Pip', appearance: 'goldfish', stage: 1})}))));
assert.equal(other.life.name, 'Pip'); assert.equal(other.life.revision, undefined);
assert.equal(other.life.appearance, 'goldfish');
const none = merge(s0, parse(text(bar({life: null, memorial: {count: 2, latest_name: 'Ginger'}}))));
assert.equal(none.life, null);
assert.equal(none.decay.paused, true, 'no life: decay block unchanged');
assert.deepEqual(none.memorial.latest, {name: 'Ginger'}); assert.equal(none.memorial.count, 2);
assert.equal(merge(s0, parse(text(bar({memorial: {count: 0, latest_name: null}})))).memorial.latest, null);
// Critical passes through as a copy.
const crit = {need: 'thirst', action: 'water', grace_ms: 21600000, elapsed_ms: 1, remaining_ms: 2, since_ms: 0};
const mc = merge(s0, parse(text(bar({critical: crit}))));
assert.deepEqual(mc.lifecycle.critical, crit);
// Database identity: versions are ordered only within one db_id.
const A = 'db-' + 'a'.repeat(24), B = 'db-' + 'b'.repeat(24);
assert.equal(parse(text(bar({db_id: A}))).db_id, A);
assert.equal(parse(text(bar({db_id: 'db-XYZ'}))), null, 'malformed db_id');
assert.equal(parse(text(bar({db_id: 7}))), null);
assert.ok(parse(text(bar())), 'a file without db_id (older writer) still parses');
const sA = Object.assign(snap(), {db_id: A});
assert.ok(isNewer(sA, parse(text(bar({db_id: A})))));
assert.ok(!isNewer(sA, parse(text(bar({db_id: B})))), 'another database is never "newer"');
assert.ok(!isNewer(sA, parse(text(bar({db_id: B, state_version: 10 ** 9})))));
assert.ok(replacesDatabase(sA, parse(text(bar({db_id: B, state_version: 2})))));
assert.ok(!replacesDatabase(sA, parse(text(bar({db_id: A, state_version: 2})))));
assert.ok(!replacesDatabase(snap(), parse(text(bar({db_id: B})))), 'unknown snapshot identity: no replacement');
assert.ok(sameFileDatabase(parse(text(bar({db_id: A}))), parse(text(bar({db_id: A})))));
assert.ok(!sameFileDatabase(parse(text(bar({db_id: A}))), parse(text(bar({db_id: B})))));
assert.ok(!sameFileDatabase(parse(text(bar())), parse(text(bar({db_id: B})))));
// A plain merge never crosses databases (finish(): a fresh state.get beats an old file).
const stale = parse(text(bar({db_id: B, state_version: 10 ** 9})));
assert.strictEqual(merge(sA, stale), sA);
// An explicit cross-database merge takes the file's subset and drops old task details.
const sTask = Object.assign(snap(), {db_id: A});
const crossed = merge(sTask, parse(text(bar({db_id: B, state_version: 2, task: null}))), true);
assert.equal(crossed.db_id, B); assert.equal(crossed.state_version, 2);
assert.equal(crossed.active_task, null); assert.deepEqual(crossed.recent, []);
assert.equal(crossed.meters.thirst, 2800);
const crossedBusy = merge(sTask, parse(text(bar({db_id: B, state_version: 2}))), true);
assert.equal(crossedBusy.active_task, null, 'the old database task is never shown against the new one');
assert.strictEqual(merge(sTask, parse(text(bar({db_id: A, state_version: 2}))), true), sTask,
  'same database, older version: still ignored');
console.log('bar-state parse, newer-only merge, XDG path and no-invention rules passed');
"""


class BarStateJs(unittest.TestCase):
    @unittest.skipUnless(shutil.which('node'), 'Node unavailable; BarState.js untested')
    def test_parse_merge_and_paths(self):
        result = subprocess.run(['node', '-e', BAR_STATE_JS + BAR_STATE_NODE],
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
        self.assertEqual(len(re.findall(r'call -- codepet', LAUNCHER)), 3)
        self.assertNotRegex(re.sub(r'#[^\n]*', '', LAUNCHER), r'call codepet')
        self.assertIn('/current/qml/shell.qml', LAUNCHER)
        self.assertIn('${XDG_DATA_HOME:-}', LAUNCHER)
        # hide/refresh/status branches never reach start_overlay.
        dispatch = LAUNCHER[LAUNCHER.index('case "$VERB" in\n    status)'):]
        for verb in ('status)', 'hide|refresh)'):
            branch = dispatch[dispatch.index(verb):]
            branch = branch[:branch.index(';;')]
            self.assertNotIn('start_overlay', branch, verb)
        self.assertNotIn('kill', re.sub(r'#[^\n]*', '', LAUNCHER))

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
        print('{{"pet":"shown"}}')
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
        for argv in ([], ["quit"], ["show", "extra"], ["--pid"]):
            r, calls = self.run_launcher(*argv)
            self.assertEqual(r.returncode, 2, argv)
            self.assertEqual(calls, [], argv)

    def test_not_running_status_hide_refresh_never_start(self):
        for verb, code in (("status", 1), ("hide", 0), ("refresh", 0)):
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
        self.assertIn('{"pet":"shown"}', r.stdout)

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


FAKE_SNAPSHOT_CODEPET = """#!{python} -I
# Test-only `codepet rpc`: logs each method to $HOME/rpc.log and answers state.get
# with a fixed v0.2-shaped snapshot (state_version 5, cat, stage 1).
import json, os, sys
req = json.loads(sys.stdin.readline())
with open(os.path.join(os.environ["HOME"], "rpc.log"), "a") as f:
    f.write(req["method"] + "\\n")
snap = {snapshot}
if req["method"] == "state.get":
    print(json.dumps({{"v": 1, "id": req["id"], "ok": True, "result": snap}}))
else:
    print(json.dumps({{"v": 1, "id": req["id"], "ok": False, "error": {{
        "code": "STORAGE_UNAVAILABLE", "message": "stub", "retryable": False}}}}))
"""

SNAPSHOT = {
    "schema_version": 4, "protocol_version": 1, "state_version": 5, "last_seq": 1,
    "meters": {"hunger": 7000, "thirst": 7000, "mental_health": 7000, "hygiene": 7000},
    "care": {"revision": 1, "bathroom_due": False, "starts_since_bathroom": 0, "sleeping": False,
             "sleep_due": False, "sleep_remaining_ms": 120000, "sleep_started_at_ms": None,
             "work_sessions": 0, "execution_ms": 0, "ready_to_work": False,
             "readiness_reasons": ["Needs care"], "welcome_care_available": True},
    "progression": {track: {"lifetime_xp": 0, "level": 1, "next_level_xp": 80,
                            "mastery": {"bugfix": 0, "quality_performance": 0,
                                        "learning_specification": 0, "cleanup": 0}}
                    for track in ("demo", "real")},
    "decay": {"enabled": False, "paused": False},
    "settings": {"revision": 1, "decay_enabled": False, "decay_per_hour": [1000, 1250, 600, 500],
                 "timeout_seconds": 600},
    "active_task": None, "recent": [], "providers": [],
    "life": {"life_id": "life-000000000000000000000001", "ordinal": 1, "name": "Ginger",
             "species": "cat", "appearance": "cat", "stage": 1, "level": 1, "xp": 0,
             "status": "healthy", "mode": "permadeath", "armed": False, "pace": "steady",
             "paused": False, "fainted": False, "death_pending": False, "revision": 1},
    "unseen_events": [], "memorial": {"count": 0, "latest": None},
}


DB_A = "db-" + "a" * 24


def bar_state_text(version, appearance, db_id=DB_A):
    return json.dumps({
        "schema": "codepet.bar.v1", "state_version": version, "db_id": db_id, "generated_at_ms": 0,
        "app_version": "0.2.0", "ruleset": "lifecycle-v1",
        "life": dict(SNAPSHOT["life"], appearance=appearance, stage=2, level=2, xp=120,
                     status="warning", armed=True, next_stage_xp=400),
        "meters": {"hunger": 6100, "thirst": 2800, "mental_health": 8000, "hygiene": 9000},
        "needs": [{"need": "thirst", "action": "water", "state": "low",
                   "until_critical_ms": 1, "until_end_ms": 2}],
        "critical": None,
        "care": {"sleeping": False, "sleep_remaining_ms": 120000, "sleep_started_at_ms": None,
                 "sleep_due": False, "bathroom_due": False, "ready_to_work": False,
                 "readiness_reasons": ["Needs water"]},
        "task": None,
        "presence": {"wanted": True, "life_id": SNAPSHOT["life"]["life_id"], "tick_ms": 60000,
                     "idle_timeout_s": 180},
        "unseen_events": [{"seq": 88, "stage": 2}],
        "memorial": {"count": 0, "latest_name": None}})


def write_atomically(path, text):
    """The writer's pattern (§10): a temp file in the same directory, then rename."""
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

    def close(self):
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

    def snapshot_env(self, root):
        env = isolated_env(root)
        fake = root / "bin/codepet"
        fake.write_text(FAKE_SNAPSHOT_CODEPET.format(python=sys.executable,
                                                     snapshot=repr(SNAPSHOT)))
        return env

    def rpc_log(self, env):
        path = Path(env["HOME"]) / "rpc.log"
        return path.read_text().split() if path.exists() else []

    def test_startup_open_view_refresh_and_tabs(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = self.snapshot_env(root)
            env["CODEPET_OPEN"] = "life"
            session = QsSession(root, env, stage_without_layer_shell(root))
            try:
                st = session.wait_for(lambda s: s.get("state_version") == 5)
                self.assertEqual(st.get("view"), "life", st)
                self.assertTrue(st.get("window"), st)
                self.assertEqual(st.get("appearance"), "cat")
                time.sleep(0.5)
                self.assertEqual(self.rpc_log(env), ["state.get"], "one startup read only")
                self.assertTrue(session.call("ping").stdout.startswith("codepet-overlay shown"))
                for verb, view in (("care", "care"), ("companion", "companion"), ("tasks", "tasks")):
                    self.assertEqual(session.call(verb).returncode, 0)
                    st = session.wait_for(lambda s: s.get("view") == view and not s.get("busy"))
                    self.assertEqual(st.get("view"), view)
                session.call("toggle")
                self.assertEqual(session.wait_for(lambda s: s.get("pet") == "hidden").get("pet"), "hidden")
                self.assertTrue(session.call("ping").stdout.startswith("codepet-overlay hidden"))
                session.call("toggle")
                self.assertEqual(session.wait_for(lambda s: s.get("pet") == "shown").get("pet"), "shown")
                before = len(self.rpc_log(env))
                session.call("refresh")
                session.wait_for(lambda s: not s.get("busy"))
                time.sleep(0.3)
                self.assertEqual(len(self.rpc_log(env)), before + 1, "refresh is one state.get")
                self.assertTrue(all(m == "state.get" for m in self.rpc_log(env)))
            finally:
                out = session.close()
        self.assertIn("Configuration Loaded", out, out)
        self.assertIsNone(LOAD_ERRORS.search(out), out)
        self.assertNotRegex(out, r"TypeError|ReferenceError|Cannot assign", out)

    def test_bar_state_reaches_the_pet_without_rpc(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = self.snapshot_env(root)
            state_dir = Path(env["XDG_STATE_HOME"]) / "codepet"
            bar = state_dir / "bar-state.json"
            entry = stage_without_layer_shell(root)
            # The state dir does not exist yet: the overlay starts before the first export.
            session = QsSession(root, env, entry)
            try:
                st = session.wait_for(lambda s: s.get("state_version") == 5)
                self.assertEqual(st.get("appearance"), "cat")
                state_dir.mkdir(parents=True)
                write_atomically(bar, bar_state_text(6, "robot"))
                st = session.wait_for(lambda s: s.get("state_version") == 6, seconds=3)
                if st.get("state_version") != 6:
                    # Directory created after startup: the watch is re-armed by the next
                    # explicit refresh (documented limit), which re-reads the file.
                    session.call("refresh")
                    st = session.wait_for(lambda s: s.get("state_version") == 6)
                self.assertEqual((st.get("state_version"), st.get("appearance"), st.get("stage")),
                                 (6, "robot", 2), st)
                reads = len(self.rpc_log(env))
                for version, look in ((7, "owl"), (8, "frog")):
                    write_atomically(bar, bar_state_text(version, look))
                    st = session.wait_for(lambda s, v=version: s.get("state_version") == v)
                    self.assertEqual((st.get("state_version"), st.get("appearance")), (version, look))
                # Older, malformed and oversized files are ignored.
                write_atomically(bar, bar_state_text(3, "dog"))
                write_atomically(bar, "{broken")
                write_atomically(bar, bar_state_text(9, "dog").replace('"codepet.bar.v1"', '"x"'))
                write_atomically(bar, bar_state_text(9, "dog")[:-1] + ', "pad": "' + "x" * 17000 + '"}')
                time.sleep(1.0)
                st = session.status()
                self.assertEqual((st.get("state_version"), st.get("appearance")), (8, "frog"), st)
                # A restored or recreated database restarts its counter: its lower-versioned
                # file is applied, not ignored until the count catches up.
                write_atomically(bar, bar_state_text(2, "penguin", db_id="db-" + "b" * 24))
                st = session.wait_for(lambda s: s.get("appearance") == "penguin")
                self.assertEqual((st.get("state_version"), st.get("appearance")), (2, "penguin"), st)
                write_atomically(bar, bar_state_text(3, "turtle", db_id="db-" + "b" * 24))
                st = session.wait_for(lambda s: s.get("state_version") == 3)
                self.assertEqual(st.get("appearance"), "turtle", st)
                # ...and within that database the order holds again.
                write_atomically(bar, bar_state_text(1, "dog", db_id="db-" + "b" * 24))
                time.sleep(1.0)
                self.assertEqual(session.status().get("appearance"), "turtle")
                self.assertEqual(len(self.rpc_log(env)), reads, "file events never launch codepet")
            finally:
                out = session.close()
        self.assertIsNone(LOAD_ERRORS.search(out), out)
        self.assertNotRegex(out, r"TypeError|ReferenceError", out)

    def test_bar_state_present_at_startup_is_applied(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = self.snapshot_env(root)
            state_dir = Path(env["XDG_STATE_HOME"]) / "codepet"
            state_dir.mkdir(parents=True)
            write_atomically(state_dir / "bar-state.json", bar_state_text(9, "axolotl"))
            session = QsSession(root, env, stage_without_layer_shell(root))
            try:
                st = session.wait_for(lambda s: s.get("state_version") == 9)
                self.assertEqual((st.get("state_version"), st.get("appearance")), (9, "axolotl"), st)
                self.assertEqual(self.rpc_log(env), ["state.get"])
            finally:
                session.close()

    def test_host_adapter_harness(self):
        """qml/tests/quickshell/shell.qml: transport, failures and v0.2 adapter additions."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = isolated_env(root)
            stage = root / "harness"
            stage.mkdir()
            for name in ("shared", "omarchy", "assets"):
                shutil.copytree(QML / name, stage / name)
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
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = isolated_env(root)
            stage = root / "real"
            stage.mkdir()
            shutil.copytree(QML / "omarchy", stage / "omarchy")
            shutil.copy(QML / "tests/quickshell/real_rpc.qml", stage / "shell.qml")
            test_root = root / "rpcroot"
            (test_root / "project/src").mkdir(parents=True)
            (test_root / "home").mkdir()
            env.update(CODEPET_BIN=os.environ["CODEPET_TEST_BIN"], CODEPET_TEST_ROOT=str(test_root))
            r = subprocess.run(["qs", "-p", str(stage / "shell.qml")], env=env,
                               capture_output=True, text=True, timeout=120)
        out = r.stdout + r.stderr
        self.assertIn("DONE failures=0", out, out[-4000:])

    def test_launcher_drives_one_real_instance(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = self.snapshot_env(root)
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
                self.assertEqual((st.get("window"), st.get("view")), (True, "tasks"), st)
                for verb in ("show", "toggle", "care", "show"):
                    r = launcher(verb)
                    self.assertEqual(r.returncode, 0, (verb, r.stderr))
                self.assertEqual(len(instances()), 1, "verbs never start a second copy")
                code, st = overlay_status()
                self.assertEqual((st.get("pet"), st.get("view")), ("shown", "care"), st)
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
