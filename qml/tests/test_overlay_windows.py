#!/usr/bin/env python3
"""Offscreen preview lifecycle; does not certify compositor focus/masking."""
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
task = root.findChild(QObject, "overlayTaskWindow")
assert scene is not None and task is not None
assert not task.property("visible"), "task panel must not open on startup"
QMetaObject.invokeMethod(scene, "taskPanelRequested", Qt.DirectConnection)
app.processEvents()
assert task.property("visible"), "explicit task gesture opens the independent window"
task.setProperty("visible", False)
app.processEvents()
assert scene.property("visible"), "closing tasks leaves pet alive"
QMetaObject.invokeMethod(scene, "hideRequested", Qt.DirectConnection)
app.processEvents()
assert not scene.property("visible"), "explicit hide removes the pet"
assert not task.property("visible"), "hide never opens a task"
scene.setProperty("visible", True)
app.processEvents()
assert not task.property("visible"), "showing pet never opens task window"
print("PASS: explicit task window, close, hide and restore lifecycle (7 checks)")
root.close()
