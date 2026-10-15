#!/usr/bin/env python3
"""Source contract + pure JS policy tests; NOT Quickshell/Wayland runtime proof.
Run: python qml/tests/test_quickshell_host.py (Node required for policy tests).
"""
from pathlib import Path
import re
import shutil
import subprocess
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


if __name__ == '__main__':
    unittest.main(verbosity=2)
