#!/usr/bin/env python3
"""Offscreen preview lifecycle; does not certify compositor focus/masking."""
import os
from pathlib import Path
from PySide6.QtCore import QObject, QMetaObject, Qt, QUrl
from PySide6.QtGui import QGuiApplication
from PySide6.QtQml import QQmlApplicationEngine
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
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
