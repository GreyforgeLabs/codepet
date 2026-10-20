#!/usr/bin/env python3
"""Real updater controller offscreen; fixture HOME/helper only, never GitHub/install."""
from pathlib import Path
import json
import os
import shutil
import subprocess
import tempfile
import time
import unittest

REPO = Path(__file__).resolve().parents[2]
QS = shutil.which("qs")


@unittest.skipUnless(QS, "Quickshell unavailable")
class UpdateHost(unittest.TestCase):
    def test_open_check_install_reconnect_poll_and_transport_errors(self):
        # Unix sockets have a short path limit. Keep only ephemeral IPC sockets in
        # XDG_RUNTIME_DIR; fixture HOME, state and logs remain under the test run.
        with tempfile.TemporaryDirectory(prefix="cpupdate") as td, tempfile.TemporaryDirectory(prefix="cpu", dir=os.environ.get("XDG_RUNTIME_DIR", "/tmp")) as ipcdir:
            root = Path(td)
            for name in ("home", "run", "qml"):
                (root / name).mkdir(mode=0o700)
            for name in ("UpdateController.qml", "UpdateModel.js"):
                shutil.copyfile(REPO / "omarchy-plugin" / name, root / "qml" / name)
            entry = root / "qml/shell.qml"
            entry.write_text('''import QtQuick
import Quickshell
import Quickshell.Io
ShellRoot {
    UpdateController { id: ctl; helper: Quickshell.env("FIXTURE_HELPER"); responseTimeoutMs: 300 }
    IpcHandler {
        target: "update-test"
        function open(): void { ctl.open() }
        function close(): void { ctl.opened = false }
        function check(): void { ctl.check() }
        function install(): void { ctl.install() }
        function restart(): void { ctl.restartBar() }
        function unsupported(): void { ctl.supported = false; ctl.open(); ctl.check(); ctl.install() }
        function missing(): void { ctl.helper = Quickshell.env("HOME") + "/not-installed"; ctl.check() }
        function status(): string { return JSON.stringify({busy:ctl.busy,opened:ctl.opened,state:ctl.snapshot}) }
    }
}
''')
            snapshot = root / "snapshot.json"
            initial = {"schema": "codepet.update.v1", "current": "0.4.0", "latest": "", "state": "idle", "message": "", "checked_at": 0}
            snapshot.write_text(json.dumps(initial))
            helper = root / "fake-update"
            helper.write_text('''#!/usr/bin/python3
import json,os,sys,time
from pathlib import Path
root=Path(os.environ['FIXTURE_ROOT']);command=sys.argv[1:]
assert len(command)==1 and command[0] in ['status','check','install','restart-bar']
with (root/'calls.jsonl').open('a') as f:f.write(json.dumps(command)+'\\n')
mode=root/'mode'
if mode.exists() and command[0]=='check':
    m=mode.read_text();mode.unlink()
    if m=='garbled':print('not-json',flush=True)
    if m=='oversize':print('x'*40000,flush=True)
    time.sleep(30)
s=json.loads((root/'snapshot.json').read_text())
if command[0]=='check':s.update(state='available',latest='0.4.1',checked_at=123,message='An update is ready.')
if command[0]=='install':
    assert s['state']=='available'
    s.update(state='downloading',message='Downloading the release.')
if command[0]=='restart-bar':
    assert s['restart_required'] is True
    s.update(restart_state='restarting',restart_message='Restarting the bar.')
(root/'snapshot.json').write_text(json.dumps(s))
print(json.dumps(s),flush=True)
''')
            helper.chmod(0o700)
            env = {"LANG": "C.UTF-8", "HOME": str(root / "home"), "PATH": "/usr/bin:/bin", "XDG_RUNTIME_DIR": ipcdir,
                   "XDG_STATE_HOME": str(root / "state"), "XDG_CACHE_HOME": str(root / "cache"),
                   "QT_QPA_PLATFORM": "offscreen", "QT_FORCE_STDERR_LOGGING": "1",
                   "FIXTURE_ROOT": str(root), "FIXTURE_HELPER": str(helper)}
            log = (root / "ui.log").open("w+")
            process = None

            def ipc(method):
                return subprocess.check_output([QS, "ipc", "-p", str(entry), "call", "--", "update-test", method],
                                               env=env, text=True, stderr=subprocess.DEVNULL, timeout=3).strip()

            def status():
                return json.loads(ipc("status"))

            def calls():
                path = root / "calls.jsonl"
                return [json.loads(line)[0] for line in path.read_text().splitlines()] if path.exists() else []

            def wait(predicate):
                end = time.monotonic() + 6
                while time.monotonic() < end:
                    try:
                        if predicate():
                            return
                    except (subprocess.SubprocessError, json.JSONDecodeError, TypeError):
                        pass
                    time.sleep(.04)
                log.flush(); log.seek(0)
                self.fail("Updater UI did not settle:\n" + log.read())

            def start():
                nonlocal process
                process = subprocess.Popen([QS, "-p", str(entry)], env=env, stdout=log, stderr=subprocess.STDOUT)
                wait(lambda: status()["state"] is None)

            def stop():
                if process and process.poll() is None:
                    process.terminate(); process.wait(timeout=4)

            try:
                start()
                time.sleep(.2)
                self.assertEqual(calls(), [], "no background startup check")
                ipc("open")
                wait(lambda: status()["state"]["state"] == "available" and not status()["busy"])
                self.assertEqual(calls(), ["status", "check"])
                time.sleep(1.4)
                self.assertEqual(calls(), ["status", "check"], "available/idle pages do not poll")
                ipc("install")
                wait(lambda: status()["state"]["state"] == "downloading" and not status()["busy"])
                ipc("install")
                self.assertEqual(calls().count("install"), 1, "active update cannot be duplicated")
                ipc("close")
                count = len(calls()); time.sleep(1.4)
                self.assertEqual(len(calls()), count, "closed page stops status polling")
                stop(); start(); ipc("open")
                wait(lambda: status()["state"]["state"] == "downloading" and not status()["busy"])
                self.assertEqual(calls().count("check"), 1, "reconnect does not replace durable active update with a check")
                self.assertEqual(calls().count("install"), 1)
                finished = {**initial, "current": "0.4.1", "latest": "0.4.1", "state": "succeeded", "message": "Updated."}
                snapshot.write_text(json.dumps(finished))
                wait(lambda: status()["state"]["state"] == "succeeded" and not status()["busy"])
                count = len(calls()); time.sleep(1.4)
                self.assertEqual(len(calls()), count, "success stops polling")
                ipc("close"); ipc("open")
                wait(lambda: not status()["busy"])
                self.assertEqual(calls().count("check"), 1, "reopening preserves success receipt")
                finished.update(restart_required=True, restart_state='idle')
                snapshot.write_text(json.dumps(finished))
                ipc('close'); ipc('open')
                wait(lambda: status()['state'].get('restart_required') and not status()['busy'])
                ipc('restart')
                wait(lambda: status()['state'].get('restart_state') == 'restarting' and not status()['busy'])
                ipc('restart')
                self.assertEqual(calls().count('restart-bar'), 1)
                stop(); start(); ipc('open')
                wait(lambda: status()['state'].get('restart_state') == 'restarting' and not status()['busy'])
                finished.update(restart_required=False, restart_state='idle')
                snapshot.write_text(json.dumps(finished))
                wait(lambda: status()['state'].get('restart_required') is False and not status()['busy'])
                self.assertEqual(calls().count('restart-bar'), 1, 'reconnect never repeats restart')
                for mode in ("garbled", "oversize", "timeout"):
                    (root / "mode").write_text(mode)
                    ipc("check")
                    wait(lambda: status()["state"]["state"] == "error" and not status()["busy"])
                    ipc("check")
                    wait(lambda: status()["state"]["state"] == "available" and not status()["busy"])
                ipc("missing")
                wait(lambda: status()["state"]["state"] == "error" and not status()["busy"])
                self.assertIn("Reinstall CodePet", status()["state"]["message"])
                count = len(calls())
                ipc("unsupported")
                self.assertFalse(status()["busy"])
                self.assertIn("test state or custom backend", status()["state"]["message"])
                self.assertEqual(len(calls()), count, "test mode blocks every helper action before process creation")
            finally:
                stop()
            log.seek(0); output = log.read(); log.close()
            for error in ("ReferenceError:", "TypeError:", "Cannot assign", "Failed to load configuration", "Binding loop"):
                self.assertNotIn(error, output)


if __name__ == "__main__":
    unittest.main(verbosity=2)
