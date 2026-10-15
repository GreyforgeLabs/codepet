#!/usr/bin/env python3
"""Contract tests for the Omarchy bar plugin `greyforge.codepet` (omarchy-plugin/).

Static source contracts always run. When Quickshell (`qs`) and the Omarchy 4 shell
sources (/usr/share/omarchy/shell) are installed, the real plugin is also loaded in an
isolated, display-less Quickshell (env -i, offscreen Qt, private HOME/XDG dirs, a fake
`codepet` and a fake launcher) and driven over a harness IPC target. That proves QML
loading and the request/file contracts; it is NOT Wayland, compositor or live-bar proof.

Run: python3 qml/tests/test_bar_plugin.py
Optional: CODEPET_PLUGIN_STAGE_DIR=<dir> also stages the plugin there and runs
`omarchy plugin validate` on that copy (never installs it).
"""
from pathlib import Path
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import tomllib
import unittest

REPO = Path(__file__).resolve().parents[2]
PLUGIN = REPO / "omarchy-plugin"
LIFETEXT = REPO / "qml" / "shared" / "LifeText.js"
OMARCHY_SHELL = Path("/usr/share/omarchy/shell")
PLUGIN_ID = "greyforge.codepet"
QML_FILES = sorted(PLUGIN.glob("*.qml"))
JS_FILES = sorted(PLUGIN.glob("*.js"))
TIMERS = {
    "codepetBar.hoverGrace": "BarWidget.qml",
    "codepet.presenceTick": "PresenceRecorder.qml",
    "codepetBar.sleepCountdown": "BarPopup.qml",
    "codepetBar.rpcWatchdog": "BarRpc.qml",
}
LIFE = "life-0123456789abcdef01234567"


def strip_comments(text):
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
    return re.sub(r"(?m)(^|[^:\"'])//[^\n]*", r"\1", text)


def source(name):
    return strip_comments((PLUGIN / name).read_text())


def block_after(text, start):
    """The balanced {...} block that begins at the first '{' at or after `start`."""
    i = text.index("{", start)
    depth = 0
    for j in range(i, len(text)):
        if text[j] == "{":
            depth += 1
        elif text[j] == "}":
            depth -= 1
            if depth == 0:
                return text[i:j + 1]
    raise AssertionError("unbalanced block")


def blocks(text, opener):
    return [block_after(text, m.start()) for m in re.finditer(re.escape(opener), text)]


def function_bodies(text):
    """{name: body} for every `function name(...) {...}` in a QML/JS source."""
    out = {}
    for m in re.finditer(r"\bfunction\s+(\w+)\s*\(", text):
        out.setdefault(m.group(1), "")
        out[m.group(1)] += block_after(text, m.end())
    return out


def stage_plugin(dest):
    """The install layout: a plain copy of omarchy-plugin/ plus the vectors-tested
    qml/shared/LifeText.js copied byte-for-byte (when it exists), no symlinks."""
    dest = Path(dest)
    if dest.exists():
        shutil.rmtree(dest)
    shutil.copytree(PLUGIN, dest, symlinks=True)
    if LIFETEXT.is_file():
        shutil.copyfile(LIFETEXT, dest / "LifeText.js")
    return dest


def qmlformat():
    for cand in ("/usr/lib/qt6/bin/qmlformat", shutil.which("qmlformat") or "", shutil.which("pyside6-qmlformat") or ""):
        if cand and Path(cand).is_file() and os.access(cand, os.X_OK):
            return cand
    return None


class Manifest(unittest.TestCase):
    def setUp(self):
        self.m = json.loads((PLUGIN / "manifest.json").read_text())

    def test_identity_and_kind(self):
        self.assertEqual(self.m["schemaVersion"], 1)
        self.assertEqual(self.m["id"], PLUGIN_ID)
        self.assertEqual(self.m["kinds"], ["bar-widget"])
        self.assertEqual(self.m["entryPoints"], {"barWidget": "BarWidget.qml"})
        self.assertEqual(self.m["license"], "MIT")
        self.assertEqual(self.m["name"], "CodePet")
        self.assertTrue((PLUGIN / self.m["entryPoints"]["barWidget"]).is_file())

    def test_version_matches_cargo(self):
        cargo = tomllib.loads((REPO / "Cargo.toml").read_text())
        self.assertEqual(self.m["version"], cargo["workspace"]["package"]["version"])
        self.assertEqual(self.m["version"], "0.2.0")
        self.assertIn("## 0.2.0", (PLUGIN / "CHANGELOG.md").read_text())

    def test_bar_widget_defaults(self):
        bw = self.m["barWidget"]
        self.assertEqual(bw["defaultSection"], "right")
        self.assertEqual(bw["defaults"], {"label": "name", "presence": True, "overlayOnLoad": False,
                                          "binary": "", "stateHome": ""})
        self.assertFalse(bw.get("allowMultiple", False))
        for entry in bw.get("schema", []):
            self.assertIn(entry["key"], bw["defaults"])
            self.assertEqual(entry["defaultValue"], bw["defaults"][entry["key"]])

    def test_description_is_honest(self):
        self.assertIn("Mock tasks only", self.m["description"])

    def test_required_files_and_no_symlinks(self):
        for name in ("manifest.json", "BarWidget.qml", "BarPopup.qml", "BarRpc.qml", "PresenceRecorder.qml",
                     "CodePetBar.js", "PresenceModel.js", "README.md", "CHANGELOG.md", "LICENSE"):
            self.assertTrue((PLUGIN / name).is_file(), name)
        links = [p for p in PLUGIN.rglob("*") if p.is_symlink()]
        self.assertEqual(links, [])
        self.assertFalse((PLUGIN / "LifeText.js").exists(), "LifeText.js is staged, never checked in here")
        self.assertEqual((PLUGIN / "LICENSE").read_text(), (REPO / "LICENSE").read_text())


class SourceContract(unittest.TestCase):
    def setUp(self):
        self.src = {p.name: strip_comments(p.read_text()) for p in QML_FILES + JS_FILES}
        self.qml = {k: v for k, v in self.src.items() if k.endswith(".qml")}
        self.all_qml = "\n".join(self.qml.values())

    def test_exactly_the_four_named_timers(self):
        self.assertEqual(self.all_qml.count("Timer {"), 4)
        for name, file in TIMERS.items():
            self.assertEqual(self.all_qml.count(f'objectName: "{name}"'), 1, name)
            self.assertIn(f'objectName: "{name}"', self.qml[file], name)
        rec = self.qml["PresenceRecorder.qml"]
        tick = blocks(rec, "Timer {")[0]
        self.assertIn("interval: 60000", tick)
        self.assertIn("repeat: true", tick)
        self.assertIn("running: recorder.eligible", tick)
        grace = blocks(self.qml["BarWidget.qml"], "Timer {")[0]
        self.assertIn("interval: 250", grace)
        self.assertIn("repeat: false", grace)
        dog = blocks(self.qml["BarRpc.qml"], "Timer {")[0]
        self.assertIn("interval: rpc.responseTimeoutMs", dog)
        self.assertIn("repeat: false", dog)
        self.assertIn("responseTimeoutMs: 15000", self.qml["BarRpc.qml"])
        countdown = blocks(self.qml["BarPopup.qml"], "Timer {")[0]
        self.assertIn("interval: 1000", countdown)
        self.assertIn("running: card.open && card.sleeping", countdown)

    def test_no_timer_handler_sends_a_request_or_launches(self):
        for name, text in self.qml.items():
            funcs = function_bodies(text)
            for timer in blocks(text, "Timer {"):
                for bad in ("request(", "launch(", "execDetached", "startup(", "openPopup(", "refresh(", "perform(",
                            "care(", "sleepOrWake(", "togglePause(", "ackSeen(", "noteUnseen("):
                    self.assertNotIn(bad, timer, f"{name}: Timer handler must not call {bad}")
                # Functions a timer calls must not reach the transport either.
                for called in re.findall(r"(?:recorder|card|root)\.(\w+)\(", timer):
                    body = funcs.get(called, "")
                    self.assertNotIn("request(", body, f"{name}: timer -> {called} sends a request")
                    self.assertNotIn("execDetached", body, f"{name}: timer -> {called} launches")

    def test_state_get_only_on_startup_popup_open_and_refresh(self):
        allowed = {"startup", "openPopup", "refresh"}
        for name, text in self.qml.items():
            if name == "BarRpc.qml":
                continue
            count = text.count('"state.get"')
            in_allowed = sum(body.count('"state.get"') for fn, body in function_bodies(text).items() if fn in allowed)
            self.assertEqual(count, in_allowed, f"{name}: state.get outside startup/openPopup/refresh")
        bw = self.qml["BarWidget.qml"]
        funcs = function_bodies(bw)
        self.assertIn("Component.onCompleted: Qt.callLater(root.startup)", bw)
        self.assertIn("!root.leader", funcs["startup"])
        self.assertIn("root.startupDone = true", funcs["startup"])
        self.assertIn("root.openPopup()", bw)
        self.assertIn("card.widget.refresh()", self.qml["BarPopup.qml"])

    def test_file_events_never_launch_rust(self):
        for name, text in self.qml.items():
            for handler in re.findall(r"on(?:FileChanged|Loaded|LoadFailed|Saved|SaveFailed)\s*:\s*([^\n]*)", text):
                self.assertNotIn("request(", handler, name)
                self.assertNotIn("execDetached", handler, name)
                self.assertNotIn("launch(", handler, name)
        self.assertIn("onFileChanged: reload()", self.qml["BarWidget.qml"])

    def test_request_methods_are_allowlisted_and_never_task(self):
        model = self.src["CodePetBar.js"]
        allow = re.search(r"var ALLOWED_METHODS = \[([^\]]*)\]", model).group(1)
        self.assertEqual(json.loads("[" + allow + "]"),
                         ["state.get", "pet.care", "pet.sleep", "pet.wake", "pet.pause", "pet.events_seen"])
        for name, text in self.qml.items():
            for method in re.findall(r"request\(\s*\"([^\"]+)\"", text):
                self.assertIn(method, json.loads("[" + allow + "]"), f"{name} sends {method}")
            self.assertNotRegex(text, r"\"task\.", f"{name} mentions a task method")
        rpc = self.qml["BarRpc.qml"]
        req = function_bodies(rpc)["request"]
        self.assertLess(req.index("Model.allowedMethod(method)"), req.index("createObject"),
                        "allowlist is checked before any process exists")
        self.assertIn("rpc.busy", req)
        self.assertIn("maxFrameBytes", req)
        self.assertIn("readonly property int maxFrameBytes: 65536", rpc)

    def test_processes_are_argv_only(self):
        rpc = self.qml["BarRpc.qml"]
        self.assertEqual(self.all_qml.count("Process {"), 1)
        self.assertIn('command: [rpc.binary, "rpc"]', rpc)
        self.assertIn("environment: rpc.childEnv", rpc)
        for name, text in self.qml.items():
            self.assertNotIn("bar.run(", text, name)
            self.assertNotIn(".run(", text.replace("running", ""), name)
            self.assertNotRegex(text, r"\"(?:sh|bash)\"\s*,\s*\"-l?c\"", name)
            self.assertNotIn("startDetached", text, name)
            for call in re.findall(r"execDetached\(([^\n]*)\)", text):
                self.assertEqual(call, "argv", f"{name}: execDetached must take the argv array: {call}")
        launch = function_bodies(self.qml["BarWidget.qml"])["launch"]
        self.assertIn("var argv = [root.paths.launcher, verb]", launch)
        self.assertIn("root.launcherVerbs.indexOf(verb) < 0", launch)
        self.assertIn('argv = ["/usr/bin/env", "XDG_STATE_HOME=" + root.paths.childEnv.XDG_STATE_HOME].concat(argv)', launch)

    def test_mouse_buttons(self):
        bw = self.qml["BarWidget.qml"]
        self.assertIn('if (mouse.button === Qt.RightButton) root.launch("toggle")', bw)
        self.assertIn('else if (mouse.button === Qt.MiddleButton) root.launch("tasks")', bw)
        self.assertIn("else root.openPopup()", bw)

    def test_state_home_reaches_files_rpc_and_launcher(self):
        bw = self.qml["BarWidget.qml"]
        self.assertIn("readonly property var paths: Model.paths(cfg, env)", bw)
        self.assertIn("path: root.paths.barState", bw)
        self.assertIn("stateDir: root.paths.stateDir", bw)
        self.assertIn("childEnv: root.paths.childEnv", bw)
        self.assertIn("root.paths.childEnv.XDG_STATE_HOME", function_bodies(bw)["launch"])
        self.assertIn('XDG_STATE_HOME: Quickshell.env("XDG_STATE_HOME")', bw)
        self.assertIn("card.widget.paths.testState", self.qml["BarPopup.qml"])
        self.assertIn('"Test state: "', self.qml["BarPopup.qml"])

    def test_nothing_is_written_inside_the_plugin_dir(self):
        set_text = re.findall(r"(\w+)\.setText\(", self.all_qml)
        self.assertEqual(set_text, ["presenceFile"], "presence.json is the only file the plugin writes")
        rec = self.qml["PresenceRecorder.qml"]
        writer = block_after(rec, rec.index("id: presenceFile") - 40)
        self.assertIn('path: recorder.stateDir !== "" ? recorder.stateDir + "/presence.json" : ""', writer)
        self.assertIn("atomicWrites: true", writer)
        self.assertIn("printErrors: false", writer)
        for name, text in self.src.items():
            for bad in ("resolvedUrl", "__sourceDir", "Qt.resolvedUrl", "writeAdapter", "setData("):
                self.assertNotIn(bad, text, f"{name} uses {bad}")
        # Every FileView path comes from the computed paths (outside the plugin folder).
        for name, text in self.qml.items():
            for fv in blocks(text, "FileView {"):
                path = re.search(r"path:\s*([^\n]+)", fv).group(1)
                self.assertRegex(path, r"^(root\.paths\.|card\.widget\.paths\.|recorder\.stateDir)", f"{name}: {path}")
        # Settings are written only through the shell facade.
        self.assertIn("bar.shell.updateEntryInline(root.moduleName, next)", self.qml["BarWidget.qml"])
        self.assertNotIn("shellConfigFile.setText", self.all_qml)

    def test_presence_recorder_rules(self):
        rec = self.qml["PresenceRecorder.qml"]
        self.assertIn("timeout: 180", rec)
        self.assertIn("respectInhibitors: false", rec)
        self.assertIn("Hyprland.focusedWorkspace.hasFullscreen", rec)
        self.assertIn("presence.wanted === true", rec)
        self.assertIn("Presence.newSession(now, Math.random)", rec)
        self.assertIn("onIdleChanged: if (recorder.idle) Presence.stop(recorder.model)", rec)
        self.assertIn("if (!recorder.eligible) Presence.stop(recorder.model)", rec)
        self.assertNotIn("WlSessionLock", rec)
        self.assertNotIn("sleeping", rec)
        bw = self.qml["BarWidget.qml"]
        self.assertIn("active: root.leader", bw)
        self.assertIn("items.length > 0 && items[0] === root", bw)
        # Writes start only after bar-state.json proved the state directory exists.
        self.assertIn("root.stateDirReady = true", bw)
        self.assertIn("stateDirReady && stateDir !== \"\"", rec)

    def test_popup_follows_the_cris_pattern_and_shows_test_state(self):
        bw = self.qml["BarWidget.qml"]
        loader = block_after(bw, bw.index("id: popupLoader") - 30)
        self.assertIn("active: root.menu", loader)
        self.assertIn("onLoaded: item.open = true", loader)
        self.assertIn("BarPopup {", loader)
        pop = self.qml["BarPopup.qml"]
        self.assertTrue(pop.lstrip().split("\n", 5)[-1].lstrip().startswith("PopupCard {") or "PopupCard {" in pop)
        for label in ('"Show/hide pet"', '"Tasks\u2026"', '"Companion & looks\u2026"', '"Refresh"', "Model.lifeButtonLabel"):
            self.assertIn(label, pop)
        self.assertIn('["food", "water", "play", "wash", "bathroom", "routine"]', pop)
        self.assertIn("enabledAction: !card.busy", pop)

    def test_text_is_plain_and_accessible(self):
        pop = self.qml["BarPopup.qml"]
        self.assertIn("textFormat: Text.PlainText", pop)
        self.assertIn("Accessible.role: Accessible.Button", pop)
        self.assertIn("Accessible.name: root.summary.accessibleName", self.qml["BarWidget.qml"])
        self.assertNotIn("Text.RichText", self.all_qml)
        self.assertNotIn("Text.StyledText", self.all_qml)

    def test_every_text_element_is_plain_text(self):
        # A Text without textFormat is AutoText, which renders a pet name such as
        # "<b>Pip</b>" as rich text. Every Text (and every component based on it) must
        # opt out explicitly, including the bar cell's Label.
        found = 0
        for name, text in self.qml.items():
            for m in re.finditer(r"(?:component\s+\w+\s*:\s*)?\bText\s*\{", text):
                block = block_after(text, m.start())
                found += 1
                self.assertIn("textFormat: Text.PlainText", block, f"{name}: {block[:120]}")
        self.assertGreaterEqual(found, 4)
        bw = self.qml["BarWidget.qml"]
        label = block_after(bw, bw.index("component Label: Text"))
        self.assertIn("textFormat: Text.PlainText", label)

    def test_js_libraries_are_pure(self):
        for js in JS_FILES:
            text = js.read_text()
            self.assertTrue(text.startswith(".pragma library\n"), js.name)
            self.assertNotRegex(text, r"(?m)^\s*\.?import\b", js.name)
            for bad in ("Quickshell", "Qt.", "XMLHttpRequest", "require("):
                self.assertNotIn(bad, strip_comments(text), f"{js.name} uses {bad}")

    def test_copy_avoids_banned_words(self):
        banned = re.compile(r"kill|starv|your fault|abandon|neglected you|guilt|punish|streak|hurry|!!!", re.I)
        for p in QML_FILES + JS_FILES:
            for lit in re.findall(r'"((?:[^"\\\n]|\\.)*)"', strip_comments(p.read_text())):
                self.assertIsNone(banned.search(lit), f"{p.name}: {lit}")

    @unittest.skipUnless(qmlformat(), "qmlformat unavailable; QML parse gate untested")
    def test_qmlformat_parses_every_qml_file(self):
        tool = qmlformat()
        for p in QML_FILES:
            r = subprocess.run([tool, str(p)], capture_output=True, text=True)
            self.assertEqual(r.returncode, 0, f"{p.name}: {r.stderr}")


class Staging(unittest.TestCase):
    def test_staged_copy_is_plain_and_lifetext_is_identical(self):
        with tempfile.TemporaryDirectory() as tmp:
            dest = stage_plugin(Path(tmp) / PLUGIN_ID)
            self.assertEqual([p for p in dest.rglob("*") if p.is_symlink()], [])
            for p in PLUGIN.iterdir():
                self.assertEqual((dest / p.name).read_bytes(), p.read_bytes(), p.name)
            if LIFETEXT.is_file():
                self.assertEqual((dest / "LifeText.js").read_bytes(), LIFETEXT.read_bytes())
                text = LIFETEXT.read_text()
                self.assertTrue(text.startswith(".pragma library"), "LifeText.js must stay a .pragma library")
                self.assertNotRegex(text, r"(?m)^\s*\.?import\b", "LifeText.js must have no imports")
            else:
                self.skipTest("qml/shared/LifeText.js does not exist yet (WP-7); byte identity untested")

    @unittest.skipUnless(os.environ.get("CODEPET_PLUGIN_STAGE_DIR") and shutil.which("omarchy"),
                         "set CODEPET_PLUGIN_STAGE_DIR (and have omarchy) to validate a staged copy")
    def test_omarchy_plugin_validate_on_a_staged_copy(self):
        dest = stage_plugin(Path(os.environ["CODEPET_PLUGIN_STAGE_DIR"]) / PLUGIN_ID)
        r = subprocess.run(["omarchy", "plugin", "validate", str(dest)], capture_output=True, text=True, timeout=60)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)


# --------------------------------------------------------------------------- live load

FAKE_CODEPET = r'''#!{python} -I
# Test-only stand-in for `codepet rpc`: logs the request and the state home it saw,
# exports bar-state.json for state.get (as the real CLI does) and answers ok.
import json, os, sys
home = os.environ["HOME"]
state_home = os.environ.get("XDG_STATE_HOME", "")
req = json.loads(sys.stdin.readline())
with open(os.path.join(home, "rpc.log"), "a") as log:
    log.write(json.dumps({{"method": req["method"], "id": req["id"], "params": req["params"],
                          "argv": sys.argv[1:], "state_home": state_home}}) + "\n")
result = {{"changed": True}}
if req["method"] == "state.get":
    sdir = os.path.join(state_home or os.path.join(home, ".local/state"), "codepet")
    os.makedirs(sdir, exist_ok=True)
    path = os.path.join(sdir, "bar-state.json")
    try:
        doc = json.load(open(path))
    except Exception:
        doc = None
    if doc:
        doc["state_version"] += 1
        tmp = path + ".tmp"
        with open(tmp, "w") as f:
            json.dump(doc, f)
        os.rename(tmp, path)
        result = {{"life": doc.get("life"), "unseen_events": [dict(e, kind="evolved") for e in doc.get("unseen_events", [])]}}
    else:
        result = {{"life": None, "unseen_events": []}}
print(json.dumps({{"v": 1, "id": req["id"], "ok": True, "result": result}}), flush=True)
'''

FAKE_LAUNCHER = r'''#!{python} -I
import json, os, sys
with open(os.path.join(os.environ["HOME"], "launch.log"), "a") as log:
    log.write(json.dumps({{"argv": sys.argv[1:], "state_home": os.environ.get("XDG_STATE_HOME", "")}}) + "\n")
'''

HARNESS = r'''import QtQuick
import Quickshell
import Quickshell.Io
import "plugin" as P

// Test-only host: a fake bar facade around two real plugin instances (two monitors).
ShellRoot {
    id: shell
    property var tips: []
    property var settingsWrites: []
    QtObject {
        id: fakeShell
        function updateEntryInline(id, entry) { shell.settingsWrites = shell.settingsWrites.concat([{ id: id, entry: entry }]); return true }
    }
    QtObject {
        id: fakeBar
        property color barForeground: "white"
        property color foreground: "white"
        property string fontFamily: "sans"
        property bool vertical: false
        property int barSize: 26
        property string position: "top"
        property var activePopout: null
        property var shell: fakeShell
        function moduleWidgets(id) { return id === "greyforge.codepet" ? [w1, w2] : [] }
        function showTooltip(t, s) { shell.tips = shell.tips.concat([String(s)]) }
        function hideTooltip(t) {}
        function requestPopout(o) { activePopout = o }
        function releasePopout(o) { if (activePopout === o) activePopout = null }
        function registerClickTarget(t) {}
        function unregisterClickTarget(t) {}
    }
    FloatingWindow {
        implicitWidth: 400
        implicitHeight: 60
        Row {
            P.BarWidget { id: w1; bar: fakeBar }
            P.BarWidget { id: w2; bar: fakeBar }
        }
    }
    function walk(o, out, seen, depth) {
        if (!o || depth > 14 || seen.indexOf(o) >= 0) return
        seen.push(o)
        if (o.objectName && String(o.objectName).indexOf("codepet") === 0 && o.running !== undefined)
            out[o.objectName] = o.running
        var lists = []
        try { if (o.data) lists.push(o.data) } catch (e) {}
        try { if (o.children) lists.push(o.children) } catch (e) {}
        try { if (o.resources) lists.push(o.resources) } catch (e) {}
        try { if (o.item) lists.push([o.item]) } catch (e) {}
        try { if (o.contentItem) lists.push([o.contentItem]) } catch (e) {}
        for (var l = 0; l < lists.length; l++)
            for (var i = 0; i < lists[l].length; i++) shell.walk(lists[l][i], out, seen, depth + 1)
    }
    function info(w) {
        var timers = {}
        shell.walk(w, timers, [], 0)
        return { leader: w.leader, summary: w.summary, version: w.barState ? w.barState.state_version : -1,
                 menu: w.menu, busy: w.rpcBusy, notInstalled: w.rpcNotInstalled, feedback: w.feedback,
                 grewText: w.grewText, eventsAcked: w.eventsAcked, testState: w.paths.testState,
                 stateDir: w.paths.stateDir, stateMissing: w.stateMissing, label: w.cfg.label, timers: timers }
    }
    IpcHandler {
        target: "harness"
        function dump(): string { return JSON.stringify({ w1: shell.info(w1), w2: shell.info(w2), tips: shell.tips, settingsWrites: shell.settingsWrites }) }
        function open(): void { w1.openPopup() }
        function close(): void { w1.close() }
        function care(action: string): void { w1.care(action) }
        function refresh(): void { w1.refresh() }
        function launch(verb: string): void { w1.launch(verb) }
        function label(mode: string): void { w1.setLabelMode(mode) }
        function hover(on: bool): void { w1.hovered = on; w1.children.length; shell.hoverRestart(w1) }
        function refuse(): string {
            var got = ""
            var rpc = shell.findRpc(w1)
            rpc.request("task.start", {}, function (resp) { got = JSON.stringify(resp) })
            return got
        }
    }
    function findRpc(w) {
        for (var i = 0; i < w.data.length; i++) if (w.data[i].objectName === "codepetBar.rpc") return w.data[i]
        return null
    }
    function hoverRestart(w) {
        for (var i = 0; i < w.data.length; i++) if (w.data[i].objectName === "codepetBar.hoverGrace") w.data[i].restart()
    }
}
'''


def bar_state(version=5, **over):
    doc = {
        "schema": "codepet.bar.v1", "state_version": version, "generated_at_ms": int(time.time() * 1000),
        "app_version": "0.2.0", "ruleset": "lifecycle-v1",
        "life": {"life_id": LIFE, "ordinal": 1, "name": "Ginger", "appearance": "cat", "stage": 2, "level": 2,
                 "xp": 120, "next_stage_xp": 400, "status": "healthy", "mode": "permadeath", "armed": True,
                 "pace": "steady", "paused": False, "fainted": False, "death_pending": False},
        "meters": {"hunger": 6100, "thirst": 8000, "mental_health": 8000, "hygiene": 9000},
        "needs": [{"need": "thirst", "action": "water", "state": "ok", "until_critical_ms": 30000000, "until_end_ms": 51600000}],
        "critical": None,
        "care": {"sleeping": False, "sleep_remaining_ms": 120000, "sleep_started_at_ms": None, "sleep_due": False,
                 "bathroom_due": False, "ready_to_work": True, "readiness_reasons": []},
        "task": None,
        "presence": {"wanted": True, "life_id": LIFE, "tick_ms": 60000, "idle_timeout_s": 180},
        "unseen_events": [{"seq": 88, "stage": 2}],
        "memorial": {"count": 0, "latest_name": None},
    }
    doc.update(over)
    return doc


LOAD_ERRORS = re.compile(r"Failed to load configuration|is not a type|is not installed|module \S+ not found"
                         r"|Type \S+ unavailable|ReferenceError|TypeError|Cannot assign|Unable to assign"
                         r"|Binding loop|non-existent property")


@unittest.skipUnless(shutil.which("qs") and (OMARCHY_SHELL / "Ui" / "BarWidget.qml").is_file()
                     and (OMARCHY_SHELL / "Commons" / "qmldir").is_file(),
                     "Quickshell or the Omarchy 4 shell sources unavailable; live plugin load untested")
class QuickshellPluginLoad(unittest.TestCase):
    """Loads the real plugin under a display-less, isolated Quickshell."""

    def start(self, installed=True, shell_entry=None, state=None, state_home=None):
        self.tmp = tempfile.TemporaryDirectory()
        root = Path(self.tmp.name)
        self.root = root
        home = root / "home"
        self.home = home
        for d in (home, root / "run", home / ".local/bin", home / ".config/omarchy"):
            d.mkdir(mode=0o700, parents=True, exist_ok=True)
        stage = root / "stage"
        stage.mkdir()
        shutil.copytree(OMARCHY_SHELL / "Ui", stage / "Ui")
        shutil.copytree(OMARCHY_SHELL / "Commons", stage / "Commons")
        stage_plugin(stage / "plugin")
        (stage / "shell.qml").write_text(HARNESS)
        self.entry = stage / "shell.qml"
        if installed:
            for name, body in (("codepet", FAKE_CODEPET), ("codepet-overlay", FAKE_LAUNCHER)):
                f = home / ".local/bin" / name
                f.write_text(body.format(python=sys.executable))
                f.chmod(0o700)
        if shell_entry is not None:
            (home / ".config/omarchy/shell.json").write_text(json.dumps(
                {"bar": {"layout": {"left": [], "center": [], "right": [dict(shell_entry, id="greyforge.codepet")]}}}))
        sdir = Path(state_home or home / ".local/state") / "codepet"
        self.state_dir = sdir
        if state is not None:
            sdir.mkdir(parents=True, exist_ok=True)
            (sdir / "bar-state.json").write_text(json.dumps(state))
        self.env = {
            "PATH": "/usr/bin:/bin", "HOME": str(home), "XDG_RUNTIME_DIR": str(root / "run"),
            "XDG_CONFIG_HOME": str(home / ".config"), "XDG_CACHE_HOME": str(home / ".cache"),
            "XDG_DATA_HOME": str(home / ".local/share"), "XDG_STATE_HOME": str(home / ".local/state"),
            "QT_QPA_PLATFORM": "offscreen",
        }
        self.log = (root / "qs.log").open("w+")
        self.proc = subprocess.Popen(["qs", "-p", str(self.entry)], env=self.env, stdout=self.log,
                                     stderr=subprocess.STDOUT)
        self.addCleanup(self.stop)
        self.wait_for(lambda: "Configuration Loaded" in self.output() or self.proc.poll() is not None, 20)
        self.assertIn("Configuration Loaded", self.output(), self.output())

    def stop(self):
        self.proc.terminate()
        try:
            self.proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait()
        out = self.output()
        self.log.close()
        self.tmp.cleanup()
        self.assertIsNone(LOAD_ERRORS.search(out), out)

    def output(self):
        self.log.flush()
        self.log.seek(0)
        return self.log.read()

    def wait_for(self, pred, timeout=10):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if pred():
                return True
            time.sleep(0.1)
        return pred()

    def call(self, *args):
        r = subprocess.run(["qs", "ipc", "-p", str(self.entry), "call", "harness", *args], env=self.env,
                           capture_output=True, text=True, timeout=10)
        self.assertEqual(r.returncode, 0, (args, r.stdout, r.stderr, self.output()))
        return r.stdout.strip()

    def dump(self):
        return json.loads(self.call("dump"))

    def rpc_log(self):
        p = self.home / "rpc.log"
        return [json.loads(line) for line in p.read_text().splitlines()] if p.exists() else []

    def launch_log(self):
        p = self.home / "launch.log"
        return [json.loads(line) for line in p.read_text().splitlines()] if p.exists() else []

    def test_leader_startup_popup_care_and_refusals(self):
        self.start(state=bar_state())
        self.assertTrue(self.wait_for(lambda: len(self.rpc_log()) >= 1))
        time.sleep(1.5)
        log = self.rpc_log()
        self.assertEqual([e["method"] for e in log], ["state.get"], "only the leader reads once at startup")
        self.assertEqual(log[0]["argv"], ["rpc"])
        d = self.dump()
        self.assertTrue(d["w1"]["leader"])
        self.assertFalse(d["w2"]["leader"])
        self.assertTrue(self.wait_for(lambda: self.dump()["w1"]["version"] == 6), "re-read after the response")
        self.assertTrue(self.wait_for(lambda: self.dump()["w2"]["version"] == 6), "every screen re-reads")
        d = self.dump()
        self.assertEqual(d["w1"]["summary"]["status"], "evolved")
        self.assertEqual(d["w1"]["summary"]["glyph"], "★")
        self.assertEqual(d["w1"]["summary"]["label"], "Ginger")
        # Timers: the recorder exists on the leader only and ticks while eligible.
        self.assertEqual(d["w1"]["timers"].get("codepet.presenceTick"), True, d["w1"]["timers"])
        self.assertNotIn("codepet.presenceTick", d["w2"]["timers"])
        self.assertEqual(d["w1"]["timers"].get("codepetBar.rpcWatchdog"), False)
        self.assertEqual(d["w1"]["timers"].get("codepetBar.hoverGrace"), False)
        self.assertNotIn("codepetBar.sleepCountdown", d["w1"]["timers"], "popup timer exists only while open")
        self.assertFalse((self.state_dir / "presence.json").exists(), "nothing is credited before the look-back")

        # A file event (an external CLI export) updates the cell without any request.
        doc = bar_state(version=50, unseen_events=[])
        doc["life"]["status"] = "warning"
        doc["needs"] = [{"need": "thirst", "action": "water", "state": "low", "until_critical_ms": 9600000}]
        tmp = self.state_dir / "bar-state.json.tmp"
        tmp.write_text(json.dumps(doc))
        os.rename(tmp, self.state_dir / "bar-state.json")
        self.assertTrue(self.wait_for(lambda: self.dump()["w2"]["version"] == 50, 10))
        time.sleep(1)
        self.assertEqual(len(self.rpc_log()), 1, "file events never launch Rust")
        self.assertEqual(self.dump()["w2"]["summary"]["glyph"], "!")

        # Popup open: one state.get, then the unseen growth is shown and acknowledged once.
        doc["unseen_events"] = [{"seq": 91, "stage": 2}]
        (self.state_dir / "bar-state.json").write_text(json.dumps(doc))
        self.call("open")
        self.assertTrue(self.wait_for(lambda: len(self.rpc_log()) >= 3))
        time.sleep(0.5)
        log = self.rpc_log()
        self.assertEqual([e["method"] for e in log[1:]], ["state.get", "pet.events_seen"])
        self.assertEqual(log[2]["params"], {"life_id": LIFE, "through_seq": 91})
        d = self.dump()
        self.assertTrue(d["w1"]["menu"])
        self.assertEqual(d["w1"]["grewText"], "★ Ginger grew into stage 2!")
        self.assertTrue(d["w1"]["eventsAcked"])
        self.assertEqual(d["w1"]["timers"].get("codepetBar.sleepCountdown"), False, "countdown only while sleeping")

        # Care: one request with a protocol-valid key; success shown after the response.
        self.call("care", "water")
        self.assertTrue(self.wait_for(lambda: len(self.rpc_log()) >= 4))
        care = self.rpc_log()[3]
        self.assertEqual((care["method"], care["params"]), ("pet.care", {"action": "water"}))
        self.assertRegex(care["id"], r"^bar-[a-z0-9]+-\d+$")
        self.assertTrue(self.wait_for(lambda: self.dump()["w1"]["feedback"] == "Care confirmed."))

        # Refusals happen client-side, before any process exists.
        out = self.call("refuse")
        self.assertIn("CLIENT_REFUSED", out)
        time.sleep(0.5)
        self.assertFalse(any(e["method"].startswith("task.") for e in self.rpc_log()))

        # Launcher verbs run the launcher as a detached argv.
        self.call("launch", "toggle")
        self.call("launch", "rm -rf")
        self.assertTrue(self.wait_for(lambda: len(self.launch_log()) >= 1))
        time.sleep(0.5)
        self.assertEqual([e["argv"] for e in self.launch_log()], [["toggle"]])
        self.assertEqual(self.launch_log()[0]["state_home"], str(self.home / ".local/state"))

        # Settings go through the shell facade only.
        self.call("label", "status")
        writes = self.dump()["settingsWrites"]
        self.assertEqual(writes[-1]["id"], "greyforge.codepet")
        self.assertEqual(writes[-1]["entry"]["label"], "status")

        # Tooltip after the hover grace.
        self.call("close")
        self.call("hover", "true")
        self.assertTrue(self.wait_for(lambda: len(self.dump()["tips"]) >= 1, 5))
        self.assertIn("Ginger", self.dump()["tips"][-1])

    def test_state_home_setting_reaches_rpc_launcher_and_files(self):
        qa = self.make_tmp_state_home()
        self.start(shell_entry={"stateHome": str(qa), "label": "status"}, state=bar_state(version=7), state_home=qa)
        self.assertTrue(self.wait_for(lambda: len(self.rpc_log()) >= 1))
        self.assertEqual(self.rpc_log()[0]["state_home"], str(qa))
        self.assertTrue(self.wait_for(lambda: self.dump()["w1"]["version"] == 8))
        d = self.dump()
        self.assertTrue(d["w1"]["testState"])
        self.assertEqual(d["w1"]["stateDir"], str(qa / "codepet"))
        self.assertEqual(d["w1"]["label"], "status")
        # (`qs ipc call` would read "show" as its own subcommand, so another verb is used.)
        self.call("launch", "companion")
        self.assertTrue(self.wait_for(lambda: len(self.launch_log()) >= 1))
        self.assertEqual(self.launch_log()[0], {"argv": ["companion"], "state_home": str(qa)})
        self.assertFalse((self.home / ".local/state/codepet").exists(), "the default state home is untouched")

    def make_tmp_state_home(self):
        holder = tempfile.TemporaryDirectory()
        self.addCleanup(holder.cleanup)
        return Path(holder.name) / "qa-state"

    def test_not_installed_and_no_state(self):
        self.start(installed=False)
        self.assertTrue(self.wait_for(lambda: self.dump()["w1"]["notInstalled"], 10), self.output())
        d = self.dump()
        self.assertFalse(d["w1"]["busy"])
        self.assertTrue(d["w1"]["stateMissing"])
        self.assertEqual(d["w1"]["summary"]["status"], "missing")
        self.assertEqual(d["w1"]["summary"]["label"], "CodePet")
        self.assertNotIn("codepet.presenceTick", {k for k, v in d["w1"]["timers"].items() if v})


if __name__ == "__main__":
    unittest.main(verbosity=2)
