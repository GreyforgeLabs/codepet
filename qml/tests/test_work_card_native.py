#!/usr/bin/env python3
"""Native offscreen compact-card clicks and synthetic renders; no pet/provider state."""
import argparse
import json
import os
from pathlib import Path
import tempfile
import time

os.environ["QT_QPA_PLATFORM"] = "offscreen"
os.environ["QT_QUICK_BACKEND"] = "software"
for key in ("DISPLAY", "WAYLAND_DISPLAY", "QT_QPA_PLATFORMTHEME"):
    os.environ.pop(key, None)

from PySide6.QtCore import QPoint, QPointF, QUrl, Qt
from PySide6.QtGui import QGuiApplication
from PySide6.QtQml import QQmlApplicationEngine
from PySide6.QtQuick import QQuickItem
from PySide6.QtTest import QTest

REPO = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--runtime", type=Path, help="Installed runtime root containing qml/shared")
    args = parser.parse_args()
    runtime = args.runtime.resolve() if args.runtime else REPO
    args.output.mkdir(parents=True, exist_ok=True)
    for name, key in (("home", "HOME"), ("cache", "XDG_CACHE_HOME"), ("state", "XDG_STATE_HOME")):
        path = args.output / "fixture" / name
        path.mkdir(parents=True, exist_ok=True, mode=0o700)
        os.environ[key] = str(path.resolve())
    app = QGuiApplication([])
    engine = QQmlApplicationEngine()
    errors = []
    engine.warnings.connect(lambda warnings: errors.extend(w.toString() for w in warnings))
    with tempfile.TemporaryDirectory(prefix="codepet-card-native-") as td:
        entry = Path(td) / "Card.qml"
        entry.write_text('''import QtQuick
import QtQuick.Window
import "''' + (runtime / "qml/shared").as_uri() + '''"
Window {
    id: win; visible: true; width: 296; height: Math.min(card.implicitHeight + 16, limitHeight); color: "#12161d"
    property real limitHeight: 10000
    property var fixture: ({})
    property bool blocked: false
    property int dismissals: 0
    property int reviews: 0
    property int care: 0
    property int updates: 0
    property int selections: 0
    property string requestedClient: ""
    function setSnapshot(value) { fixture=JSON.parse(value) }
    function failSelection() { ctl.error="Choice was not saved"; blocked=false; ctl.selectingClient="" }
    function confirmSelection() {
        var selected=fixture.clients.filter(function(c) { return c.id===requestedClient })[0]
        fixture=Object.assign({},fixture,{provider:selected})
        blocked=false; ctl.selectingClient=""; ctl.error=""
    }
    QtObject {
        id: ctl
        property var pet: ({name:"Fixture",level:2,xp:80,adopted_at_ms:Date.now()-5*86400000})
        property var state: win.fixture
        property bool busy: win.blocked
        property string error: ""
        property string selectingClient: ""
        function selectClient(id) { if (!busy) { win.selections++;win.requestedClient=id;selectingClient=id;win.blocked=true } }
        function dismiss() { win.dismissals++; win.fixture=Object.assign({},win.fixture,{task:null}) }
        function action(method,params) {}
        function fix() {}
    }
    WorkCard {
        id: card; objectName:"card"; x:8; y:8; width:win.width-16; height:Math.min(implicitHeight,win.height-16)
        controller:ctl
        meters: ({hunger:9300,thirst:8700,mental_health:7200,hygiene:8100})
        onReviewRequested: win.reviews++
        onHomeRequested: win.care++
        onUpdatesRequested: win.updates++
    }
}
''')
        engine.load(QUrl.fromLocalFile(str(entry)))
        assert engine.rootObjects(), errors
        win = engine.rootObjects()[0]

        def find(item, name):
            if item.objectName() == name:
                return item
            children = item.childItems() if isinstance(item, QQuickItem) else item.children()
            for child in children:
                result = find(child, name)
                if result is not None:
                    return result
            return None

        def settle():
            app.processEvents()
            QTest.qWait(60)
            app.processEvents()

        def click(name):
            item = find(win, name)
            assert item and item.isVisible(), name
            point = item.mapToScene(QPointF(item.width() / 2, item.height() / 2))
            QTest.mouseClick(win, Qt.LeftButton, Qt.NoModifier, QPoint(round(point.x()), round(point.y())))
            settle()

        clients = [{"id": key, "name": name, "available": index < 3, "detail": "Fixture capability: file edits only; command checks unavailable.",
                    "reason": "" if index < 3 else "Install and sign in to this client. <b>Literal explanation.</b>"}
                   for index, (key, name) in enumerate((("codex", "Codex"), ("claude-code", "Claude Code"),
                                                      ("opencode", "OpenCode"), ("muse", "Muse"),
                                                      ("grok", "Grok"), ("hermes", "Hermes"), ("pi", "pi")))]
        state = {"readiness": {"ready": True, "reason": "Ready for a job", "energy": 100},
                 "provider": clients[0], "clients": clients,
                 "scout": {"enabled": False, "next_due_ms": 0}, "task": None}
        task = {"id": "fixture-old-repair", "kind": "bug", "source_kind": "scout_fix", "project": "/fixture/repo",
                "started_at_ms": int(time.time() * 1000) - 3 * 86400000,
                "error": "Codex could not finish: A file exceeds this job’s size limit. <b>Literal text.</b>",
                "output": "Synthetic saved output. No provider job was run."}
        checks = []
        for name, status in [("idle", None), ("failed-repair", "failed"), ("running", "running"), ("review", "review")]:
            win.setSnapshot(json.dumps(dict(state, task=dict(task, status=status) if status else None)))
            settle()
            card = find(win, "card")
            assert card.height() <= 180, (name, card.height())
            assert "80 XP" in find(win, "workCard.stats").property("text")
            assert "Food 93%" in find(win, "workCard.needs").property("text")
            assert win.grabWindow().save(str(args.output / f"{name}.png"))
            checks.append({"case": name, "height": card.height(), "result": "pass"})
            if status == "failed":
                assert not find(win, "workCard.previousMessage").isVisible()
                win.setProperty("blocked", True)
                click("workCard.dismiss")
                assert win.property("dismissals") == 0
                win.setProperty("blocked", False)
                click("workCard.details")
                assert find(win, "workCard.previousMessage").isVisible()
                assert find(win, "workCard.previousTime").property("text") == "Started 3 days ago"
                assert win.grabWindow().save(str(args.output / "failed-repair-details.png"))
                click("workCard.dismiss")
                assert win.property("dismissals") == 1
                assert not find(win, "workCard.dismiss").isVisible()
            elif status in ("running", "review"):
                assert not find(win, "workCard.dismiss").isVisible()
                assert find(win, "workCard.primary").isVisible()
            if status == "review":
                click("workCard.primary")
                assert win.property("reviews") == 1
        click("workCard.care")
        click("workCard.updates")
        assert win.property("care") == 1 and win.property("updates") == 1
        win.setSnapshot(json.dumps(dict(state, task=dict(task, status="running", client_id="claude-code", client_name="Claude Code"))))
        settle()
        toggle = find(win, "workClient.toggle")
        assert "Codex" in toggle.property("text")
        assert "Claude Code" in find(win, "workClient.origin").property("text")
        click("workClient.toggle")
        assert find(win, "workClient.detail").isVisible()
        assert "command checks unavailable" in find(win, "workClient.detail").property("text")
        assert win.grabWindow().save(str(args.output / "clients-expanded.png"))
        boxes = []
        for client in clients:
            option = find(win, "workClient.option." + client["id"])
            assert option and option.isVisible() and option.width() <= 260
            point = option.mapToScene(QPointF(0, 0))
            boxes.append((point.x(), point.y(), option.width(), option.height()))
        assert all(a[1]+a[3] <= b[1] for a,b in zip(boxes, boxes[1:])), boxes
        click("workClient.option.opencode")
        assert win.property("selections") == 1 and win.property("requestedClient") == "opencode"
        assert "Codex" in toggle.property("text"), "held response must not claim a saved choice"
        click("workClient.option.claude-code")
        assert win.property("selections") == 1, "pending request blocks duplicate choice"
        win.failSelection(); settle()
        assert "Codex" in toggle.property("text")
        assert "not saved" in find(win, "workCard.error").property("text")
        click("workClient.option.opencode"); win.confirmSelection(); settle()
        assert "OpenCode" in toggle.property("text")
        assert "Claude Code" in find(win, "workClient.origin").property("text"), "active job keeps its own client"
        assert not find(win, "workClient.option.codex").isVisible()
        assert toggle.hasActiveFocus(), "confirmed choice returns focus to the visible selector"
        assert not find(win, "workClient.detail").isVisible(), "capability detail stays out of the collapsed strip"
        assert win.grabWindow().save(str(args.output / "active-client-next-choice.png"))
        # Actual pointer focus followed by keyboard activation; no direct chosen() calls.
        click("workClient.toggle")
        QTest.keyClick(win, Qt.Key_Tab); settle()
        assert find(win, "workClient.option.codex").hasActiveFocus()
        QTest.keyClick(win, Qt.Key_Return); settle()
        assert win.property("requestedClient") == "codex"
        win.confirmSelection(); settle()
        toggle.forceActiveFocus(); QTest.keyClick(win, Qt.Key_Space); settle()
        assert find(win, "workClient.option.codex").isVisible()
        click("workClient.option.claude-code"); win.confirmSelection(); settle()
        assert "Claude Code" in toggle.property("text")
        click("workClient.toggle")
        for client in clients[3:]:
            click("workClient.option." + client["id"]); win.confirmSelection(); settle()
            assert client["name"] in toggle.property("text")
            reason = find(win, "workClient.reason")
            assert reason.isVisible() and reason.property("text") == client["reason"]
            click("workClient.toggle")
        QTest.keyClick(win, Qt.Key_Escape); settle()
        assert not find(win, "workClient.option.codex").isVisible()
        win.setVisible(False); settle(); win.setVisible(True); settle()
        assert "pi" in toggle.property("text"), "reopening preserves confirmed choice"
        assert win.grabWindow().save(str(args.output / "client-unavailable.png"))
        click("workClient.toggle")
        win.setWidth(240); win.setProperty("limitHeight", 260); settle()
        scroll = find(win, "workCard.scroll")
        assert scroll.property("contentHeight") > scroll.height()
        toggle.forceActiveFocus()
        for client in clients:
            QTest.keyClick(win, Qt.Key_Tab); settle()
            focused = find(win, "workClient.option." + client["id"])
            assert focused.hasActiveFocus(), client["id"]
            top = focused.mapToItem(scroll, QPointF(0, 0)).y()
            assert top >= -1 and top + focused.height() <= scroll.height() + 1, (client["id"], top, scroll.height())
        assert win.grabWindow().save(str(args.output / "client-narrow-keyboard.png"))
        count = win.property("selections")
        click("workClient.option.pi")
        assert win.property("selections") == count+1, "last client remains pointer-reachable in a short narrow card"
        win.confirmSelection(); settle()
        assert win.grabWindow().save(str(args.output / "client-narrow.png"))
        checks.append({"case":"clients", "result":"pass", "options":len(clients), "pointer_keyboard_pending_error_reopen":True})
        assert not errors, errors
        win.close()
        (args.output / "checks.json").write_text(json.dumps({"cases": checks, "native_clicks": "dismiss/busy/details/review/care/updates passed", "warnings": errors}, indent=2) + "\n")
        print("4 compact renders; native navigation and 7-client pointer/keyboard/pending/error/reopen checks passed")


if __name__ == "__main__":
    main()
