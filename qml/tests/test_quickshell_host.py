#!/usr/bin/env python3
"""Source contract + pure JS policy tests, plus real Quickshell load smokes when `qs` is
installed. NOT Wayland runtime proof: the load smokes run offscreen with no compositor.
Run: python qml/tests/test_quickshell_host.py (Node required for policy tests).
"""
from pathlib import Path
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

QML = Path(__file__).resolve().parents[1]
SHELL = (QML / "shell.qml").read_text()
CODE = re.sub(r"//[^\n]*", "", SHELL)
POLICY = (QML / "omarchy/OverlayPolicy.js").read_text().replace(".pragma library", "")


class HostContract(unittest.TestCase):
    def test_surface_is_transparent_nonexclusive_and_never_focusable(self):
        for token in ('color: "transparent"', 'exclusiveZone: 0',
                      'WlrLayershell.layer: WlrLayer.Top',
                      'WlrLayershell.keyboardFocus: WlrKeyboardFocus.None',
                      'anchors { left: true; right: true; top: true; bottom: true }'):
            self.assertIn(token, CODE)
        self.assertNotIn('WlrKeyboardFocus.OnDemand', CODE)
        self.assertNotIn('WlrKeyboardFocus.Exclusive', CODE)

    def test_hit_mask_has_only_three_bounded_regions(self):
        mask = CODE.split('mask: Region {', 1)[1].split('DesktopPet {', 1)[0]
        self.assertEqual(mask.count('Region {'), 3)
        self.assertEqual(mask.count('Intersection.Combine'), 4)
        self.assertNotIn('item:', mask)
        self.assertNotIn('mask: null', CODE)
        for name in ('pet', 'menu', 'props'):
            self.assertIn(f'OverlayPolicy.clipHit(pet.{name}Hit, width, height, shell.surfaceActive)', CODE)
            self.assertIn(f'width: overlay.{name}Input.width', mask)

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
                for call in (["show"], ["hide"], ["tasks"], ["closeTasks"],
                             ["setReducedMotion", "true"], ["setFullscreen", "false"]):
                    r = subprocess.run(["qs", "ipc", "-p", str(entry), "call", "codepet", *call],
                                       env=env, capture_output=True, text=True, timeout=10)
                    self.assertEqual(r.returncode, 0, (call, r.stdout, r.stderr))
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


if __name__ == '__main__':
    unittest.main(verbosity=2)
