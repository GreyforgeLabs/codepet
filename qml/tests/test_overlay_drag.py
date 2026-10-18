#!/usr/bin/env python3
"""Actual pointer events against the shipped scene, offscreen only."""
import os
from pathlib import Path
os.environ["QT_QPA_PLATFORM"] = "offscreen"
os.environ['QT_FORCE_STDERR_LOGGING'] = '1'
for key in ("WAYLAND_DISPLAY", "DISPLAY", "QT_QPA_PLATFORMTHEME"):
    os.environ.pop(key, None)
from PySide6.QtCore import QObject, QPoint, QPointF, QUrl
from PySide6.QtGui import QGuiApplication, Qt
from PySide6.QtQuick import QQuickWindow
from PySide6.QtQml import QQmlApplicationEngine
from PySide6.QtTest import QTest
app = QGuiApplication([])
engine = QQmlApplicationEngine()
engine.load(QUrl.fromLocalFile(str(Path(__file__).resolve().parents[1] / 'standalone/OverlayPreview.qml')))
window = engine.rootObjects()[0]
assert isinstance(window, QQuickWindow)
scene = window.findChild(QObject, 'overlayScene')
home = window.findChild(QObject, 'overlayHomeWindow')
window.setProperty('reducedMotion', True)
QTest.qWait(50)

def center():
    r = scene.property('petHit')
    return QPoint(round(r.x() + r.width()/2), round(r.y() + r.height()/2))

def drag(dx, dy):
    start = center()
    QTest.mousePress(window, Qt.LeftButton, pos=start)
    QTest.mouseMove(window, start + QPoint(dx, dy), delay=20)
    QTest.mouseRelease(window, Qt.LeftButton, pos=start + QPoint(dx, dy))
    app.processEvents()

def wheel_click(action):
    todo = [window.contentItem()]
    seen = []
    while todo:
        item = todo.pop()
        if item in seen: continue
        seen.append(item)
        if item.objectName() == 'desktopPet.wheel.' + action:
            p = item.mapToScene(QPointF(item.width()/2, item.height()/2))
            QTest.mouseClick(window, Qt.LeftButton, pos=QPoint(round(p.x()), round(p.y())))
            app.processEvents()
            return
        todo.extend(item.childItems())
    raise AssertionError('wheel control missing: ' + action)

start = scene.property('petHit')
drag(230, -220)
end = scene.property('petHit')
assert abs(end.x() - start.x() - 230) <= 2 and abs(end.y() - start.y() + 220) <= 2, 'drag must move in both axes'
assert not home.property('visible'), 'drag release must not open Home'
QTest.mouseClick(window, Qt.RightButton, pos=center())
app.processEvents()
assert scene.property('wheelOpen') is True and not scene.property('pinned'), 'right click opens care without changing pin'
wheel_click('pin')
assert scene.property('pinned') is True, 'explicit Pin action pins pet'
assert not scene.property('wheelOpen') and scene.property('wheelHit').width() == 0, 'Pin/Roam closes the wheel immediately'
window.setProperty('reducedMotion', False)
scene.setProperty('idleTicks', 0)
QTest.qWait(400)
assert not scene.property('canRoam') and not scene.property('walking'), 'pinned pet cannot wander'
drag(120, -100)
assert scene.property('pinned') is True and not home.property('visible'), 'manual drag works while pinned and keeps pin'
QTest.mouseClick(window, Qt.RightButton, pos=center())
app.processEvents()
assert scene.property('wheelOpen') is True and scene.property('pinned'), 'reopening wheel preserves pin'
wheel_click('pin')
assert scene.property('pinned') is False, 'explicit Roam action unpins pet'
assert not scene.property('wheelOpen') and scene.property('wheelHit').width() == 0, 'Pin/Roam closes the wheel immediately'
window.setProperty('face', 'working')
app.processEvents()
assert scene.property('canRoam'), 'active coding must not park an unpinned pet indefinitely'
QTest.mouseClick(window, Qt.LeftButton, pos=center())
app.processEvents()
assert home.property('visible'), 'ordinary click still opens Home'
home.setProperty('visible', False)
# Mouse stays within the window but pushes the sprite beyond its top/left bounds.
drag(-1000, -1000)
assert scene.property('petX') >= 0 and scene.property('petY') >= 0, 'drag clamps on screen'
window.close()
print('PASS: pointer drag, release vs click, pin/unpin, manual pinned move, working roam, edge clamp')
