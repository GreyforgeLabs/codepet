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
        function testSelect(id: string): void { workController.automatic = true; workController.selectClient(id) }
        function testObservedChoice(id: string): void {
            work.state = Object.assign({}, workController.state, {provider: workController.state.clients.filter(function(c) { return c.id === id })[0]})
        }
        function testNoAuto(): void { workController.automatic = false }
        function testFix(): void { workController.fix() }
        function testAccept(): void { panel.verificationAcknowledged = true; panel.accept() }
        function testCancel(): void { work.action("cancel", {task_id: work.task.id}) }
        function testNew(): void { work.newJob() }
        function testDismiss(): void { workController.dismiss() }
        function testScoutKey(): string { return workController.scoutKey(workController.state) }
        function testRedactedSummary(): void {
            var scout = workController.state.scout
            work.state = Object.assign({}, workController.state, {scout:{enabled:scout.enabled,next_due_ms:scout.next_due_ms,notice:scout.notice}})
        }
        function testOldSummary(): void {
            window.visible = false
            work.state = Object.assign({}, workController.state, {task:{id:workController.dismissedTask,status:"failed"}})
        }
        function testOldScoutSummary(): void {
            window.visible = false
            work.state = Object.assign({}, workController.state, {scout:Object.assign({},workController.state.scout,{notice:"Stale check notice",last_error:"Old private error"})})
        }
        function testReopen(): void { window.visible = true }
        function testRefresh(): void { work.refresh() }
        function status(): string {'''))
            entry.write_text(entry.read_text().replace('error: work.error', 'error: work.error, notice: workController.state && workController.state.scout ? workController.state.scout.notice : "", privateNotice: workController.state && workController.state.scout ? workController.state.scout.last_error : ""'))
            entry.write_text(entry.read_text().replace('error: work.error,', 'error: work.error, client: Work.clientId(workController.state), taskClient: work.task ? (work.task.client_id || "codex") : "",'))
            transport = root / "qml/omarchy/WorkController.qml"
            transport.write_text(transport.read_text().replace('WorkRpc { id: rpc;', 'WorkRpc { id: rpc; responseTimeoutMs: 250;'))
            state = root / "fixture.json"
            initial = {"readiness": {"ready": True, "reason": "Ready for a job", "energy": 100, "sleeping": False},
                       "provider": {"id": "codex", "name": "Codex", "available": True},
                       "clients": [{"id": key, "name": name, "available": True} for key,name in
                                   (("codex", "Codex"), ("claude-code", "Claude Code"), ("opencode", "OpenCode"))],
                       "scout": {"enabled": True, "next_due_ms": 0}, "task": None}
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
    assert p['client_id']==s['provider']['id']
    if (root/'reject-start').exists():
        print(json.dumps({'ok':False,'error':{'message':'Fixture start refused'}}));sys.exit(0)
    task_id='job-fixture-next' if s['task'] else 'job-fixture'
    s['task']={'id':task_id,'client_id':p['client_id'],'client_name':s['provider']['name'],'kind':p['kind'],'project':p['project'],'prompt':p['prompt'],'status':'running','progress':'Checking files','output':'<b>literal</b>','changes':[],'reward_xp':20,'reward_need':'hunger'}
elif method=='client-select':
    if (root/'reject-selection').exists():
        (root/'reject-selection').unlink()
        print(json.dumps({'ok':False,'error':{'message':'Fixture selection refused'}}));sys.exit(0)
    s['provider']=next(c for c in s['clients'] if c['id']==p['client_id'])
elif method=='fix':
    assert p['client_id']==s['provider']['id'] and p['task_id']==s['task']['id'] and p['review_digest']==s['task']['review_digest']
    print(json.dumps({'ok':False,'error':{'message':'Fixture rejected repair before starting'}}));sys.exit(0)
elif method=='accept':
    assert p=={'task_id':'job-fixture','review_digest':'digest-fixture','acknowledge_unverified':True}
    s['task']['status']='accepted'
elif method=='cancel': s['task']['status']='cancelled'
elif method=='dismiss':
    if (root/'reject-dismiss').exists():
        (root/'reject-dismiss').unlink()
        print(json.dumps({'ok':False,'error':{'message':'Fixture dismissal refused'}}));sys.exit(0)
    if 'task_id' in p:
        assert p=={'task_id':s['task']['id']}
        s['task']['acknowledged']=True
    else:
        assert p=={'scout_due_ms':s['scout']['next_due_ms']}
        s['scout']['last_error_acknowledged']=True
(root/'fixture.json').write_text(json.dumps(s))
if s.get('task',{} ) and s['task'].get('acknowledged'): s['task']=None
if s.get('scout',{}).get('last_error_acknowledged'): s['scout'].update(last_error='',notice='')
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
                self.fail("Work window did not settle:\n" + log.read() + "\nSTATUS: " + str(status())
                          + "\nCALLS: " + (root / "calls.jsonl").read_text())

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
                (root / "reject-selection").touch()
                ipc("testSelect", "opencode")
                wait(lambda: "selection refused" in status()["error"] and not status()["busy"])
                self.assertEqual(status()["client"], "codex", "failed save retains confirmed choice")
                ipc("testSelect", "opencode")
                wait(lambda: status()["client"] == "opencode" and not status()["busy"])
                time.sleep(.35)
                requests = [json.loads(line) for line in (root / "calls.jsonl").read_text().splitlines()]
                self.assertFalse(any(r["method"] in ("scout", "start", "fix") for r in requests), "selection must not dispatch a due scout")
                self.assertEqual(json.loads(state.read_text())["scout"]["next_due_ms"], 0, "selection cannot reset cooldown")
                s = json.loads(state.read_text()); s["provider"] = s["clients"][1]; state.write_text(json.dumps(s))
                ipc("testObservedChoice", "claude-code")
                wait(lambda: status()["client"] == "claude-code" and not status()["busy"])
                time.sleep(.35)
                requests = [json.loads(line) for line in (root / "calls.jsonl").read_text().splitlines()]
                self.assertFalse(any(r["method"] in ("scout", "start", "fix") for r in requests), "another window's choice must not dispatch work")
                ipc("testNoAuto")
                stop(); start()
                self.assertEqual(status()["client"], "claude-code", "new UI process rehydrates saved selection")
                s = json.loads(state.read_text())
                s["scout"] = {"enabled": True, "next_due_ms": 9999999999999,
                              "notice": "The last bug check could not start.",
                              "last_error": "PRIVATE_FIXTURE: file exceeds the input limit"}
                state.write_text(json.dumps(s))
                ipc("testRefresh")
                wait(lambda: bool(status()["notice"]) and not status()["error"] and not status()["busy"])
                stop(); start()
                wait(lambda: bool(status()["notice"]) and not status()["error"] and not status()["busy"])
                ipc("testRedactedSummary")
                wait(lambda: status().get("privateNotice") == "PRIVATE_FIXTURE: file exceeds the input limit" and not status()["busy"])
                # Repeat after the summary transition settled: this merge must not erase hydrated detail.
                ipc("testRedactedSummary")
                self.assertEqual(status().get("privateNotice"), "PRIVATE_FIXTURE: file exceeds the input limit")
                notice_key = ipc("testScoutKey")
                ipc("testDismiss")
                wait(lambda: not status()["notice"] and not status()["busy"])
                ipc("testOldScoutSummary")
                wait(lambda: not status()["visible"] and not status()["notice"])
                ipc("testReopen")
                wait(lambda: status()["visible"] and not status()["notice"] and not status()["busy"])
                stop(); start()
                wait(lambda: not status()["notice"] and not status()["busy"])
                self.assertEqual(ipc("testScoutKey"), notice_key, "acknowledgement cannot rearm a scout attempt")
                saved = json.loads(state.read_text())["scout"]
                self.assertEqual(saved["last_error"], "PRIVATE_FIXTURE: file exceeds the input limit")
                self.assertTrue(saved["last_error_acknowledged"])
                s["scout"].update(last_error="", notice="")
                state.write_text(json.dumps(s))
                ipc("testRefresh")
                wait(lambda: not status()["error"] and not status()["busy"])
                ipc("testStart")
                wait(lambda: status()["task"] == "running" and not status()["busy"])
                self.assertEqual(json.loads(state.read_text())["task"]["prompt"], "literal $(no shell)")
                self.assertEqual(status()["taskClient"], "claude-code")
                ipc("testSelect", "opencode")
                wait(lambda: status()["client"] == "opencode" and not status()["busy"])
                self.assertEqual(status()["taskClient"], "claude-code", "changing next client cannot relabel the running job")
                ipc("testNoAuto")
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
                ipc("testNew")
                wait(lambda: status()["task"] is None and not status()["busy"])
                ipc("testStart")
                wait(lambda: status()["task"] == "running" and not status()["busy"])
                ipc("testCancel")
                wait(lambda: status()["task"] == "cancelled" and not status()["busy"])
                (root / "reject-dismiss").touch()
                ipc("testDismiss")
                wait(lambda: "refused" in status()["error"] and not status()["busy"])
                self.assertEqual(status()["task"], "cancelled", "refused acknowledgement cannot hide a result")
                ipc("testDismiss"); ipc("testDismiss")
                wait(lambda: status()["task"] is None and not status()["busy"])
                ipc("testOldSummary")
                wait(lambda: not status()["visible"] and status()["task"] is None)
                ipc("testReopen")
                wait(lambda: status()["visible"] and status()["task"] is None and not status()["busy"])
                stop(); start()
                wait(lambda: status()["task"] is None and not status()["busy"])
                requests = [json.loads(line) for line in (root / "calls.jsonl").read_text().splitlines()]
                self.assertFalse(any(r["method"] == "scout" for r in requests), "dismiss/reopen never launches work")
                self.assertEqual(sum(r["method"] == "start" for r in requests), 2)
                saved = json.loads(state.read_text())
                self.assertEqual(saved["task"]["status"], "cancelled", "dismiss preserves terminal evidence")
                saved["task"] = {"id":"fresh-failure","status":"failed","kind":"bug","error":"Fresh failure"}
                state.write_text(json.dumps(saved))
                ipc("testRefresh")
                wait(lambda: status()["task"] == "failed" and not status()["busy"])
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
                saved = json.loads(state.read_text())
                saved["task"] = {"id":"finding","status":"scouted","has_finding":True,"review_digest":"fixture-digest","client_id":"claude-code"}
                state.write_text(json.dumps(saved)); ipc("testRefresh")
                wait(lambda: status()["task"] == "scouted" and not status()["busy"])
                ipc("testFix"); wait(lambda: bool(status()["error"]) and not status()["busy"])
                ipc("testFix"); wait(lambda: bool(status()["error"]) and not status()["busy"])
                ipc("testSelect", "codex"); wait(lambda: status()["client"] == "codex" and not status()["busy"])
                ipc("testFix"); wait(lambda: bool(status()["error"]) and not status()["busy"])
                repairs = [json.loads(line)["params"] for line in (root / "calls.jsonl").read_text().splitlines() if json.loads(line)["method"] == "fix"]
                self.assertEqual(repairs[0]["client_id"], "opencode")
                self.assertEqual(repairs[2]["client_id"], "codex")
                self.assertEqual(repairs[0]["request_id"], repairs[1]["request_id"], "same repair retry keeps request identity")
                self.assertNotEqual(repairs[0]["request_id"], repairs[2]["request_id"], "different executor cannot replay an old repair request")
                saved = json.loads(state.read_text()); saved["task"] = None; state.write_text(json.dumps(saved))
                ipc("testRefresh"); wait(lambda: status()["task"] is None and not status()["busy"])
                (root / "reject-start").touch()
                ipc("testStart"); wait(lambda: "start refused" in status()["error"] and not status()["busy"])
                ipc("testStart"); wait(lambda: "start refused" in status()["error"] and not status()["busy"])
                ipc("testSelect", "opencode"); wait(lambda: status()["client"] == "opencode" and not status()["busy"])
                ipc("testStart"); wait(lambda: "start refused" in status()["error"] and not status()["busy"])
                starts = [json.loads(line)["params"] for line in (root / "calls.jsonl").read_text().splitlines() if json.loads(line)["method"] == "start"][-3:]
                self.assertEqual(starts[0]["request_id"], starts[1]["request_id"])
                self.assertNotEqual(starts[0]["request_id"], starts[2]["request_id"], "draft identity changes with the expected client")
                self.assertFalse(any(json.loads(line)["method"] == "scout" for line in (root / "calls.jsonl").read_text().splitlines()), "no selection, refresh or reopen dispatches a scout")
                saved = json.loads(state.read_text()); saved["scout"]["next_due_ms"] = int(time.time()*1000)+2000
                state.write_text(json.dumps(saved)); ipc("testRefresh")
                wait(lambda: not status()["busy"])
                ipc("testObservedChoice", "opencode")
                ipc("testSelect", "codex"); wait(lambda: status()["client"] == "codex" and not status()["busy"])
                time.sleep(.1)
                self.assertFalse(any(json.loads(line)["method"] == "scout" for line in (root / "calls.jsonl").read_text().splitlines()), "selection cannot accelerate a future deadline")
                wait(lambda: any(json.loads(line)["method"] == "scout" for line in (root / "calls.jsonl").read_text().splitlines()))
            finally:
                stop()
            log.seek(0)
            output = log.read()
            log.close()
            for error in ("TypeError:", "ReferenceError:", "Failed to load configuration", "Binding loop", "Cannot assign"):
                self.assertNotIn(error, output)


if __name__ == "__main__":
    unittest.main(verbosity=2)
