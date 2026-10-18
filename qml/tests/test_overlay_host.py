#!/usr/bin/env python3
"""Quickshell overlay logic + preference persistence, isolated and offscreen.
PanelWindow is replaced by FloatingWindow: compositor masking is not tested here.
Pointer behavior is covered separately by test_overlay_drag.py.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import sys

QS = shutil.which("qs")
if not QS:
    print("SKIP: Quickshell unavailable; overlay host persistence untested")
    sys.exit(0)

REPO = Path(__file__).resolve().parents[2]
with tempfile.TemporaryDirectory(prefix='cpmove') as td:
    root = Path(td)
    home = root / 'home'
    settings = home / '.config/omarchy/shell.json'
    state = home / '.local/state/codepet/bar-state.json'
    for p in (settings.parent, state.parent, root / 'run', root / 'bin'):
        p.mkdir(parents=True, mode=0o700)
    def entry(**values):
        settings.write_text(json.dumps({'version': 1, 'bar': {'layout': {'right': [
            {'id': 'greyforge.codepet', 'overlayOnLoad': True, **values}]}}}))
    entry(desktopPlacement={'x': .25, 'y': .5, 'pinned': True, 'screen': ''})
    state.write_text((REPO / 'docs/v0.3/contracts/fixtures/bar-v2/idle-happy.json').read_text())
    work_state = state.with_name('work-state.json')
    work_doc = {"readiness": {"ready": True, "reason": "Ready", "energy": 100, "sleeping": False},
                "provider": {"available": True, "name": "Codex"}, "scout": {"enabled": False, "next_due_ms": 0}, "task": None}
    work_state.write_text(json.dumps(work_doc))
    provider = root / 'bin/codepet'
    provider.write_text("""#!/usr/bin/python3
import json,os,sys,time
from pathlib import Path
if sys.argv[1:]==['rpc']:
    req=json.loads(sys.stdin.readline());r=Path(os.environ['FIXTURE_ROOT'])
    assert req['method']=='pet.care' and req['params']['action']=='food'
    (r/'care-call.json').write_text(json.dumps(req))
    print(json.dumps({'v':1,'id':req['id'],'ok':True,'result':{'changed':True}}))
    sys.exit(0)
assert sys.argv[1:]==['work-rpc']
r=Path(os.environ['FIXTURE_ROOT']);p=r/'home/.local/state/codepet/work-state.json'
req=json.loads(sys.stdin.readline());m=req['method'];params=req['params']
with (r/'calls.jsonl').open('a') as f:f.write(json.dumps(req)+'\\n')
s=json.loads(p.read_text())
if s['readiness'].get('sleeping'): s['readiness']['sleep_remaining_ms']=max(0,s['readiness'].get('sleep_remaining_ms',10000)-1)
if m=='scout':
    assert s['scout']['enabled'] and s['readiness']['ready']
    s['scout']['next_due_ms']=int(time.time()*1000)+21600000
    s['task']={'id':'scout-fixture','kind':'bug','source_kind':'scout','status':'running','progress':'Looking through local code','has_finding':False}
elif m=='fix':
    assert params['task_id']=='scout-fixture' and params['review_digest']=='finding-digest' and params['request_id']
    s['task']={'id':'fix-fixture','kind':'bug','source_kind':'scout_fix','status':'running','progress':'Preparing a candidate'}
elif m=='cancel': s['task']['status']='cancelled'
elif m=='scout-settings':s['scout']['enabled']=params['enabled']
# Real work-rpc republishes its redacted export on every get as well.
t=p.with_suffix('.new');t.write_text(json.dumps(s));t.replace(p)
if s.get('task') and s['task']['status']=='scouted':
    s['task'].update(project='/fixture/codepet',review_digest='finding-digest',finding={'title':'An empty search may crash','path':'search.rs','line':12,'evidence':'Provider evidence','check':'A proposed check','verification':'provider_reported'})
print(json.dumps({'ok':True,'result':s}))
""")
    provider.chmod(0o700)
    stage = root / 'qml'
    shutil.copytree(REPO / 'qml/shared', stage / 'shared')
    shutil.copytree(REPO / 'qml/omarchy', stage / 'omarchy')
    desktop_scene = stage / 'shared/DesktopPet.qml'
    desktop_scene.write_text(desktop_scene.read_text().replace('property bool cardOpen: false', 'property bool cardOpen: false\n    property alias testExpanded: card.expanded'))
    (stage / 'assets').symlink_to(REPO / 'qml/assets', target_is_directory=True)
    text = (REPO / 'qml/shell.qml').read_text().replace('function ping(): string {', '''
        function testDrag(): void { pet.beginDrag(0, 0); pet.moveDrag(90, -80); pet.endDrag() }
        function testPin(): void { pet.togglePinned() }
        function testFix(): void { work.fix() }
        function testCancel(): void { work.action("cancel", {task_id: work.task.id}) }
        function testCloseCard(): void { pet.cardOpen = false }
        function testDetails(): void { pet.testExpanded = true }
        function testPetClick(): void { pet.petClicked() }
        function testCareFood(): void { pet.chooseCare("food") }
        function testLeftTop(): void { pet.petX = 0; pet.petY = 0 }
        function testRightBottom(): void { pet.petX = 799; pet.petY = 599; pet.clampPosition() }
        function testUpdates(): void {
            var pending = [pet]
            while (pending.length) {
                var item = pending.pop()
                if (item.objectName === "workCard.updates") { item.tapped(); return }
                if (item.children) for (var i = 0; i < item.children.length; i++) pending.push(item.children[i])
            }
            throw new Error("Updates control missing")
        }
        function ping(): string {''')
    text = text.replace('"work_busy": work.busy,', '"work_busy": work.busy, "test_care_busy": pet.careBusy, "test_reaction": pet.transientState, "test_auto": work.automatic, "test_summary": work.summary, "test_state": work.state, "test_attempt": work.attemptedScoutKey, "test_expanded": pet.testExpanded,')
    # Offscreen has no layer-shell backend. Preserve the actual controller/scene and
    # adapt only the host surface; the installed compositor is checked separately.
    text = text.replace('PanelWindow {', 'FloatingWindow {').replace(
        'anchors { left: true; right: true; top: true; bottom: true }',
        'implicitWidth: 800; implicitHeight: 600')
    text = '\n'.join(line for line in text.splitlines() if 'WlrLayershell.' not in line and 'exclusiveZone:' not in line)
    (stage / 'shell.qml').write_text(text)
    # Only the explicit host preference write is intercepted. Every other invocation fails.
    fake = root / 'bin/qs'
    fake.write_text('''#!/usr/bin/python3
import sys,json,os
from pathlib import Path
a=sys.argv[1:]
assert a[:2]==['ipc','-p'] and a[3:9]==['call','--','shell','setBarWidget','greyforge.codepet','desktopPlacement'],a
p=Path(os.environ['HOME'])/'.config/omarchy/shell.json'
d=json.loads(p.read_text());d['bar']['layout']['right'][0]['desktopPlacement']=json.loads(a[9])
t=p.with_suffix('.new');t.write_text(json.dumps(d));t.replace(p)
print('ok')
''')
    fake.chmod(0o700)
    shell_stub = root / 'bin/omarchy-shell'
    shell_stub.write_text('''#!/usr/bin/python3
import json,os,sys
from pathlib import Path
assert sys.argv[1:]==['-q','greyforge.codepet','openUpdates']
(Path(os.environ['FIXTURE_ROOT'])/'updates-launch.json').write_text(json.dumps(sys.argv[1:]))
''')
    shell_stub.chmod(0o700)
    env = {"HOME": str(home), "PATH": str(root / 'bin') + ':/usr/bin:/bin',
           "XDG_RUNTIME_DIR": str(root / 'run'), "XDG_CONFIG_HOME": str(home / '.config'),
           "XDG_STATE_HOME": str(home / '.local/state'), "XDG_CACHE_HOME": str(home / '.cache'),
           "XDG_DATA_HOME": str(home / '.local/share'),
           "QT_QPA_PLATFORM": "offscreen", "QT_FORCE_STDERR_LOGGING": "1",
           "CODEPET_BIN": str(provider), "FIXTURE_ROOT": str(root)}
    log = (root / 'overlay.log').open('w+')
    proc = None
    def ipc(method):
        return subprocess.check_output([QS,'ipc','-p',str(stage / 'shell.qml'),
                                        'call','--','codepet',method],env=env,text=True,stderr=subprocess.DEVNULL,timeout=3).strip()
    def wait(predicate):
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            try:
                v = predicate()
                if v: return v
            except (subprocess.SubprocessError, json.JSONDecodeError): pass
            time.sleep(.05)
        log.seek(0)
        details = log.read()
        try:
            last_status = str(status())
        except (subprocess.SubprocessError, json.JSONDecodeError) as error:
            last_status = str(error)
        raise AssertionError('overlay check timed out\n' + details + '\n' + last_status)
    def status(): return json.loads(ipc('status'))
    def start():
        global proc
        proc = subprocess.Popen([QS,'-p',str(stage / 'shell.qml')],env=env,stdout=log,stderr=subprocess.STDOUT)
        wait(lambda: status().get('drawn'))
    def stop():
        if proc and proc.poll() is None:
            proc.terminate(); proc.wait(timeout=5)
    try:
        start()
        before = status()
        assert before['pinned'] and before['x'] > 0 and before['y'] > 0, before
        ipc('testDrag')
        moved = wait(lambda: (s if abs(s['x'] - before['x'] - 90) < 2 and abs(s['y'] - before['y'] + 80) < 2 else None)
                     if (s := status()) else None)
        wait(lambda: json.loads(settings.read_text())['bar']['layout']['right'][0]['desktopPlacement']['y'] != .5)
        assert moved['home_requests'] == 0 and moved['pinned'], moved
        stop(); start()
        restored = status()
        assert abs(restored['x']-moved['x']) < 2 and abs(restored['y']-moved['y']) < 2 and restored['pinned'], restored
        ipc('testPin')
        wait(lambda: not json.loads(settings.read_text())['bar']['layout']['right'][0]['desktopPlacement']['pinned'])
        assert not status()['pinned']
        # Changing the pin in More follows the watched host setting without moving the pet.
        position = json.loads(settings.read_text())['bar']['layout']['right'][0]['desktopPlacement']
        position['pinned'] = True
        entry(desktopPlacement=position)
        wait(lambda: status()['pinned'])
        assert abs(status()['x'] - moved['x']) < 2
        ipc('hide'); assert not status()['drawn']
        ipc('show'); wait(lambda: status()['drawn'])
        ipc('testCareFood')
        wait(lambda: (root/'care-call.json').exists() and not status()['test_care_busy'])
        assert json.loads((root/'care-call.json').read_text())['params']=={'action':'food'}
        assert status()['test_reaction']=='food', 'real host transport only reacts after its matching care receipt'
        # Only a ready, armed pet scouts automatically; no window/card opens itself.
        def publish(doc):
            tmp=work_state.with_suffix('.new');tmp.write_text(json.dumps(doc));tmp.replace(work_state)
        def calls():
            path=root/'calls.jsonl'
            return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []
        ipc('openCard')
        time.sleep(.4)
        reads_before=len([c for c in calls() if c['method']=='get'])
        time.sleep(.6)
        reads_after=len([c for c in calls() if c['method']=='get'])
        assert reads_before==reads_after, f'equivalent export rewrites must not poll an idle card: {reads_before} -> {reads_after}'
        ipc('testCloseCard')
        # Countdown values change on every get, but the visible nap poll owns cadence.
        work_doc['readiness'].update(ready=False,sleeping=True,sleep_remaining_ms=10000)
        publish(work_doc)
        wait(lambda: status()['test_state']['readiness'].get('sleeping'))
        nap_before=len([c for c in calls() if c['method']=='get'])
        ipc('openCard');time.sleep(1.6)
        nap_reads=len([c for c in calls() if c['method']=='get'])-nap_before
        assert 1<=nap_reads<=3, f'nap countdown rewrites must retain bounded polling: {nap_reads}'
        ipc('testCloseCard');wait(lambda: not status()['work_busy'])
        work_doc['readiness'].update(ready=True,sleeping=False,sleep_remaining_ms=0)
        reads_before_scout=len([c for c in calls() if c['method']=='get'])
        work_doc['scout']['enabled']=True
        publish(work_doc)
        wait(lambda: any(c['method']=='scout' for c in calls()))
        wait(lambda: not status()['work_busy'])
        assert not status()['card_open'], 'automatic finding must not open a card'
        time.sleep(1.5)
        assert len([c for c in calls() if c['method']=='scout'])==1, calls()
        assert len([c for c in calls() if c['method']=='get'])==reads_before_scout+1, 'closed card only reads once after attempt'
        result=json.loads(work_state.read_text())
        result['task']['status']='scouted';result['task']['has_finding']=True
        publish(result)
        ipc('openCard');wait(lambda: status()['card_open'] and not status()['work_busy'])
        ipc('testLeftTop')
        hit=status()['card_hit'];assert hit['x']>=0 and hit['y']>=0 and hit['x']+hit['width']<=800 and hit['y']+hit['height']<=600,hit
        ipc('testRightBottom')
        hit=status()['card_hit'];assert hit['x']>=0 and hit['y']>=0 and hit['x']+hit['width']<=800 and hit['y']+hit['height']<=600,hit
        ipc('testFix');wait(lambda: any(c['method']=='fix' for c in calls()))
        wait(lambda: not status()['work_busy'])
        ipc('testCancel');wait(lambda: any(c['method']=='cancel' for c in calls()))
        wait(lambda: not status()['work_busy'])
        ipc('testCloseCard');assert not status()['card_open']
        ipc('testPetClick');assert status()['card_open'], 'idle/terminal pet click opens compact card'
        assert status()['home_requests']==0, 'pet click does not unexpectedly open Home'
        wait(lambda: not status()['work_busy'])
        ipc('testDetails')
        terminal_reads=len([c for c in calls() if c['method']=='get'])
        publish(json.loads(work_state.read_text()))
        time.sleep(1.6)
        assert len([c for c in calls() if c['method']=='get'])==terminal_reads, 'terminal summary rewrites must not poll'
        assert status()['test_expanded'], 'terminal Details must stay expanded while reading'
        ipc('testUpdates')
        wait(lambda: (root/'updates-launch.json').exists())
        assert json.loads((root/'updates-launch.json').read_text())==['-q','greyforge.codepet','openUpdates']
        assert not status()['card_open'], 'Updates closes the card and navigates to the existing bar page'
        log.flush(); log.seek(0); output = log.read()
        for error in ('ReferenceError', 'TypeError', 'Cannot assign', 'Binding loop', 'placement save failed', 'Cannot read', 'failed to load'):
            assert error not in output, output
        print('PASS: overlay controller load, host preference write, restart restore, pin switch, hide/show, Updates navigation')
    finally:
        stop()
        log.close()
