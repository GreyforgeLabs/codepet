#!/usr/bin/env python3
"""Load the real work window/transport offscreen with a fixture-only backend.

No provider, live pet, desktop window or original project is touched.
"""
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
class WorkHost(unittest.TestCase):
    def test_start_review_cancel_reopen_and_literal_text(self):
        # Unix IPC socket names must fit sun_path even when TMPDIR is a long run path.
        runtime_parent = Path("/run/user") / str(os.getuid())
        with tempfile.TemporaryDirectory(prefix="cpwork") as td, tempfile.TemporaryDirectory(
                prefix="cpwork-ipc-", dir=runtime_parent if runtime_parent.is_dir() else "/tmp") as runtime:
            root = Path(td)
            for name in ("run", "home", "qml", "qml/omarchy"):
                (root / name).mkdir(mode=0o700)
            for name in ("WorkPanel.qml", "WorkController.qml", "WorkRpc.qml", "WorkModel.js", "HostAdapter.qml", "BarState.js", "OverlayFlow.js"):
                shutil.copyfile(REPO / "qml/omarchy" / name, root / "qml/omarchy" / name)
            shutil.copytree(REPO / "qml/shared", root / "qml/shared")
            (root / "qml/assets").symlink_to(REPO / "qml/assets", target_is_directory=True)
            entry = root / "qml/work.qml"
            shutil.copyfile(REPO / "qml/work.qml", entry)
            entry.write_text(entry.read_text().replace('function status(): string {', '''
        function testStart(): void { panel.project = "/test/project"; panel.prompt = "literal $(no shell)"; panel.selectedKind = "bug"; panel.start() }
        function testAccept(): void { panel.verificationAcknowledged = true; panel.accept() }
        function testCancel(): void { work.action("cancel", {task_id: work.task.id}) }
        function testNew(): void { work.newJob() }
        function testRefresh(): void { work.refresh() }
        function status(): string {'''))
            transport = root / "qml/omarchy/WorkController.qml"
            transport.write_text(transport.read_text().replace('WorkRpc { id: rpc;', 'WorkRpc { id: rpc; responseTimeoutMs: 250;'))
            state = root / "fixture.json"
            initial = {"readiness": {"ready": True, "reason": "Ready for a job", "energy": 100, "sleeping": False},
                       "provider": {"name": "Codex", "available": True}, "task": None}
            state.write_text(json.dumps(initial))
            fake = root / "fake-codepet"
            fake.write_text('''#!/usr/bin/python3
import json,os,sys,time
from pathlib import Path
root=Path(os.environ['FIXTURE_ROOT']); req=json.loads(sys.stdin.readline()); method=req['method']; p=req['params']
with (root/'calls.jsonl').open('a') as f: f.write(json.dumps(req)+'\\n')
mode=root/'transport-mode'
if mode.exists():
    value=mode.read_text();mode.unlink()
    if value=='garbled': print('not json',flush=True)
    if value=='oversize': print('x'*1100000,flush=True)
    time.sleep(30)
s=json.loads((root/'fixture.json').read_text())
if method=='start':
    assert p['request_id'] and p['network'] is True
    task_id='job-fixture-next' if s['task'] else 'job-fixture'
    s['task']={'id':task_id,'kind':p['kind'],'project':p['project'],'prompt':p['prompt'],'status':'running','progress':'Checking files','output':'<b>literal</b>','changes':[],'reward_xp':20,'reward_need':'hunger'}
elif method=='accept':
    assert p=={'task_id':'job-fixture','review_digest':'digest-fixture','acknowledge_unverified':True}
    s['task']['status']='accepted'
elif method=='cancel': s['task']['status']='cancelled'
(root/'fixture.json').write_text(json.dumps(s))
print(json.dumps({'ok':True,'result':s}))
''')
            fake.chmod(0o700)
            env = {"HOME": str(root / "home"), "PATH": "/usr/bin:/bin", "XDG_RUNTIME_DIR": runtime,
                   "XDG_CACHE_HOME": str(root / "cache"), "QT_QPA_PLATFORM": "offscreen",
                   "QT_QUICK_CONTROLS_STYLE": "Basic", "QT_FORCE_STDERR_LOGGING": "1",
                   "CODEPET_BIN": str(fake), "CODEPET_WORK_KIND": "bug", "FIXTURE_ROOT": str(root)}
            log = (root / "window.log").open("w+")
            process = None

            def ipc(method, *args):
                return subprocess.check_output([QS, "ipc", "-p", str(entry), "call", "--", "codepet-work", method, *args],
                                               env=env, text=True, stderr=subprocess.DEVNULL, timeout=3).strip()

            def wait(predicate):
                end = time.monotonic() + 8
                while time.monotonic() < end:
                    try:
                        if predicate():
                            return
                    except (subprocess.SubprocessError, json.JSONDecodeError):
                        pass
                    time.sleep(.05)
                log.flush(); log.seek(0)
                self.fail("Work window did not settle:\n" + log.read())

            def status():
                return json.loads(ipc("status"))

            def start():
                nonlocal process
                process = subprocess.Popen([QS, "-p", str(entry)], env=env, stdout=log, stderr=subprocess.STDOUT)
                wait(lambda: status()["visible"] and not status()["busy"])

            def stop():
                if process and process.poll() is None:
                    process.terminate(); process.wait(timeout=5)

            try:
                start()
                s = json.loads(state.read_text())
                s["scout"] = {"enabled": True, "next_due_ms": 9999999999999,
                              "notice": "The last bug check could not start.",
                              "last_error": "PRIVATE_FIXTURE: file exceeds the input limit"}
                state.write_text(json.dumps(s))
                ipc("testRefresh")
                wait(lambda: "PRIVATE_FIXTURE" in status()["error"] and not status()["busy"])
                stop(); start()
                wait(lambda: "PRIVATE_FIXTURE" in status()["error"] and not status()["busy"])
                s["scout"].update(last_error="", notice="")
                state.write_text(json.dumps(s))
                ipc("testRefresh")
                wait(lambda: not status()["error"] and not status()["busy"])
                ipc("testStart")
                wait(lambda: status()["task"] == "running" and not status()["busy"])
                self.assertEqual(json.loads(state.read_text())["task"]["prompt"], "literal $(no shell)")
                stop(); start()
                wait(lambda: status()["task"] == "running" and not status()["busy"])
                requests = [json.loads(line) for line in (root / "calls.jsonl").read_text().splitlines()]
                self.assertEqual(sum(r["method"] == "start" for r in requests), 1, "reopening must not duplicate work")
                self.assertFalse(any(r["method"] == "cancel" for r in requests), "closing must not cancel work")
                s = json.loads(state.read_text())
                s["task"].update(status="review", review_digest="digest-fixture", changes=[{"path": "a.txt", "before": "old", "after": "new"}])
                state.write_text(json.dumps(s))
                ipc("testRefresh")
                wait(lambda: status()["task"] == "review" and not status()["busy"])
                ipc("testAccept")
                wait(lambda: status()["task"] == "accepted" and not status()["busy"])
                ipc("testNew"); ipc("testStart")
                wait(lambda: status()["task"] == "running" and not status()["busy"])
                ipc("testCancel")
                wait(lambda: status()["task"] == "cancelled" and not status()["busy"])
                for mode in ("garbled", "oversize", "timeout"):
                    (root / "transport-mode").write_text(mode)
                    ipc("testRefresh")
                    wait(lambda: bool(status()["error"]) and not status()["busy"])
                    ipc("testRefresh")
                    wait(lambda: not status()["error"] and not status()["busy"])
                ipc("open", "compact")
                wait(lambda: status()["compact"] and not status()["busy"])
                ipc("open", "research")
                wait(lambda: not status()["compact"] and not status()["busy"])
            finally:
                stop()
            log.seek(0)
            output = log.read()
            log.close()
            for error in ("TypeError:", "ReferenceError:", "Failed to load configuration", "Binding loop", "Cannot assign"):
                self.assertNotIn(error, output)


if __name__ == "__main__":
    unittest.main(verbosity=2)
