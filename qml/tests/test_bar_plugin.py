#!/usr/bin/env python3
"""Contract tests for the Omarchy bar plugin `greyforge.codepet` (omarchy-plugin/), v0.3.

Static source contracts always run. When Quickshell (`qs`) and the Omarchy 4 shell sources
(/usr/share/omarchy/shell) are installed, the real plugin is also loaded in an isolated,
display-less Quickshell (env -i, offscreen Qt, private HOME/XDG dirs, a fake `codepet` and a
fake launcher) and driven over a harness IPC target. The popup there runs on the real Omarchy
Style and Color; only qs.Ui KeyboardPanel is swapped for a plain window (offscreen Quickshell
has no layer-shell backend). That proves QML loading, the request/file contracts and the
K3/K6/K9/K13 checks under the real theme tokens at font base sizes 12, 16 and 20; it is NOT
Wayland, compositor, keyboard-focus or live-bar proof.

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
TESTS = REPO / "qml" / "tests"
LIFETEXT = REPO / "qml" / "shared" / "LifeText.js"
CONTRACTS = REPO / "docs" / "v0.3" / "contracts"
OMARCHY_SHELL = Path("/usr/share/omarchy/shell")
PLUGIN_ID = "greyforge.codepet"
QML_FILES = sorted(PLUGIN.glob("*.qml"))
JS_FILES = sorted(PLUGIN.glob("*.js"))
# Every Timer in the plugin: name -> (file, repeat). Nothing else may tick (SPEC-v0.3 §8).
TIMERS = {
    "codepetBar.faceWake": ("BarWidget.qml", False),
    "codepet.presenceTick": ("PresenceRecorder.qml", True),
    "codepetBar.rpcWatchdog": ("BarRpc.qml", False),
    "codepetPopup.frames": ("PopupApp.qml", True),
    "codepetPopup.bubble": ("PopupApp.qml", False),
}
# Files that run only inside Quickshell; every other QML file imports QtQuick (and
# qs.Commons / QtQuick.Effects) only, so the PySide smokes load it.
SHELL_FILES = {"BarWidget.qml", "BarRpc.qml", "BarPopup.qml", "PresenceRecorder.qml"}
SMOKES = ["HomeSmoke.qml", "FirstRunSmoke.qml", "HelpersSmoke.qml", "MoreSmoke.qml", "HelpSmoke.qml", "OffSmoke.qml"]
LIFE = "life-0123456789abcdef01234567"


def strip_comments(text):
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
    return re.sub(r"(?m)(^|[^:\"'])//[^\n]*", r"\1", text)


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
        self.assertIn(f"## {self.m['version']}", (PLUGIN / "CHANGELOG.md").read_text())

    def test_bar_widget_defaults(self):
        bw = self.m["barWidget"]
        self.assertEqual(bw["defaultSection"], "right")
        self.assertEqual(bw["defaults"], {"label": "name", "presence": True, "overlayOnLoad": False, "calm": False,
                                          "binary": "", "stateHome": ""})
        self.assertFalse(bw.get("allowMultiple", False))
        for entry in bw.get("schema", []):
            self.assertIn(entry["key"], bw["defaults"])
            self.assertEqual(entry["defaultValue"], bw["defaults"][entry["key"]])
        label = next(e for e in bw["schema"] if e["key"] == "label")
        self.assertEqual(label["options"], ["name", "none"])

    def test_description_is_honest(self):
        d = self.m["description"]
        self.assertNotIn("Mock", d)
        self.assertNotIn("task", d.lower())
        for app in ("Claude Code", "Codex", "OpenCode", "Hermes", "Grok"):
            self.assertIn(app, d)

    def test_required_files_and_no_symlinks(self):
        for name in ("manifest.json", "BarWidget.qml", "BarPopup.qml", "BarRpc.qml", "PresenceRecorder.qml",
                     "PopupApp.qml", "PopupHeader.qml", "PopupTheme.qml", "HomeScreen.qml", "PickerScreen.qml",
                     "ConnectScreen.qml", "HelpersScreen.qml", "CodexGuide.qml", "MoreScreen.qml", "HelpScreen.qml",
                     "MemoriesScreen.qml", "MemorialScreen.qml", "GoodbyeScreen.qml", "MenuScreen.qml",
                     "NameEditor.qml", "PetPlayer.qml", "CodePetBar.js", "petstate.js", "Strings.js", "strings.json",
                     "PresenceModel.js", "README.md", "CHANGELOG.md", "LICENSE", "assets/icons/index.json"):
            self.assertTrue((PLUGIN / name).is_file(), name)
        links = [p for p in PLUGIN.rglob("*") if p.is_symlink()]
        self.assertEqual(links, [])
        self.assertFalse((PLUGIN / "LifeText.js").exists(), "LifeText.js is staged, never checked in here")
        self.assertEqual((PLUGIN / "LICENSE").read_text(), (REPO / "LICENSE").read_text())

    def test_retired_v02_ui_is_gone(self):
        for name in ("BarPopup.qml", "BarWidget.qml", "CodePetBar.js"):
            text = (PLUGIN / name).read_text()
            for gone in ("Tasks…", "readiness", "Refresh", "sleepCountdown", "hoverGrace", "tooltip(", "lifecycleText"):
                self.assertNotIn(gone, text, f"{name} still has {gone}")
            if name.endswith(".qml"):
                self.assertNotIn("simulation", text.lower(), name)


class SourceContract(unittest.TestCase):
    def setUp(self):
        self.src = {p.name: strip_comments(p.read_text()) for p in QML_FILES + JS_FILES}
        self.qml = {k: v for k, v in self.src.items() if k.endswith(".qml")}
        self.all_qml = "\n".join(self.qml.values())

    def test_exactly_the_named_timers(self):
        self.assertEqual(self.all_qml.count("Timer {"), len(TIMERS))
        for name, (file, repeat) in TIMERS.items():
            self.assertEqual(self.all_qml.count(f'objectName: "{name}"'), 1, name)
            text = self.qml[file]
            timer = next(b for b in blocks(text, "Timer {") if f'objectName: "{name}"' in b)
            self.assertIn(f"repeat: {'true' if repeat else 'false'}", timer, name)
        rec = blocks(self.qml["PresenceRecorder.qml"], "Timer {")[0]
        self.assertIn("interval: 60000", rec)
        self.assertIn("running: recorder.eligible", rec)
        frames = next(b for b in blocks(self.qml["PopupApp.qml"], "Timer {") if "codepetPopup.frames" in b)
        self.assertIn("running: popupRoot.animating", frames)
        # The animation clock runs only while the popup is open, On and not in Calm mode.
        anim = re.search(r"readonly property bool animating:([^\n]*\n[^\n]*)", self.qml["PopupApp.qml"]).group(1)
        for need in ("ctl.open === true", "popupRoot.isOn", "!popupRoot.calm"):
            self.assertIn(need, anim)
        dog = blocks(self.qml["BarRpc.qml"], "Timer {")[0]
        self.assertIn("interval: rpc.responseTimeoutMs", dog)
        self.assertIn("responseTimeoutMs: 15000", self.qml["BarRpc.qml"])
        # The face one-shot is never armed while Off (wake_at_ms is null then, and isOn guards it).
        arm = function_bodies(self.qml["BarWidget.qml"])["armWake"]
        self.assertIn("root.isOn", arm)
        self.assertIn("faceWake.stop()", arm)

    def test_no_timer_handler_sends_a_request_or_launches(self):
        for name, text in self.qml.items():
            funcs = function_bodies(text)
            for timer in blocks(text, "Timer {"):
                for bad in ("request", "launch", "execDetached", "startup", "openPopup", "openHome", "care",
                            "setPower", "start", "noteShown", "turnOnAndOpen"):
                    self.assertNotRegex(timer, rf"(?<!Presence\.)\b{bad}\(", f"{name}: Timer handler must not call {bad}")
                for called in re.findall(r"(?:recorder|card|root|popupRoot|rpc)\.(\w+)\(", timer):
                    body = funcs.get(called, "")
                    self.assertNotIn("request(", body, f"{name}: timer -> {called} sends a request")
                    self.assertNotIn("execDetached", body, f"{name}: timer -> {called} launches")

    def test_state_get_only_on_startup_and_popup_open(self):
        allowed = {"startup", "openPopup"}
        for name, text in self.qml.items():
            if name == "BarRpc.qml":
                continue
            count = text.count('"state.get"')
            in_allowed = sum(body.count('"state.get"') for fn, body in function_bodies(text).items() if fn in allowed)
            self.assertEqual(count, in_allowed, f"{name}: state.get outside startup/openPopup")
        bw = self.qml["BarWidget.qml"]
        self.assertIn("Qt.callLater(root.startup)", bw)
        self.assertIn("!root.leader", function_bodies(bw)["startup"])
        self.assertIn("root.startupDone = true", function_bodies(bw)["startup"])

    def test_file_events_never_launch_rust(self):
        for name, text in self.qml.items():
            for handler in re.findall(r"on(?:FileChanged|Loaded|LoadFailed|Saved|SaveFailed)\s*:\s*([^\n]*)", text):
                self.assertNotIn("request(", handler, name)
                self.assertNotIn("execDetached", handler, name)
                self.assertNotIn("launch(", handler, name)
        self.assertIn("onFileChanged: reload()", self.qml["BarWidget.qml"])

    def test_request_methods_are_allowlisted_and_never_task(self):
        model = self.src["CodePetBar.js"]
        allow = json.loads("[" + re.search(r"var ALLOWED_METHODS = \[([^\]]*)\]", model, re.S).group(1) + "]")
        proto = json.loads((CONTRACTS / "protocol.json").read_text())
        known = set(proto["kept"]) | {m["method"] for m in proto["added"]}
        self.assertTrue(set(allow) <= known, set(allow) - known)
        for gone in ("pet.adopt", "pet.configure_life", "hello", "lifecycle.rules"):
            self.assertNotIn(gone, allow)
        for name, text in self.qml.items():
            for method in re.findall(r"request\(\s*\"([^\"]+)\"", text):
                self.assertIn(method, allow, f"{name} sends {method}")
            self.assertNotRegex(text, r"\"task\.", f"{name} mentions a task method")
        rpc = self.qml["BarRpc.qml"]
        req = function_bodies(rpc)["request"]
        self.assertLess(req.index("Model.allowedMethod(method)"), req.index("createObject"),
                        "allowlist is checked before any process exists")
        self.assertLess(req.index("Model.allowedMethod(method)"), req.index("rpc.pending"),
                        "a refused method is never queued")
        self.assertIn("readonly property int maxPending: 8", rpc)
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
        bw = self.qml["BarWidget.qml"]
        launch = function_bodies(bw)["launch"]
        self.assertIn("var argv = [root.paths.launcher, verb]", launch)
        self.assertIn("root.launcherVerbs.indexOf(verb) < 0", launch)
        self.assertIn('argv = ["/usr/bin/env"].concat(root.paths.launcherEnv, argv)', launch)
        self.assertIn('if (root.paths.launcher.indexOf("=") >= 0) return false', launch)
        self.assertIn('readonly property var launcherVerbs: ["show", "hide", "toggle"]', bw)

    def test_mouse_buttons_and_the_off_click(self):
        bw = self.qml["BarWidget.qml"]
        self.assertIn("acceptedButtons: Qt.LeftButton | Qt.RightButton", bw)
        self.assertNotIn("MiddleButton", bw, "middle click is retired")
        press = function_bodies(bw)["triggerPress"]
        self.assertIn("Qt.RightButton", press)
        self.assertIn('root.openPopup("menu", "")', press)
        self.assertIn("else if (!root.isOn) root.turnOnAndOpen()", press)
        on = function_bodies(bw)["turnOnAndOpen"]
        self.assertIn('"app.set_enabled", { enabled: true }', on)
        self.assertIn("bar.registerClickTarget(root)", bw)

    def test_open_home_ipc(self):
        bw = self.qml["BarWidget.qml"]
        ipc = block_after(bw, bw.index("IpcHandler {"))
        self.assertIn('target: "greyforge.codepet"', ipc)
        self.assertIn("enabled: root.leader", ipc)
        self.assertIn("function openHome(): void { root.openHome() }", ipc)

    def test_state_home_reaches_files_rpc_and_launcher(self):
        bw = self.qml["BarWidget.qml"]
        self.assertIn("readonly property var paths: Model.paths(cfg, env)", bw)
        self.assertIn("path: root.paths.barState", bw)
        self.assertIn("stateDir: root.paths.stateDir", bw)
        self.assertIn("childEnv: root.paths.childEnv", bw)
        self.assertIn("root.paths.launcherEnv", function_bodies(bw)["launch"])
        self.assertIn('XDG_STATE_HOME: Quickshell.env("XDG_STATE_HOME")', bw)
        self.assertIn('"Test state: "', self.qml["HelpScreen.qml"])

    def test_nothing_is_written_inside_the_plugin_dir(self):
        set_text = re.findall(r"(\w+)\.setText\(", self.all_qml)
        self.assertEqual(set_text, ["presenceFile"], "presence.json is the only file the plugin writes")
        rec = self.qml["PresenceRecorder.qml"]
        writer = block_after(rec, rec.index("id: presenceFile") - 40)
        self.assertIn('path: recorder.stateDir !== "" ? recorder.stateDir + "/presence.json" : ""', writer)
        self.assertIn("atomicWrites: true", writer)
        for name, text in self.src.items():
            for bad in ("resolvedUrl", "__sourceDir", "writeAdapter", "setData(", "XMLHttpRequest"):
                self.assertNotIn(bad, text, f"{name} uses {bad}")
        for name, text in self.qml.items():
            for fv in blocks(text, "FileView {"):
                path = re.search(r"path:\s*([^\n]+)", fv).group(1)
                self.assertRegex(path, r"^(root\.paths\.|recorder\.stateDir)", f"{name}: {path}")
        bw = self.qml["BarWidget.qml"]
        self.assertIn("bar.shell.updateEntryInline(root.moduleName, next)", bw)
        self.assertIn('["label", "presence", "overlayOnLoad", "calm"].indexOf(key) < 0', function_bodies(bw)["setSetting"])

    def test_presence_recorder_rules(self):
        rec = self.qml["PresenceRecorder.qml"]
        self.assertIn("timeout: 180", rec)
        self.assertIn("respectInhibitors: false", rec)
        self.assertIn("Hyprland.focusedWorkspace.hasFullscreen", rec)
        self.assertIn("presence.wanted === true", rec)
        self.assertIn("onIdleChanged: if (recorder.idle) Presence.stop(recorder.model)", rec)
        bw = self.qml["BarWidget.qml"]
        self.assertIn("active: root.leader", bw)
        self.assertIn("items.length > 0 && items[0] === root", bw)
        self.assertIn("root.stateDirReady = true", bw)

    def test_popup_is_a_keyboard_panel_in_a_loader(self):
        bw = self.qml["BarWidget.qml"]
        loader = block_after(bw, bw.index("id: popupLoader") - 30)
        self.assertIn("active: root.menu", loader)
        self.assertIn("onLoaded: item.open = true", loader)
        self.assertIn("BarPopup {", loader)
        pop = self.qml["BarPopup.qml"]
        self.assertIn("KeyboardPanel {", pop)
        self.assertIn("PopupApp {", pop)
        self.assertIn("Style.space(480)", pop, "the card height cap")
        self.assertIn("screenH * 0.9", pop)

    def test_popup_screens_need_no_quickshell(self):
        for name, text in self.qml.items():
            imports = re.findall(r"(?m)^import\s+(\S+)", (PLUGIN / name).read_text())
            if name in SHELL_FILES:
                continue
            for imp in imports:
                self.assertTrue(imp in ("QtQuick", "QtQuick.Effects", "qs.Commons") or imp.startswith('"'),
                                f"{name} imports {imp}: the PySide smokes could not load it")

    def test_header_with_power_switch_on_every_screen(self):
        app = self.qml["PopupApp.qml"]
        header = block_after(app, app.index("PopupHeader {"))
        self.assertIn('visible: popupRoot.mode !== "menu"', header)
        self.assertIn("powerOn: popupRoot.isOn", header)
        self.assertIn("onPower: on => popupRoot.setPower(on)", header)
        self.assertIn('icon: "care-floor/power"', self.qml["PopupHeader.qml"])

    def test_text_is_plain_and_accessible(self):
        self.assertNotIn("Text.RichText", self.all_qml)
        self.assertNotIn("Text.StyledText", self.all_qml)
        # A Text without textFormat is AutoText, which renders a pet name such as "<b>Pip</b>"
        # as rich text: every Text (and every component based on it) opts out explicitly.
        found = 0
        for name, text in self.qml.items():
            for m in re.finditer(r"(?:component\s+\w+\s*:\s*)?\bText\s*\{", text):
                block = block_after(text, m.start())
                found += 1
                self.assertIn("textFormat: Text.PlainText", block, f"{name}: {block[:120]}")
        self.assertGreaterEqual(found, 30)
        for name in ("BigButton.qml", "RoundButton.qml", "CareTile.qml", "SwitchPill.qml", "IconButton.qml"):
            self.assertIn("Accessible.role:", self.qml[name], name)
            self.assertIn("activeFocusOnTab: true", self.qml[name], name)

    def test_no_tooltips_anywhere(self):
        for name, text in self.qml.items():
            for bad in ("showTooltip", "tooltipHovered", "ToolTip"):
                self.assertNotIn(bad, text, f"{name}: SPEC-v0.3 §3 has no tooltips")

    def test_js_libraries_are_pure(self):
        for js in JS_FILES:
            text = js.read_text()
            self.assertTrue(text.startswith(".pragma library\n"), js.name)
            self.assertNotRegex(text, r"(?m)^\s*\.?import\b", js.name)
            for bad in ("Quickshell", "Qt.", "XMLHttpRequest", "require("):
                self.assertNotIn(bad, strip_comments(text), f"{js.name} uses {bad}")

    def test_strings_hold_every_contract_string(self):
        r = subprocess.run([sys.executable, str(REPO / "tools/ui/gen_plugin_strings.py"), "--check"], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        plugin = json.loads((PLUGIN / "strings.json").read_text())
        contract = json.loads((CONTRACTS / "strings.json").read_text())
        for key, text in contract.items():
            self.assertEqual(plugin.get(key), text, key)

    def test_every_icon_used_is_shipped(self):
        index = json.loads((PLUGIN / "assets/icons/index.json").read_text())["icons"]
        used = set()
        for name, text in self.src.items():
            used |= set(re.findall(r'"((?:care-floor|care-aquatic|care-robot|status|apps|bar|ui)/[a-z0-9_]+)"', text))
        self.assertGreater(len(used), 20)
        for icon in sorted(used):
            self.assertIn(icon, index, icon)
            self.assertTrue((PLUGIN / "assets/icons" / (icon + ".webp")).is_file(), icon)

    def test_theme_members_exist_in_the_stub_and_the_real_commons(self):
        used = set()
        for name, text in self.qml.items():
            used |= set(re.findall(r"\b(Style\.[a-zA-Z]+(?:\.[a-zA-Z]+)?|Color\.[a-zA-Z]+(?:\.[a-zA-Z]+)?)", text))
        self.assertTrue(used)
        stubs = {"Style": (TESTS / "stubs/qs/Commons/Style.qml").read_text(),
                 "Color": (TESTS / "stubs/qs/Commons/Color.qml").read_text()}
        real = {k: (OMARCHY_SHELL / "Commons" / f"{k}.qml") for k in ("Style", "Color")}
        for member in sorted(used):
            single, *parts = member.split(".")
            for leaf in parts:
                self.assertRegex(stubs[single], rf"\b{leaf}\b", f"{member} missing from the PySide stub")
                if real[single].is_file():
                    self.assertRegex(real[single].read_text(), rf"\b{leaf}\b", f"{member} missing from Omarchy {single}")

    def test_the_popup_smokes_exist(self):
        for smoke in SMOKES:
            self.assertTrue((TESTS / smoke).is_file(), smoke)
            text = (TESTS / smoke).read_text()
            self.assertIn('import "../../omarchy-plugin"', text, smoke)
            self.assertIn("kit.run(", text, smoke)
        home = (TESTS / "HomeSmoke.qml").read_text()
        self.assertIn("[12, 16, 20]", home, "K13 runs at base sizes 12, 16 and 20")

    def test_copy_avoids_banned_words(self):
        banned = re.compile(r"kill|starv|your fault|abandon|neglected you|guilt|punish|streak|hurry|!!!", re.I)
        for p in QML_FILES + JS_FILES:
            for lit in re.findall(r'"((?:[^"\\\n]|\\.)*)"', strip_comments(p.read_text())):
                self.assertIsNone(banned.search(lit), f"{p.name}: {lit}")

    @unittest.skipUnless(qmlformat(), "qmlformat unavailable; QML parse gate untested")
    def test_qmlformat_parses_every_qml_file(self):
        tool = qmlformat()
        for p in QML_FILES + sorted(TESTS.glob("*.qml")) + sorted((TESTS / "stubs").rglob("*.qml")):
            r = subprocess.run([tool, str(p)], capture_output=True, text=True)
            self.assertEqual(r.returncode, 0, f"{p.name}: {r.stderr}")


class Staging(unittest.TestCase):
    def test_staged_copy_is_plain_and_lifetext_is_identical(self):
        with tempfile.TemporaryDirectory() as tmp:
            dest = stage_plugin(Path(tmp) / PLUGIN_ID)
            self.assertEqual([p for p in dest.rglob("*") if p.is_symlink()], [])
            for p in (q for q in PLUGIN.rglob("*") if q.is_file()):
                rel = p.relative_to(PLUGIN)
                self.assertEqual((dest / rel).read_bytes(), p.read_bytes(), str(rel))
            if LIFETEXT.is_file():
                self.assertEqual((dest / "LifeText.js").read_bytes(), LIFETEXT.read_bytes())

    @unittest.skipUnless(os.environ.get("CODEPET_PLUGIN_STAGE_DIR") and shutil.which("omarchy"),
                         "set CODEPET_PLUGIN_STAGE_DIR (and have omarchy) to validate a staged copy")
    def test_omarchy_plugin_validate_on_a_staged_copy(self):
        dest = stage_plugin(Path(os.environ["CODEPET_PLUGIN_STAGE_DIR"]) / PLUGIN_ID)
        r = subprocess.run(["omarchy", "plugin", "validate", str(dest)], capture_output=True, text=True, timeout=60)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)


# --------------------------------------------------------------------------- live load

FAKE_CODEPET = r'''#!{python} -I
# Test-only stand-in for `codepet rpc`: logs the request and the state home it saw,
# applies On/Off and care to bar-state.json (as the real CLI exports) and answers ok.
import json, os, sys
home = os.environ["HOME"]
state_home = os.environ.get("XDG_STATE_HOME", "")
req = json.loads(sys.stdin.readline())
with open(os.path.join(home, "rpc.log"), "a") as log:
    log.write(json.dumps({{"method": req["method"], "id": req["id"], "params": req["params"],
                          "argv": sys.argv[1:], "state_home": state_home}}) + "\n")
sdir = os.path.join(state_home or os.path.join(home, ".local/state"), "codepet")
path = os.path.join(sdir, "bar-state.json")
try:
    doc = json.load(open(path))
except Exception:
    doc = None
result = {{"changed": True}}
if doc is not None:
    if req["method"] == "app.set_enabled":
        doc["enabled"] = bool(req["params"]["enabled"])
    doc["state_version"] += 1
    tmp = path + ".tmp"
    with open(tmp, "w") as f:
        json.dump(doc, f)
    os.rename(tmp, path)
    if req["method"] in ("state.get", "app.set_enabled"):
        result = dict(doc, count_helpers=False, schema_version=5)
print(json.dumps({{"v": 1, "id": req["id"], "ok": True, "result": result}}), flush=True)
'''

FAKE_LAUNCHER = r'''#!{python} -I
import json, os, sys
with open(os.path.join(os.environ["HOME"], "launch.log"), "a") as log:
    log.write(json.dumps({{"argv": sys.argv[1:], "state_home": os.environ.get("XDG_STATE_HOME", ""),
                          "bin": os.environ.get("CODEPET_BIN"), "calm": os.environ.get("CODEPET_REDUCED_MOTION")}}) + "\n")
'''

# A wrong `binary` setting: prints a non-protocol line, then would wait for a minute.
GARBLED_CODEPET = r'''#!/bin/sh
echo "$$" >> "$HOME/garbled.pids"
echo garbage
exec sleep 60
'''

HARNESS = r'''import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import "plugin" as P
import "tests" as T

// Test-only host: a fake bar facade around two real plugin instances (two monitors).
ShellRoot {
    id: shell
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
        property var clickTargets: []
        function moduleWidgets(id) { return id === "greyforge.codepet" ? [w1, w2] : [] }
        function showTooltip(t, s) {}
        function hideTooltip(t) {}
        function requestPopout(o) { activePopout = o }
        function releasePopout(o) { if (activePopout === o) activePopout = null }
        function registerClickTarget(t) { clickTargets = clickTargets.concat([t]) }
        function unregisterClickTarget(t) { clickTargets = clickTargets.filter(function (x) { return x !== t }) }
        function targetBelongsToWindow(t, w) { return false }
    }
    FloatingWindow {
        implicitWidth: 400
        implicitHeight: 60
        Row {
            P.BarWidget { id: w1; bar: fakeBar }
            P.BarWidget { id: w2; bar: fakeBar }
        }
    }
    T.SmokeKit { id: kit; autoLoad: false }
    function app() { return w1.menu && w1.children ? shell.popupOf(w1) : null }
    function popupOf(w) {
        for (var i = 0; i < w.data.length; i++) {
            var d = w.data[i]
            if (d && d.item !== undefined && d.active !== undefined && d.item && d.item.app) return d.item.app
        }
        return null
    }
    function info(w) {
        var timers = {}
        var all = kit.runningTimers(w)
        var named = []
        function walk(o, depth, seen) {
            if (!o || depth > 30 || seen.indexOf(o) >= 0) return
            seen.push(o)
            if (o.objectName && String(o.objectName).indexOf("codepet") === 0 && o.running !== undefined && o.interval !== undefined)
                timers[o.objectName] = o.running
            var lists = []
            try { if (o.data) lists.push(o.data) } catch (e) {}
            try { if (o.item) lists.push([o.item]) } catch (e) {}
            try { if (o.contentItem) lists.push([o.contentItem]) } catch (e) {}
            for (var l = 0; l < lists.length; l++) for (var i = 0; i < lists[l].length; i++) walk(lists[l][i], depth + 1, seen)
        }
        walk(w, 0, [])
        var a = shell.popupOf(w)
        return { leader: w.leader, face: w.face, cell: w.cell, version: w.barState ? w.barState.state_version : -1,
                 menu: w.menu, mode: w.popupMode, busy: w.rpcBusy, notInstalled: w.rpcNotInstalled, isOn: w.isOn,
                 testState: w.paths.testState, stateDir: w.paths.stateDir, stateMissing: w.stateMissing, cfg: w.cfg,
                 timers: timers, running: all, screen: a ? a.screen : "", label: w.labelText, seen: w.seen,
                 badgeScale: w.cell.pulse }
    }
    IpcHandler {
        target: "harness"
        function dump(): string { return JSON.stringify({ w1: shell.info(w1), w2: shell.info(w2), settingsWrites: shell.settingsWrites, targets: fakeBar.clickTargets.length }) }
        function press(button: int): void { w1.triggerPress(button === 2 ? Qt.RightButton : Qt.LeftButton) }
        function close(): void { w1.close() }
        function page(name: string): void { w1.openPage(name) }
        function tap(name: string): void { var o = kit.find(shell.app(), name); if (o) o.tapped() }
        function toggle(name: string, on: bool): void { var o = kit.find(shell.app(), name); if (o) o.toggled(on) }
        function setting(key: string, on: bool): void { w1.setSetting(key, on) }
        function launch(verb: string): void { w1.launch(verb) }
        function refuse(): string {
            var got = ""
            for (var i = 0; i < w1.data.length; i++)
                if (w1.data[i].objectName === "codepetBar.rpc") w1.data[i].request("task.start", {}, function (r) { got = JSON.stringify(r) })
            return got
        }
        // K3/K6/K13 and the kid copy under the real Omarchy Style and Color.
        function kcheck(label: string): string {
            var a = shell.app()
            if (!a) return JSON.stringify({ error: "no popup" })
            kit.failed = []
            kit.all(a, label)
            kit.oneLine(a, label)
            return JSON.stringify({ failed: kit.failed, base: Style.fontBaseSize, height: a.implicitHeight, checks: kit.checks })
        }
    }
}
'''


def fixture(name, **over):
    """A bar-v2 contract fixture moved to now: every millisecond time shifts by the same
    amount, so stale windows and celebrations mean what they meant at the fixture's NOW."""
    doc = json.loads((CONTRACTS / "fixtures/bar-v2" / f"{name}.json").read_text())
    delta = int(time.time() * 1000) - doc["generated_at_ms"]

    def shift(v):
        if isinstance(v, dict):
            return {k: (x + delta if k.endswith("_ms") and isinstance(x, int) and x > 10**12 else shift(x)) for k, x in v.items()}
        if isinstance(v, list):
            return [shift(x) for x in v]
        return v
    doc = shift(doc)
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

    def start(self, installed=True, shell_entry=None, state=None, state_home=None, base_size=None):
        # Short paths: the Quickshell IPC socket lives under XDG_RUNTIME_DIR (108-byte limit).
        self.tmp = tempfile.TemporaryDirectory(prefix="cpbar")
        root = Path(self.tmp.name)
        self.root = root
        home = root / "home"
        self.home = home
        for d in (home, root / "run", home / ".local/bin", home / ".config/omarchy", home / ".local/share/codepet"):
            d.mkdir(mode=0o700, parents=True, exist_ok=True)
        # The installed runtime is this checkout (read-only link): pet art and the registry.
        (home / ".local/share/codepet/current").symlink_to(REPO)
        stage = root / "stage"
        stage.mkdir()
        shutil.copytree(OMARCHY_SHELL / "Ui", stage / "Ui")
        shutil.copytree(OMARCHY_SHELL / "Commons", stage / "Commons")
        shutil.copyfile(TESTS / "stubs/live/KeyboardPanel.qml", stage / "Ui/KeyboardPanel.qml")
        stage_plugin(stage / "plugin")
        (stage / "tests").mkdir()
        shutil.copyfile(TESTS / "SmokeKit.qml", stage / "tests/SmokeKit.qml")
        # SmokeKit imports the model by its repository path; point it at the staged plugin.
        kit = (stage / "tests/SmokeKit.qml").read_text().replace('"../../omarchy-plugin/', '"../plugin/')
        (stage / "tests/SmokeKit.qml").write_text(kit)
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
        if base_size is not None:
            self.base(base_size)
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

    def base(self, size):
        """Sets the Omarchy font base size (the shell watches ~/.config/omarchy/shell.toml)."""
        (self.home / ".config/omarchy/shell.toml").write_text(f"[font]\nbase-size = {size}\n")

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
        r = subprocess.run(["qs", "ipc", "-p", str(self.entry), "call", *args], env=self.env,
                           capture_output=True, text=True, timeout=10)
        self.assertEqual(r.returncode, 0, (args, r.stdout, r.stderr, self.output()))
        return r.stdout.strip()

    def harness(self, *args):
        return self.call("harness", *args)

    def dump(self):
        return json.loads(self.harness("dump"))

    def rpc_log(self):
        p = self.home / "rpc.log"
        return [json.loads(line) for line in p.read_text().splitlines()] if p.exists() else []

    def launch_log(self):
        p = self.home / "launch.log"
        return [json.loads(line) for line in p.read_text().splitlines()] if p.exists() else []

    def write_state(self, doc):
        tmp = self.state_dir / "bar-state.json.tmp"
        tmp.write_text(json.dumps(doc))
        os.rename(tmp, self.state_dir / "bar-state.json")

    def test_leader_startup_cell_popup_and_refusals(self):
        self.start(state=fixture("idle-happy", state_version=5))
        self.assertTrue(self.wait_for(lambda: len(self.rpc_log()) >= 1))
        time.sleep(1.5)
        log = self.rpc_log()
        self.assertEqual([e["method"] for e in log], ["state.get"], "only the leader reads once at startup")
        self.assertEqual(log[0]["argv"], ["rpc"])
        d = self.dump()
        self.assertTrue(d["w1"]["leader"])
        self.assertFalse(d["w2"]["leader"])
        self.assertEqual(d["targets"], 2, "both cells register as bar click targets")
        self.assertTrue(self.wait_for(lambda: self.dump()["w2"]["version"] == 6), "every screen re-reads after the response")
        d = self.dump()
        self.assertEqual(d["w1"]["face"]["state"], "idle")
        self.assertEqual(d["w1"]["label"], "Ginger")
        self.assertEqual(d["w1"]["cell"]["badge"], "")
        self.assertEqual(d["w1"]["timers"].get("codepet.presenceTick"), True, d["w1"]["timers"])
        self.assertNotIn("codepet.presenceTick", d["w2"]["timers"])
        self.assertEqual(d["w1"]["timers"].get("codepetBar.faceWake"), False, "nothing to wake for")
        self.assertEqual(d["w1"]["timers"].get("codepetBar.rpcWatchdog"), False)
        self.assertNotIn("codepetPopup.frames", d["w1"]["timers"], "popup timers exist only while open")

        # A file event (an external export) updates the cell without any request.
        self.write_state(fixture("waiting", state_version=50))
        self.assertTrue(self.wait_for(lambda: self.dump()["w2"]["version"] == 50, 10))
        time.sleep(1)
        self.assertEqual(len(self.rpc_log()), 1, "file events never launch Rust")
        d = self.dump()
        self.assertEqual(d["w2"]["cell"]["badge"], "bar/badge_hand")

        # Left click opens Home: one state.get; the popup's clock runs while open.
        self.write_state(fixture("idle-happy", state_version=60))
        self.assertTrue(self.wait_for(lambda: self.dump()["w1"]["version"] == 60, 10))
        self.harness("press", "1")
        self.assertTrue(self.wait_for(lambda: len(self.rpc_log()) >= 2))
        self.assertEqual(self.rpc_log()[1]["method"], "state.get")
        self.assertTrue(self.wait_for(lambda: self.dump()["w1"]["screen"] == "home"), self.dump())
        self.assertEqual(self.dump()["w1"]["timers"].get("codepetPopup.frames"), True)

        # A care tap from the popup: one request, the bubble at once.
        self.harness("tap", "cp.tile.food")
        self.assertTrue(self.wait_for(lambda: any(e["method"] == "pet.care" for e in self.rpc_log())))
        care = [e for e in self.rpc_log() if e["method"] == "pet.care"][0]
        self.assertEqual(care["params"], {"action": "food"})
        self.assertRegex(care["id"], r"^bar-[a-z0-9]+-\d+$", "a care press carries its retry key")

        # Refusals happen client-side, before any process exists.
        self.assertIn("CLIENT_REFUSED", self.harness("refuse"))
        time.sleep(0.5)
        self.assertFalse(any(e["method"].startswith("task.") for e in self.rpc_log()))

        # Launcher verbs: only show/hide/toggle, as a detached argv.
        self.harness("launch", "toggle")
        self.harness("launch", "rm -rf")
        self.harness("launch", "tasks")
        self.assertTrue(self.wait_for(lambda: len(self.launch_log()) >= 1))
        time.sleep(0.5)
        self.assertEqual([e["argv"] for e in self.launch_log()], [["toggle"]])

        # Settings go through the shell facade only; Calm reaches the launcher.
        self.harness("setting", "calm", "true")
        writes = self.dump()["settingsWrites"]
        self.assertEqual(writes[-1]["id"], "greyforge.codepet")
        self.assertEqual(writes[-1]["entry"]["calm"], True)
        # (`qs ipc call` reads "show" as its own subcommand, so another verb is used.)
        self.harness("launch", "hide")
        self.assertTrue(self.wait_for(lambda: len(self.launch_log()) >= 2))
        self.assertEqual(self.launch_log()[-1]["calm"], "1")

        # The right click opens the small menu; IPC openHome opens Home.
        self.harness("close")
        self.harness("press", "2")
        self.assertTrue(self.wait_for(lambda: self.dump()["w1"]["screen"] == "menu"))
        self.call("greyforge.codepet", "openHome")
        self.assertTrue(self.wait_for(lambda: self.dump()["w1"]["mode"] == "popup" and self.dump()["w1"]["screen"] == "home"),
                        self.dump()["w1"])
        self.harness("close")

    def test_off_is_really_off_and_one_click_turns_it_on(self):
        self.start(state=fixture("off", state_version=7))
        self.assertTrue(self.wait_for(lambda: self.dump()["w1"]["version"] == 8))
        d = self.dump()
        self.assertFalse(d["w1"]["isOn"])
        self.assertEqual(d["w1"]["cell"]["badge"], "bar/badge_moon")
        self.assertEqual(d["w1"]["cell"]["opacity"], 0.4)
        # K9: no running timer in the bar widget while Off (presence is not wanted).
        self.assertTrue(self.wait_for(lambda: self.dump()["w1"]["running"] == []), self.dump()["w1"]["running"])
        self.assertEqual(self.dump()["w2"]["running"], [])
        # A left click on the Off cell turns CodePet on and opens Home.
        self.harness("press", "1")
        self.assertTrue(self.wait_for(lambda: any(e["method"] == "app.set_enabled" for e in self.rpc_log())))
        on = [e for e in self.rpc_log() if e["method"] == "app.set_enabled"][0]
        self.assertEqual(on["params"], {"enabled": True})
        self.assertTrue(self.wait_for(lambda: self.dump()["w1"]["isOn"] and self.dump()["w1"]["screen"] == "home"), self.dump()["w1"])
        # The header switch turns it off again: the Off look, at once.
        self.harness("toggle", "cp.power", "false")
        self.assertTrue(self.wait_for(lambda: self.dump()["w1"]["screen"] == "off_look"), self.dump()["w1"])
        self.harness("close")
        self.assertTrue(self.wait_for(lambda: self.dump()["w1"]["running"] == []), self.dump()["w1"]["running"])

    def test_kid_sizes_and_fit_under_the_real_omarchy_theme(self):
        """K3/K6/K13 and the kid copy at font base sizes 12, 16 and 20, real Style and Color."""
        self.start(state=fixture("idle-treats", state_version=7), base_size=12)
        self.assertTrue(self.wait_for(lambda: self.dump()["w1"]["version"] >= 8))
        cases = [("idle-treats", ""), ("need-food-critical", ""), ("fainted", ""), ("asleep", ""), ("off", ""),
                 ("working-two", ""), ("fresh-no-pet", ""), ("owner-unarmed-welcome", ""), ("memorial", ""),
                 ("clients-mixed", "helpers"), ("idle-happy", "more"), ("idle-happy", "change")]
        version = 100
        for size in (12, 16, 20):
            self.base(size)
            for name, page in cases:
                version += 1
                self.write_state(fixture(name, state_version=version))
                self.assertTrue(self.wait_for(lambda: self.dump()["w1"]["version"] >= version, 10))
                if page:
                    self.harness("page", page)
                else:
                    self.harness("press", "1") if not self.dump()["w1"]["menu"] else None
                self.assertTrue(self.wait_for(lambda: self.dump()["w1"]["screen"] not in ("", "loading"), 10))
                time.sleep(0.4)
                res = json.loads(self.harness("kcheck", f"{name}{'/' + page if page else ''} @{size}"))
                self.assertEqual(res.get("failed"), [], res)
                self.assertEqual(res["base"], size, "the shell picked up the base size")
                self.harness("close")
                self.assertTrue(self.wait_for(lambda: not self.dump()["w1"]["menu"], 5))

    def test_state_home_setting_reaches_rpc_launcher_and_files(self):
        qa = self.make_tmp_state_home()
        self.start(shell_entry={"stateHome": str(qa)}, state=fixture("idle-happy", state_version=7), state_home=qa)
        self.assertTrue(self.wait_for(lambda: len(self.rpc_log()) >= 1))
        self.assertEqual(self.rpc_log()[0]["state_home"], str(qa))
        self.assertTrue(self.wait_for(lambda: self.dump()["w1"]["version"] == 8))
        d = self.dump()
        self.assertTrue(d["w1"]["testState"])
        self.assertEqual(d["w1"]["stateDir"], str(qa / "codepet"))
        self.harness("launch", "toggle")
        self.assertTrue(self.wait_for(lambda: len(self.launch_log()) >= 1))
        self.assertEqual(self.launch_log()[0], {"argv": ["toggle"], "state_home": str(qa), "bin": None, "calm": None})
        self.assertFalse((self.home / ".local/state/codepet").exists(), "the default state home is untouched")

    def test_binary_setting_reaches_the_launcher(self):
        alt = "/opt/elsewhere/codepet"
        self.start(shell_entry={"binary": alt}, state=fixture("idle-happy", state_version=7))
        self.harness("launch", "toggle")
        self.assertTrue(self.wait_for(lambda: len(self.launch_log()) >= 1))
        self.assertEqual(self.launch_log()[0]["argv"], ["toggle"])
        self.assertEqual(self.launch_log()[0]["bin"], alt)

    def test_a_refused_answer_ends_the_child_before_the_slot_frees(self):
        garbled = self.make_tmp_state_home().parent / "garbled-codepet"
        garbled.write_text(GARBLED_CODEPET)
        garbled.chmod(0o700)
        self.start(shell_entry={"binary": str(garbled)}, state=fixture("idle-happy", state_version=7))
        pids = self.home / "garbled.pids"
        self.assertTrue(self.wait_for(lambda: pids.exists() and pids.read_text().strip() != ""), self.output())
        pid = int(pids.read_text().split()[0])

        def alive():
            try:
                os.kill(pid, 0)
            except ProcessLookupError:
                return False
            return Path(f"/proc/{pid}").exists() and "Z" not in Path(f"/proc/{pid}/stat").read_text().split()[2]
        self.assertTrue(self.wait_for(lambda: not alive(), 5), "the refused child is signalled, not left running")
        self.assertTrue(self.wait_for(lambda: not self.dump()["w1"]["busy"], 5), "the slot frees once it exits")
        self.assertEqual(len(pids.read_text().split()), 1, "no second child started next to the first")

    def make_tmp_state_home(self):
        holder = tempfile.TemporaryDirectory(prefix="cpqa")
        self.addCleanup(holder.cleanup)
        return Path(holder.name) / "qa-state"

    def test_not_installed_and_no_state(self):
        self.start(installed=False)
        self.assertTrue(self.wait_for(lambda: self.dump()["w1"]["notInstalled"], 10), self.output())
        d = self.dump()
        self.assertFalse(d["w1"]["busy"])
        self.assertTrue(d["w1"]["stateMissing"])
        self.assertEqual(d["w1"]["cell"]["thumb"], "ui/egg")
        self.assertEqual(d["w1"]["label"], "CodePet")


if __name__ == "__main__":
    unittest.main(verbosity=2)
