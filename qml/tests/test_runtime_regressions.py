#!/usr/bin/env python3
"""Offline launcher/gate regressions. Every command uses fixture HOME and stubs."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[2]


def executable(path, text):
    path.write_text(text)
    path.chmod(0o700)


class RuntimeRegressions(unittest.TestCase):
    def test_overlay_fallback_excludes_work_and_nested_entries(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            data = root / "data/codepet"
            qml = data / "current/qml/shell.qml"
            qml.parent.mkdir(parents=True)
            qml.write_text("fixture")
            qs = root / "qs"
            executable(qs, '''#!/usr/bin/python3
import json,os,sys
from pathlib import Path
r=Path(os.environ['HOME']);a=sys.argv[1:]
with (r/'calls').open('a') as f:f.write(json.dumps(a)+'\\n')
if a[0]=='list': print((r/'list').read_text());raise SystemExit(0)
if a[0]=='-n': (r/'started').touch();raise SystemExit(0)
if a[0]=='ipc':
 if a[1]=='--pid' and a[2]=='77':
  print('codepet-overlay' if a[-1]=='ping' else '{"supervisor":false}');raise SystemExit(0)
 if (r/'started').exists():
  print('codepet-overlay' if a[-1]=='ping' else '{"supervisor":false}');raise SystemExit(0)
raise SystemExit(1)
''')
            env = {"HOME": td, "PATH": "/usr/bin:/bin", "XDG_DATA_HOME": str(root / "data"), "CODEPET_QS": str(qs)}
            bad = [{"config_path": str(data / "current/qml/work.qml"), "pid": 50},
                   {"config_path": str(data / "releases/0.4.0/qml/work.qml"), "pid": 51},
                   {"config_path": str(data / "releases/0.4.0/other/qml/shell.qml"), "pid": 52}]
            (root / "list").write_text(json.dumps(bad, indent=2))
            launcher = REPO / "packaging/omarchy/codepet-overlay"
            self.assertEqual(subprocess.run([launcher, "status"], env=env, capture_output=True).returncode, 1)
            result = subprocess.run([launcher, "show"], env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue((root / "started").exists())
            (root / "started").unlink()
            (root / "calls").write_text("")
            (root / "list").write_text(json.dumps(bad + [{"config_path": str(data / "releases/0.3.0/qml/shell.qml"), "pid": 77}], indent=2))
            result = subprocess.run([launcher, "show"], env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            calls = [json.loads(x) for x in (root / "calls").read_text().splitlines()]
            self.assertIn(["ipc", "--pid", "77", "call", "--", "codepet", "show"], calls)
            self.assertFalse((root / "started").exists())

    def test_claude_gate_refuses_failed_required_steps_and_bad_idempotence(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            unshare = root / "unshare"
            executable(unshare, '#!/bin/sh\nshift\nexec "$@"\n')
            cli = root / "claude"
            executable(cli, '''#!/usr/bin/python3
import json,os,sys
from pathlib import Path
r=Path(os.environ['HOME']);a=sys.argv[1:];mode=Path(__file__).with_name('mode').read_text()
p=r/'.claude/plugins/installed_plugins.json';p.parent.mkdir(parents=True,exist_ok=True)
if a[1]=='validate': print('Validation passed');raise SystemExit(7 if mode=='validate' else 0)
if a[1]=='install':
 p.write_text(json.dumps({'plugins':{'codepet@codepet-local':[]}})); print(json.dumps({'command':'install','outcome':'ok'}));raise SystemExit(8 if mode=='install' else 0)
if a[1]=='list':print(json.dumps([{'id':'codepet@codepet-local','scope':'user','enabled':True}]));raise SystemExit(0)
verb='marketplace-remove' if a[1]=='marketplace' else 'uninstall';marker=r/verb
if marker.exists():
 print(json.dumps({'command':verb,'outcome':'failed','failureCode':'unexpected' if mode=='idempotent' else 'not_configured' if verb=='marketplace-remove' else 'not_installed'}));raise SystemExit(1)
marker.touch();p.write_text(json.dumps({'plugins':{}}));print(json.dumps({'command':verb,'outcome':'ok'}))
''')
            for mode in ('validate', 'install', 'idempotent', 'success'):
                (root / 'mode').write_text(mode)
                result = subprocess.run([REPO / 'tools/integrations/claude-day0-gate.sh', cli, root / 'scratch'], env={"HOME": td, "PATH": td + ':/usr/bin:/bin'}, capture_output=True, text=True)
                if mode == 'success':
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertIn('gate: passed', result.stdout)
                else:
                    self.assertNotEqual(result.returncode, 0, result.stdout)
                    self.assertNotIn('gate: passed', result.stdout)
                    self.assertNotIn('== validated sources', result.stdout)


if __name__ == '__main__':
    unittest.main(verbosity=2)
