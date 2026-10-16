#!/usr/bin/env python3
"""Offscreen preview lifecycle of the v0.3 desktop pet; does not certify compositor
focus/masking. A click opens Home (a stand-in window for the bar popup); hiding the pet
never opens anything."""
import os
from pathlib import Path

# Always offscreen, whatever the caller's session exports (a desktop session sets
# QT_QPA_PLATFORM=wayland;xcb): this test must never map a window on the live desktop.
os.environ["QT_QPA_PLATFORM"] = "offscreen"
for var in ("WAYLAND_DISPLAY", "DISPLAY", "QT_QPA_PLATFORMTHEME"):
    os.environ.pop(var, None)

from PySide6.QtCore import QObject, QMetaObject, Qt, QUrl  # noqa: E402
from PySide6.QtGui import QGuiApplication  # noqa: E402
from PySide6.QtQml import QQmlApplicationEngine  # noqa: E402
app = QGuiApplication([])
engine = QQmlApplicationEngine()
engine.load(QUrl.fromLocalFile(str(Path(__file__).resolve().parents[1] / "standalone/OverlayPreview.qml")))
assert engine.rootObjects(), "preview must load"
root = engine.rootObjects()[0]
scene = root.findChild(QObject, "overlayScene")
home = root.findChild(QObject, "overlayHomeWindow")
assert scene is not None and home is not None
app.processEvents()
assert scene.property("shown"), "the preview pet is drawn"
assert not home.property("visible"), "Home must not open on startup"
QMetaObject.invokeMethod(scene, "petClicked", Qt.DirectConnection)
app.processEvents()
assert home.property("visible"), "a click on the pet opens Home"
assert scene.property("heartMsLeft") > 0, "with a heart bubble"
home.setProperty("visible", False)
app.processEvents()
assert scene.property("visible"), "closing Home leaves the pet alive"
scene.setProperty("visible", False)
app.processEvents()
assert not scene.property("active"), "a hidden pet is inactive"
assert not home.property("visible"), "hiding never opens Home"
scene.setProperty("visible", True)
app.processEvents()
assert not home.property("visible"), "showing the pet never opens Home"
for face in ("working", "needs you", "hungry", "asleep", "all done", "happy"):
    root.setProperty("face", face)
    app.processEvents()
    assert scene.property("shown"), face
print("PASS: click opens Home, close, hide, restore and every preview face (9 checks)")
root.close()
