#!/usr/bin/env python3
"""Optional Qt6/PySide6 runner for QML smoke tests (no product dependency).
Run: QT_QPA_PLATFORM=offscreen python qml/tests/run_qml.py qml/tests/PreviewSmoke.qml
Requires an already installed PySide6. Does not install anything or contact a service.
"""
import sys
from pathlib import Path
from PySide6.QtCore import QUrl, QTimer
from PySide6.QtGui import QGuiApplication
from PySide6.QtQml import QQmlApplicationEngine

app = QGuiApplication(sys.argv)
engine = QQmlApplicationEngine()
engine.exit.connect(app.exit)
engine.quit.connect(app.quit)
engine.load(QUrl.fromLocalFile(str(Path(sys.argv[1]).resolve())))
if not engine.rootObjects():
    raise SystemExit(2)
QTimer.singleShot(20000, lambda: app.exit(124))
raise SystemExit(app.exec())
