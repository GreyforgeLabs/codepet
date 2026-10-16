"""Pure/mocked QA guard regressions; never contacts a compositor or process."""
import itertools
from pathlib import Path
import signal
import tomllib
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

import rig_qa


class RigSafetyTests(unittest.TestCase):
    def test_helpers_coverage_requires_all_five_rows_and_both_hints(self):
        rows = ['cp.helperRow.' + cid for cid in rig_qa.HELPER_CLIENTS]
        hints = ['Open a new Grok window.', 'Hermes is too old.']
        expected = {'screen': 'helpers', 'row_prefix': 'cp.helperRow.', 'rows': rows, 'texts': hints}
        observed = {'screen': 'helpers', 'nodes': [{'name': row} for row in rows]
                    + [{'name': '', 'text': hint} for hint in hints]}
        self.assertTrue(rig_qa.page_converged(observed, expected))
        self.assertFalse(rig_qa.page_converged({'screen': 'helpers', 'nodes': []}, expected))
        for i in range(len(observed['nodes'])):
            missing = dict(observed, nodes=observed['nodes'][:i] + observed['nodes'][i + 1:])
            self.assertFalse(rig_qa.page_converged(missing, expected))

    def test_connect_coverage_requires_exact_offered_count(self):
        for count in range(1, 5):
            rows = ['cp.connectRow.' + cid for cid in rig_qa.CONNECT_CLIENTS[:count]]
            expected = {'screen': 'connect', 'row_prefix': 'cp.connectRow.', 'rows': rows}
            observed = {'screen': 'connect', 'nodes': [{'name': row} for row in rows]}
            self.assertTrue(rig_qa.page_converged(observed, expected))
            self.assertFalse(rig_qa.page_converged(dict(observed, screen='home'), expected))
            self.assertFalse(rig_qa.page_converged(
                dict(observed, nodes=observed['nodes'] + [{'name': rows[0]}]), expected))

    def test_font_settings_are_actual_toml_for_each_sweep_size(self):
        for size in (12, 16, 20):
            with self.subTest(size=size):
                self.assertEqual(tomllib.loads(rig_qa.font_settings(size)),
                                 {'font': {'base-size': size}})
        with self.assertRaisesRegex(RuntimeError, 'Unsupported QA font size'):
            rig_qa.font_settings(18)

    def test_font_observation_must_match_intended_heading_and_scale(self):
        for size, heading, scale in ((12, 16, .75), (16, 21, 1.0), (20, 27, 1.25)):
            with self.subTest(size=size):
                self.assertTrue(rig_qa.font_converged(
                    {'headingSize': heading, 'scale': scale}, size))
                self.assertFalse(rig_qa.font_converged(
                    {'headingSize': heading + 1, 'scale': scale}, size))
                self.assertFalse(rig_qa.font_converged(
                    {'headingSize': heading, 'scale': scale + .25}, size))
                self.assertFalse(rig_qa.font_converged({}, size))
        for size in (16, 20):
            self.assertFalse(rig_qa.font_converged({'headingSize': 16, 'scale': .75}, size))

    def test_mislabeled_capture_fails_before_screenshot(self):
        rig = object.__new__(rig_qa.Rig)
        rig.observe = Mock(return_value={'headingSize': 16, 'scale': .75})
        rig.run = Mock()
        with self.assertRaisesRegex(RuntimeError, 'expected font 20'):
            rig.capture('fixture-20', 'synthetic-export', font_size=20)
        rig.run.assert_not_called()

    def test_rename_requires_final_version_and_name(self):
        final = {'state_version': 9, 'life': {'name': 'Juniper'}}
        self.assertFalse(rig_qa.rename_converged(
            {'state_version': 4, 'life': {'name': 'Juniper'}}, final))
        self.assertFalse(rig_qa.rename_converged(
            {'state_version': 9, 'life': {'name': 'Ginger'}}, final))
        self.assertFalse(rig_qa.rename_converged({}, final))
        self.assertTrue(rig_qa.rename_converged(final, final))

    def cleanup_rig(self, alive):
        rig = object.__new__(rig_qa.Rig)
        rig.home = Path('/unused/qa/home')
        rig.args = SimpleNamespace(display='wayland-test')
        rig.manifest = {}
        rig.guard = Mock()
        rig.pids = Mock(side_effect=lambda: sorted(alive))
        return rig

    def test_disappearing_process_does_not_skip_remaining_cleanup(self):
        alive = {101, 102}
        rig = self.cleanup_rig(alive)

        def kill(pid, sig):
            alive.remove(pid)
            if pid == 101:
                raise ProcessLookupError()

        with patch.object(rig_qa, 'matching_qs', return_value=True), \
                patch.object(rig_qa.os, 'kill', side_effect=kill) as killed:
            rig.cleanup()
        self.assertEqual(killed.call_count, 2)
        self.assertIn('cleanup', rig.manifest)

    def test_escalation_rechecks_identity_and_waits_for_exit(self):
        alive = {101}
        rig = self.cleanup_rig(alive)
        identities = []

        def matches(*args):
            identities.append(args)
            return True

        def kill(pid, sig):
            if sig == signal.SIGKILL:
                alive.remove(pid)

        with patch.object(rig_qa, 'matching_qs', side_effect=matches), \
                patch.object(rig_qa.os, 'kill', side_effect=kill) as killed, \
                patch.object(rig_qa.time, 'monotonic', side_effect=itertools.count()), \
                patch.object(rig_qa.time, 'sleep') as slept:
            rig.cleanup()
        self.assertEqual([c.args[1] for c in killed.call_args_list],
                         [signal.SIGTERM, signal.SIGKILL])
        self.assertEqual(len(identities), 2)
        self.assertTrue(slept.called)
        self.assertEqual(rig.guard.call_count, 4)

    def test_changed_identity_is_not_signalled_during_escalation(self):
        alive = {101}
        rig = self.cleanup_rig(alive)
        checks = 0

        def matches(*args):
            nonlocal checks
            checks += 1
            if checks == 2:
                alive.clear()
                return False
            return True

        with patch.object(rig_qa, 'matching_qs', side_effect=matches), \
                patch.object(rig_qa.os, 'kill') as killed, \
                patch.object(rig_qa.time, 'monotonic', side_effect=itertools.count()), \
                patch.object(rig_qa.time, 'sleep'):
            rig.cleanup()
        killed.assert_called_once_with(101, signal.SIGTERM)

    def test_remaining_process_fails_cleanup(self):
        rig = self.cleanup_rig({101})
        with patch.object(rig_qa, 'matching_qs', return_value=True), \
                patch.object(rig_qa.os, 'kill'), \
                patch.object(rig_qa.time, 'monotonic', side_effect=itertools.count()), \
                patch.object(rig_qa.time, 'sleep'):
            with self.assertRaisesRegex(RuntimeError, 'remain after cleanup'):
                rig.cleanup()

    def test_cleanup_failure_fails_run_and_is_saved(self):
        rig = Mock()
        rig.manifest = {}
        rig.output = Mock()
        rig.output.__truediv__ = Mock(return_value=Mock(exists=lambda: True))
        rig.cleanup.side_effect = RuntimeError('still running')
        args = SimpleNamespace(**dict.fromkeys(
            ('home', 'display', 'instance', 'binary', 'output', 'pointer'), 'unused'))
        with patch.object(rig_qa.argparse.ArgumentParser, 'parse_args', return_value=args), \
                patch.object(rig_qa, 'Rig', return_value=rig):
            with self.assertRaisesRegex(RuntimeError, 'cleanup failed'):
                rig_qa.main()
        self.assertEqual(rig.manifest['status'], 'failed')
        self.assertEqual(rig.manifest['cleanup_error'], 'still running')
        rig.save.assert_called_once()


if __name__ == '__main__':
    unittest.main()
