#!/usr/bin/env python3
"""Optional Qt6/PySide6 runner for QML smoke tests (no product dependency).
Run: python qml/tests/run_qml.py qml/tests/PreviewSmoke.qml   (always offscreen)

- QML messages (console.log and warnings) always go to stderr: Qt otherwise sends them to
  the systemd journal when stderr is not a terminal, and a smoke's PASS/FAIL lines vanish.
- qml/tests/stubs is on the import path: test-only stand-ins for the Omarchy shell's
  qs.Commons (Style, Color), so the bar plugin's popup screens load without Quickshell.
- Smokes may read repository files with XMLHttpRequest (fixtures, the pet registry).
- A smoke that never calls Qt.exit() fails with 124 after the timeout (default 20 s,
  CODEPET_QML_TIMEOUT_S to change it).
Requires an already installed PySide6. Does not install anything or contact a service.
"""
import os
import sys
from pathlib import Path

os.environ.setdefault("QML_XHR_ALLOW_FILE_READ", "1")
# Always offscreen: a smoke must never open a window on the live desktop, whatever the
# caller's QT_QPA_PLATFORM says (a desktop session exports wayland;xcb).
os.environ["QT_QPA_PLATFORM"] = "offscreen"
for var in ("WAYLAND_DISPLAY", "DISPLAY", "QT_QPA_PLATFORMTHEME"):
    os.environ.pop(var, None)

from PySide6.QtCore import QUrl, QTimer, QtMsgType, qInstallMessageHandler  # noqa: E402
from PySide6.QtGui import QGuiApplication  # noqa: E402
from PySide6.QtQml import QQmlApplicationEngine  # noqa: E402

STUBS = Path(__file__).resolve().parent / "stubs"
PREFIX = {QtMsgType.QtDebugMsg: "qml", QtMsgType.QtInfoMsg: "qml", QtMsgType.QtWarningMsg: "warning",
          QtMsgType.QtCriticalMsg: "critical", QtMsgType.QtFatalMsg: "fatal"}


def to_stderr(mode, context, message):
    sys.stderr.write(f"{PREFIX.get(mode, 'qml')}: {message}\n")
    sys.stderr.flush()


qInstallMessageHandler(to_stderr)
app = QGuiApplication(sys.argv)
engine = QQmlApplicationEngine()
engine.addImportPath(str(STUBS))
engine.exit.connect(app.exit)
engine.quit.connect(app.quit)
engine.load(QUrl.fromLocalFile(str(Path(sys.argv[1]).resolve())))
if not engine.rootObjects():
    raise SystemExit(2)
QTimer.singleShot(int(float(os.environ.get("CODEPET_QML_TIMEOUT_S", "20")) * 1000), lambda: app.exit(124))
raise SystemExit(app.exec())
