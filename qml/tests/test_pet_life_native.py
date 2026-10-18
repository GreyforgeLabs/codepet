#!/usr/bin/env python3
"""Actual pointer/render regression; synthetic state only, configurable shipped UI roots."""
import argparse
import json
import os
from pathlib import Path
import tempfile

REPO = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser()
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--runtime', type=Path, default=REPO)
parser.add_argument('--plugin', type=Path, default=REPO / 'omarchy-plugin')
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
for key in ("WAYLAND_DISPLAY", "DISPLAY", "QT_QPA_PLATFORMTHEME"):
    os.environ.pop(key, None)
os.environ["QT_QPA_PLATFORM"] = "offscreen"
os.environ["QT_FORCE_STDERR_LOGGING"] = "1"
private = tempfile.TemporaryDirectory(prefix='native-', dir=args.output)
for key, suffix in [('HOME', 'home'), ('XDG_STATE_HOME', 'state'), ('XDG_CONFIG_HOME', 'config'),
                    ('XDG_CACHE_HOME', 'cache'), ('XDG_RUNTIME_DIR', 'runtime')]:
    path = Path(private.name) / suffix
    path.mkdir(mode=0o700)
    os.environ[key] = str(path)

from PySide6.QtCore import QObject, QPoint, QPointF, QUrl  # noqa: E402
from PySide6.QtGui import QGuiApplication, QAccessible, Qt  # noqa: E402
from PySide6.QtQml import QQmlApplicationEngine  # noqa: E402
from PySide6.QtQuick import QQuickWindow, QQuickItem  # noqa: E402
from PySide6.QtTest import QTest  # noqa: E402

app = QGuiApplication([])
engine = QQmlApplicationEngine()
engine.addImportPath(str(REPO / 'qml/tests/stubs'))
source = (REPO / 'qml/tests/PetLifeNative.qml').read_text()
source = source.replace('"../shared"', '"' + (args.runtime / 'qml/shared').resolve().as_uri() + '"')
source = source.replace('"../../omarchy-plugin"', '"' + args.plugin.resolve().as_uri() + '"')
source = source.replace('"../../omarchy-plugin/petstate.js"', '"' + (args.plugin / 'petstate.js').resolve().as_uri() + '"')
entry = Path(private.name) / 'PetLifeNative.qml'
entry.write_text(source)
engine.load(QUrl.fromLocalFile(str(entry)))
assert engine.rootObjects(), 'fixture must load the actual selected UI'
window = engine.rootObjects()[0]
assert isinstance(window, QQuickWindow)
window.setProperty('registryInput', json.loads((args.runtime / 'qml/assets/pets/registry.json').read_text()))
window.setProperty('runtimeBaseInput', args.runtime.resolve().as_uri() + '/')
def find(name, root=None):
    todo = [(root or window).contentItem()]
    seen = []
    while todo:
        item = todo.pop()
        if item in seen: continue
        seen.append(item)
        if item.objectName() == name: return item
        todo.extend(item.childItems() if isinstance(item, QQuickItem) else item.children())
    return None

pet = find('native.pet')
popup = find('native.popup')
receipt = {'runtime': str(args.runtime), 'plugin': str(args.plugin), 'checks': []}

def value(obj, key):
    result = obj.property(key)
    return result.toVariant() if hasattr(result, 'toVariant') else result

def check(condition, name):
    receipt['checks'].append({'check': name, 'pass': bool(condition)})
    assert condition, name

def click(obj, button=Qt.LeftButton):
    assert obj is not None
    point = obj.mapToScene(QPointF(obj.width()/2, obj.height()/2))
    QTest.mouseClick(window, button, pos=QPoint(round(point.x()), round(point.y())))
    app.processEvents()

def pet_click(button):
    r = pet.property('petHit')
    QTest.mouseClick(window, button, pos=QPoint(round(r.x()+r.width()/2), round(r.y()+r.height()/2)))
    app.processEvents()

def wheel(action):
    if not pet.property('wheelOpen'): pet_click(Qt.RightButton)
    click(find('desktopPet.wheel.' + action))
    check(not pet.property('wheelOpen') and pet.property('wheelHit').width() == 0, action + ' pointer selection immediately closes wheel and input island')

def shot(name):
    QTest.qWait(50)
    assert window.grabWindow().save(str(args.output / (name + '.png')))

def stage_shot(name):
    target = find('cp.stage')
    point = target.mapToScene(QPointF(0, 0))
    assert window.grabWindow().copy(round(point.x()), round(point.y()), round(target.width()), round(target.height())).save(str(args.output / (name + '.png')))

try:
    QTest.qWait(80)
    pet_click(Qt.RightButton)
    check(pet.property('wheelOpen') and pet.property('pinned'), 'right click opens wheel without changing pin')
    check(pet.property('interactiveOpen') and find('desktopPet.wheel.pet').hasActiveFocus(), 'explicit wheel open gives initial keyboard focus to Pet')
    QTest.keyClick(window, Qt.Key_Escape); app.processEvents()
    check(not pet.property('wheelOpen') and not pet.property('interactiveOpen'), 'Escape closes wheel and releases interactive focus policy')
    pet_click(Qt.RightButton)
    for _ in range(7): QTest.keyClick(window, Qt.Key_Tab)
    check(find('desktopPet.wheel.close').hasActiveFocus(), 'Tab reaches wheel close')
    QTest.keyClick(window, Qt.Key_Space); app.processEvents()
    check(not pet.property('wheelOpen'), 'Space operates wheel close')
    pet_click(Qt.LeftButton)
    check(pet.property('interactiveOpen') and find('workCard.close').hasActiveFocus(), 'explicit card open focuses its safe close action')
    QTest.keyClick(window, Qt.Key_Tab); QTest.keyClick(window, Qt.Key_Space); app.processEvents()
    check(find('workClient.detail').parentItem().isVisible(), 'keyboard opens the inline client chooser')
    QTest.keyClick(window, Qt.Key_Escape); app.processEvents()
    check(pet.property('cardOpen') and find('workClient.toggle').hasActiveFocus(), 'first Escape collapses chooser and returns focus to its toggle')
    QTest.keyClick(window, Qt.Key_Escape); app.processEvents()
    check(not pet.property('cardOpen') and not pet.property('interactiveOpen'), 'next Escape closes the card and releases interactive focus policy')
    pet_click(Qt.RightButton); pet.setProperty('surfaceActive', False); app.processEvents()
    check(not pet.property('interactiveOpen') and not pet.property('wheelOpen') and not pet.property('cardOpen'), 'hidden surface dismisses islands and releases interactive policy')
    pet.setProperty('surfaceActive', True); app.processEvents()
    pet_click(Qt.RightButton)
    bath = find('desktopPet.wheel.wash')
    check(bath is not None and value(bath, 'modelData')['label'] == 'Bath', 'Bath is present and clearly labelled on the wheel')
    bath_images = [child for child in bath.childItems() if child.property('source') is not None]
    check(len(bath_images) == 1 and '/care-floor/wash.webp' in bath_images[0].property('source').toString()
          and window.imageReady(bath_images[0]), 'Bath renders the existing wash artwork')
    actions = ['pet', 'food', 'water', 'play', 'wash', 'sleep', 'pin', 'close']
    boxes = []
    for action in actions:
        target = find('desktopPet.wheel.' + action)
        assert target is not None
        point = target.mapToScene(QPointF(0, 0))
        boxes.append((action, point.x(), point.y(), target.width(), target.height()))
    check(all(a[1] >= b[1]+b[3] or a[1]+a[3] <= b[1] or a[2] >= b[2]+b[4] or a[2]+a[4] <= b[2]
              for i, a in enumerate(boxes) for b in boxes[i+1:]), 'all seven action targets and close are separate')
    receipt['wheel_targets'] = boxes
    shot('wheel')
    corners = []
    for index, (x, y) in enumerate([(0, 0), (888, 0), (0, 608), (888, 608)]):
        pet.setProperty('petX', x); pet.setProperty('petY', y); app.processEvents()
        r = pet.property('wheelHit')
        check(r.x() >= 0 and r.y() >= 0 and r.right() <= 1000 and r.bottom() <= 720, 'wheel corner ' + str(index))
        corners.append([r.x(), r.y(), r.width(), r.height()])
        shot('wheel-corner-' + str(index))
    receipt['wheel_rects'] = corners
    window.setWidth(280); window.setHeight(420); app.processEvents()
    pet.setProperty('petX', 168); pet.setProperty('petY', 308); app.processEvents()
    r = pet.property('wheelHit')
    check(r.x() >= 0 and r.y() >= 0 and r.right() <= 280 and r.bottom() <= 420, 'wheel clamps inside narrow 280px surface')
    shot('wheel-narrow')
    window.setWidth(1000); window.setHeight(720); app.processEvents()
    click(find('desktopPet.wheel.close'))
    check(not pet.property('wheelOpen') and pet.property('wheelHit').width() == 0, 'close clears wheel hit island')
    window.reset(); app.processEvents()
    before = len(value(window, 'calls'))
    wheel('pet'); shot('pet-start'); QTest.qWait(450); shot('pet-middle'); QTest.qWait(1400); shot('pet-end')
    check(len(value(window, 'calls')) == before and value(window, 'doc')['life']['xp'] == 75, 'Pet is cosmetic and gives no XP')
    wheel('pet'); check(pet.property('transientElapsedMs') == 0, 'repeated Pet restarts happy reaction')
    QTest.qWait(450); wheel('pet')
    check(pet.property('transientElapsedMs') == 0, 'Pet can restart before its previous reaction finishes')
    pet.clearTransients(); pet.setProperty('heartMsLeft', 0)
    for action, meter in [('food', 'hunger'), ('water', 'thirst'), ('play', 'mental_health'), ('wash', 'hygiene')]:
        doc = value(window, 'doc'); doc['meters'][meter] = 2500; window.setSnapshot(json.dumps(doc))
        before_care = len(value(window, 'calls'))
        window.setProperty('holdReply', True); wheel(action)
        check(len(value(window, 'calls')) == before_care + 1 and value(window, 'calls')[-1] == {'method': 'pet.care', 'params': {'action': action}}, action + ' click sends exactly its care request')
        wheel('water')
        check(len(value(window, 'calls')) == before_care + 1 and pet.property('careBusy'), 'reopened busy wheel closes without a second request')
        check(pet.property('careBusy') and pet.property('transientState') == '', action + ' waits for authoritative response')
        window.release(); window.setProperty('holdReply', False); app.processEvents()
        check(pet.property('transientState') == action and value(window, 'doc')['meters'][meter] == 10000, action + ' reacts after confirmed refill')
        check(not pet.property('wheelOpen'), action + ' late confirmation does not reopen wheel')
        check(value(window, 'doc')['life']['xp'] == 75, action + ' grants no XP')
        check(value(find('desktopPet.workCard'), 'meters')[meter] == 10000, action + ' confirmed meter also reaches the compact status strip')
        shot('care-' + action); pet.clearTransients()
    wheel('play'); app.processEvents()
    check(pet.property('transientState') == 'play' and pet.property('careMessage') == 'Already feeling good'
          and value(window, 'doc')['life']['xp'] == 75, 'full-mood confirmed Play still reacts without refill or XP')
    pet.clearTransients()
    wheel('sleep'); app.processEvents()
    check(value(window, 'doc')['care']['sleeping'] and not pet.property('motionRunning'), 'confirmed Nap stops awake motion')
    wheel('sleep'); app.processEvents()
    check(not value(window, 'doc')['care']['sleeping'], 'same wheel control becomes Wake')
    pet.clearTransients()
    paused = value(window, 'doc'); paused['life']['paused'] = True
    window.setSnapshot(json.dumps(paused)); app.processEvents(); pet_click(Qt.RightButton)
    check(value(find('desktopPet.wheel.sleep'), 'modelData')['label'] == 'Resume', 'paused resting art offers Resume rather than Wake')
    click(find('desktopPet.wheel.sleep')); app.processEvents()
    check(value(window, 'calls')[-1] == {'method': 'pet.pause', 'params': {'paused': False}}
          and not value(window, 'doc')['life']['paused'] and pet.property('careMessage') == 'Resumed', 'Resume uses the existing unpause route and confirms its actual meaning')
    fainted = value(window, 'doc'); fainted['life']['fainted'] = True
    fainted['meters'].update(hunger=0, thirst=0)
    window.setSnapshot(json.dumps(fainted)); app.processEvents(); pet_click(Qt.RightButton)
    nap = find('desktopPet.wheel.sleep')
    check(value(nap, 'modelData')['label'] == 'Nap' and not nap.isEnabled()
          and 'Snack + Drink' in find('desktopPet.wheel.message').property('text'), 'fainted resting art guides recovery care and cannot offer a false Wake')
    before = len(value(window, 'calls')); click(nap)
    check(len(value(window, 'calls')) == before and pet.property('wheelOpen'), 'disabled fainted Nap never sends sleep or wake')
    wheel('food'); check(value(window, 'calls')[-1] == {'method': 'pet.care', 'params': {'action': 'food'}}, 'fainted Snack remains an available recovery request')
    wheel('water'); check(value(window, 'calls')[-1] == {'method': 'pet.care', 'params': {'action': 'water'}}, 'fainted Drink remains an available recovery request')
    window.reset(); app.processEvents(); pet.clearTransients()
    window.setProperty('holdReply', True); wheel('food')
    check(find('desktopPet.careFeedback').isVisible() and pet.property('careBusy'), 'pending care has visible feedback without a reward')
    QTest.qWait(6150)
    check(not find('desktopPet.careFeedback').isVisible() and pet.property('careBusy'), 'pending toast expires within its six-second bound without pretending the request finished')
    window.release(); window.setProperty('holdReply', False); app.processEvents()
    check(find('desktopPet.careFeedback').isVisible(), 'late confirmed reply gets a fresh bounded feedback period')
    pet.clearTransients()
    window.setProperty('failReply', True); wheel('food'); app.processEvents()
    check(pet.property('transientState') == '' and 'Try again' in pet.property('careMessage'), 'failed care never celebrates')
    QTest.qWait(100)
    check(not pet.property('wheelOpen') and not pet.property('careBusy'), 'failed reply leaves the wheel closed and frees the pending action')
    check(find('desktopPet.careFeedback').isVisible()
          and find('desktopPet.careFeedbackText').property('text') == 'Try again from Care',
          'failed care remains visibly explained after wheel autoclose')
    shot('care-failure-feedback')
    accessible = QAccessible.queryAccessibleInterface(find('desktopPet.careFeedback'))
    check(accessible is not None and accessible.role() == QAccessible.Role.StaticText
          and accessible.text(QAccessible.Text.Name) == 'Try again from Care', 'care feedback exposes its status text to accessibility without taking focus')
    QTest.qWait(3000); window.setSnapshot(json.dumps(value(window, 'doc'))); app.processEvents(); QTest.qWait(3150)
    check(not find('desktopPet.careFeedback').isVisible() and pet.property('careMessage') == '', 'failure toast expires within six seconds despite snapshot refresh')
    window.setProperty('holdReply', True); wheel('food'); pet.setProperty('surfaceActive', False); app.processEvents()
    window.release(); window.setProperty('holdReply', False); app.processEvents(); pet.setProperty('surfaceActive', True); app.processEvents()
    check(not find('desktopPet.careFeedback').isVisible() and not pet.property('careBusy'), 'late reply while hidden cannot replay old feedback on show')
    window.setProperty('failReply', False); pet.setProperty('wheelOpen', False)
    doc = value(window, 'doc'); doc['meters']['hunger'] = 6000
    doc['agent']['apps'] = [{'id': 'codex', 'state': 'working', 'since_ms': 1700007000000, 'stale_at_ms': 1700008000000}]
    window.setSnapshot(json.dumps(doc)); window.setProperty('workState', {'task': {'status': 'running', 'source_kind': 'scout_fix'}})
    check(value(pet, 'face')['state'] == 'need_soon' and 'food.webp' in ','.join(value(pet, 'bubbleIcons')), 'need cue survives owned and observed work')
    shot('needs-with-work')
    window.reset(); app.processEvents()
    observed = value(window, 'doc')
    observed['agent']['apps'] = [{'id': 'codex', 'state': 'working', 'since_ms': 1700007100000, 'stale_at_ms': 1700007200500}]
    window.setSnapshot(json.dumps(observed)); app.processEvents(); pet.clearTransients(); pet.refreshClock()
    check(pet.property('observerLabel') == 'Codex working', 'observer bubble names the separate Codex activity')
    bubble = find('desktopPet.bubble'); sprite = find('desktopPet.sprite')
    bubble_point = bubble.mapToScene(QPointF(0, 0)); sprite_point = sprite.mapToScene(QPointF(0, 0))
    check(bubble_point.y()+bubble.height() <= sprite_point.y() or bubble_point.y() >= sprite_point.y()+sprite.height(), 'observer label sits outside the pet')
    shot('observer-working')
    window.setProperty('clock', 1700007200500); pet.refreshClock(); app.processEvents()
    check(pet.property('observerLabel') == '' and not bubble.property('visible'), 'observer label clears at the existing stale deadline')
    shot('observer-idle-clear')
    window.setProperty('clock', 1700007200000)
    observed['agent']['apps'] = []
    window.setSnapshot(json.dumps(observed)); pet.refreshClock(); app.processEvents()
    check(pet.property('observerLabel') == '', 'observer completion clears the label')
    window.reset(); window.setProperty('homeOpen', True); app.processEvents(); shot('home-75-xp'); stage_shot('growth-before-crop')
    check('Energy' in find('cp.energyLine').property('text'), 'Home retains secondary energy and readiness')
    before_pet = len(value(window, 'calls'))
    find('cp.pet').forceActiveFocus()
    QTest.keyClick(window, Qt.Key_Space); app.processEvents()
    check(popup.property('bubbleText') == 'Hee hee!' and len(value(window, 'calls')) == before_pet, 'keyboard Pet is accessible and cosmetic')
    click(find('cp.tile.play')); app.processEvents()
    check(popup.property('actionState') == 'play' and popup.property('bubbleText') == 'Pip played already!'
          and value(window, 'doc')['life']['xp'] == 75, 'Home full-mood Play confirms a cosmetic reaction and truthful copy without XP')
    initial_frame = find('cp.pet').property('rel')
    window.setProperty('holdReply', True); click(find('cp.treats'))
    check('+5 XP' not in popup.property('bubbleText'), 'treat does not claim reward before response')
    window.release(); window.setProperty('holdReply', False); app.processEvents()
    check(value(window, 'doc')['life']['stage'] == 2 and value(window, 'doc')['life']['xp'] == 80, 'confirmed 75 to 80 crosses stage 2')
    check('grew' in find('cp.statusLine').property('text'), 'Home actually displays growth')
    check(find('cp.treats.count').property('text') == 'Treats 0/10', 'last treat leaves visible empty jar')
    shot('home-growth-80-xp'); stage_shot('growth-during-crop')
    accent, portrait = find('cp.confetti'), find('cp.pet')
    check(accent.property('visible') and accent.width() <= 32 and accent.x() + accent.width() < portrait.x(), 'growth accent is small and fully outside the pet')
    speech = find('cp.speech')
    check(accent.y() >= speech.y() + speech.height(), 'growth accent is not obscured by the reward toast')
    QTest.qWait(900)
    window.setSnapshot(json.dumps(value(window, 'doc'))); app.processEvents()
    QTest.qWait(850)
    check(not find('cp.confetti').property('visible'), 'growth accent expires within two seconds despite refreshed snapshot')
    shot('home-growth-expired'); stage_shot('growth-after-crop')
    popup.go('more', {}); app.processEvents(); popup.home(); app.processEvents()
    check(not find('cp.confetti').property('visible'), 'reopening Home does not replay the same growth event')
    stable_growth = value(window, 'doc')
    window.setProperty('calm', True)
    calm_growth = json.loads(json.dumps(stable_growth)); calm_growth['unseen_events'] = [{'seq': 3, 'kind': 'evolved'}]
    window.setSnapshot(json.dumps(calm_growth)); app.processEvents()
    check(not find('cp.confetti').property('visible'), 'Calm suppresses a new growth accent')
    window.setProperty('calm', False); window.setSnapshot(json.dumps(stable_growth)); app.processEvents()
    check(not find('cp.confetti').property('visible'), 'older growth snapshots cannot replay after a newer presentation')
    QTest.qWait(1050)
    check(find('cp.pet').property('rel') != initial_frame, 'stable stage 2 art differs after care animation')
    saved = value(window, 'doc'); saved['unseen_events'] = []; window.setSnapshot(json.dumps(saved)); app.processEvents()
    shot('home-stage-2-stable')
    check(find('cp.pet').property('stage') == 2, 'saved 80 XP state retains stage 2')
    receipt['saved_state'] = {'xp': saved['life']['xp'], 'stage': saved['life']['stage'], 'jar': saved['treats']['jar']}
    saved_path = args.output / 'synthetic-saved-life.json'
    saved_path.write_text(json.dumps(saved))
    window.setProperty('homeOpen', False); window.reset(); app.processEvents()
    x, y = pet.property('petX'), pet.property('petY')
    expressions_before = pet.property('expressionCount')
    for _ in range(2):
        pet.setProperty('idleTicks', 0); pet.setProperty('resting', True); pet.updateInterval(); QTest.qWait(300); pet.clearTransients()
    check(pet.property('expressionCount') == expressions_before + 2 and pet.property('petX') == x and pet.property('petY') == y, 'pinned idle expressions recur without translation')
    wheel('pin'); check(not pet.property('pinned'), 'wheel explicitly enables Roam')
    pet.setProperty('wheelOpen', False); pet.setProperty('idleTicks', 0); pet.setProperty('resting', True); pet.cosmeticTick()
    for _ in range(10): pet.cosmeticTick()
    check(pet.property('petX') != x, 'unpinned pet walks')
    doc = value(window, 'doc'); doc['life']['appearance'] = 'goldfish'; window.setSnapshot(json.dumps(doc))
    pet.setProperty('pinned', True); x, y = pet.property('petX'), pet.property('petY'); pet.express(True); QTest.qWait(250)
    check(pet.property('hopOffset') == 0 and pet.property('wiggleAngle') == 0 and pet.property('petX') == x and pet.property('petY') == y, 'aquarium remains upright and stationary while petted')
    shot('aquatic-pet')
    window.setProperty('calm', True); pet.express(True); QTest.qWait(250)
    check(pet.property('hopOffset') == 0 and pet.property('wiggleAngle') == 0, 'Calm petting remains a static happy face')
    fresh = QQmlApplicationEngine(); fresh.addImportPath(str(REPO / 'qml/tests/stubs'))
    fresh.load(QUrl.fromLocalFile(str(entry)))
    reopened = fresh.rootObjects()[0]
    reopened.setProperty('registryInput', value(window, 'registryInput'))
    reopened.setProperty('runtimeBaseInput', args.runtime.resolve().as_uri() + '/')
    reopened.setSnapshot(saved_path.read_text())
    reopened.setProperty('homeOpen', True)
    QTest.qWait(100)
    check(find('cp.pet', reopened).property('stage') == 2 and '/s2/' in find('cp.pet', reopened).property('rel'), 'fresh UI reloads durable stage 2 artwork from saved 80 XP')
    assert reopened.grabWindow().save(str(args.output / 'home-stage-2-reopened.png'))
    reopened.close()
    print(json.dumps(receipt, indent=2))
finally:
    (args.output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    window.close()
    private.cleanup()
