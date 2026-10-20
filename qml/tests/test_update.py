#!/usr/bin/env python3
"""Offline updater trust/concurrency fixtures; never fetch or change the installed pet.

The integration fixture executes the real install.sh with a temporary HOME and stub
shell tools. CODEPET_UPDATE_TEST_BINARY optionally selects a qualified real backend
for an additional migrate/SQLite-preservation install; it is not an updater override.
CODEPET_UPDATE_TEST_ARCHIVE optionally replaces the synthetic integration archive
with a locally built candidate archive; all network and shell tools remain fixtures.
CODEPET_UPDATE_TEST_PREVIOUS_BINARY additionally tests an actual older release's
companion data, schema backup and post-migration refusal by that older binary.
"""
import contextlib
import hashlib
import importlib.machinery
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import unittest
import time
import uuid
from unittest import mock

sys.dont_write_bytecode = True
REPO = Path(__file__).resolve().parents[2]
VERSION = tomllib.loads((REPO / "Cargo.toml").read_text())["workspace"]["package"]["version"]
SCRIPT = REPO / "packaging/omarchy/codepet-update"
loader = importlib.machinery.SourceFileLoader("codepet_update", str(SCRIPT))
spec = importlib.util.spec_from_loader(loader.name, loader)
update = importlib.util.module_from_spec(spec)
loader.exec_module(update)


def executable(path, body):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(body)
    path.chmod(0o700)


def digest(data):
    return "sha256:" + hashlib.sha256(data).hexdigest()


def metadata(latest="0.4.0", archive=b"archive", checksums=None):
    name = f"codepet-{latest}-x86_64-linux.tar.gz"
    sums = checksums if checksums is not None else (hashlib.sha256(archive).hexdigest() + "  " + name + "\n").encode()
    return {"tag_name": "v" + latest, "draft": False, "prerelease": False,
            "html_url": "https://github.com/GreyforgeLabs/codepet/releases/tag/v" + latest,
            "assets": [{"name": n, "state": "uploaded", "size": len(data), "digest": digest(data),
                        "browser_download_url": update.DOWNLOAD + "v" + latest + "/" + n}
                       for n, data in ((name, archive), ("SHA256SUMS", sums))]}, sums


class FakeNetwork:
    def __init__(self, release, archive=b"archive", sums=None):
        self.release = release
        latest = release["tag_name"]
        self.content = {update.API: json.dumps(release).encode(),
                        update.DOWNLOAD + latest + "/codepet-" + latest[1:] + "-x86_64-linux.tar.gz": archive,
                        update.DOWNLOAD + latest + "/SHA256SUMS": sums or b""}
        self.calls = []

    def fetch(self, url, limit, destination=None):
        self.calls.append(url)
        data = self.content[url]
        if isinstance(data, Exception):
            raise data
        if len(data) > limit:
            raise update.UpdateError("fixture over limit")
        if destination:
            destination.write_bytes(data)
            destination.chmod(0o600)
            return len(data)
        return data


class Fixture:
    def __init__(self, network):
        self.tmp = tempfile.TemporaryDirectory(prefix="codepet-update-")
        self.home = Path(self.tmp.name) / "home"
        self.home.mkdir(mode=0o700)
        self.bin = self.home / ".local/bin"
        self.bin.mkdir(parents=True)
        self.stubs = Path(self.tmp.name) / "stubs"
        self.stubs.mkdir()
        executable(self.bin / "codepet", "#!/bin/sh\n[ \"$1\" = --version ] && { echo 'codepet 0.3.0'; exit 0; }\nexit 0\n")
        executable(self.stubs / "qs", "#!/bin/sh\n[ \"$1\" = list ] && { echo 'No running instances.'; exit 0; }\nexit 1\n")
        executable(self.stubs / "omarchy", "#!/bin/sh\n[ \"$1 $2\" = 'plugin validate' ] && exit 0\nexit 1\n")
        executable(self.stubs / "omarchy-shell", "#!/bin/sh\nexit 1\n")
        self.env = {"HOME": str(self.home), "PATH": str(self.stubs) + ":/usr/bin:/bin", "XDG_STATE_HOME": str(self.home / ".local/state"), "CODEPET_QS": str(self.stubs / "qs")}
        self.commands = []
        self.service_state = b"active\n"
        self.updater = update.Updater(self.env, network, self.run)

    def run(self, argv, **kwargs):
        self.commands.append(argv)
        if argv[0] == "systemd-run":
            return b""
        if argv[0] == "systemctl":
            return self.service_state
        return update.run_bounded(argv, **kwargs)

    def close(self):
        self.tmp.cleanup()


def archive_bytes(entries):
    out = io.BytesIO()
    with tarfile.open(fileobj=out, mode="w:gz") as archive:
        for entry, data in entries:
            if isinstance(entry, str):
                entry = tarfile.TarInfo(entry)
                entry.size = len(data)
                entry.mode = 0o600
            archive.addfile(entry, io.BytesIO(data))
    return out.getvalue()


class UpdaterTests(unittest.TestCase):
    def setUp(self):
        release, sums = metadata()
        self.network = FakeNetwork(release, sums=sums)
        self.fixture = Fixture(self.network)
        self.addCleanup(self.fixture.close)
        self.u = self.fixture.updater

    def test_numeric_stable_versions(self):
        self.assertGreater(update.version("0.10.0"), update.version("0.9.9"))
        for value in ("0.4", "v0.4.0", "0.4.0-rc.1", "01.0.0", "0.4.0+dev", None):
            with self.subTest(value=value), self.assertRaises(update.UpdateError):
                update.version(value)

    def installed_identity(self, identity="a" * 64):
        current = self.u.data / "current"
        current.mkdir(parents=True, exist_ok=True)
        update.atomic_json(current / "build.json", {"schema": "codepet.build.v1", "identity": identity, "plugin": True})
        return identity

    def test_pending_bar_restart_survives_check_and_version_changes(self):
        expected = self.installed_identity()
        self.u.loaded_identity = lambda: "b" * 64
        result = self.u.record_ui()
        self.assertTrue(result["restart_required"])
        self.assertEqual(result["expected_identity"], expected)
        for operation in (self.u.status, self.u.check, self.u.status):
            self.assertTrue(operation()["restart_required"])
        self.u.loaded_identity = lambda: ""
        old_ui = self.u.check()
        self.assertTrue(old_ui["restart_required"], "old plugin without IPC is not fresh")
        self.assertIn("codepet-update restart-bar", old_ui["message"], "old UI has an actionable recovery path in its existing message field")
        self.assertIn(old_ui["state"], {"available", "current"}, "old state parser remains compatible")
        self.u.loaded_identity = lambda: expected
        fresh = self.u.status()
        self.assertFalse(fresh["restart_required"])
        self.assertNotIn("codepet-update restart-bar", fresh["message"])
        self.assertFalse(json.loads((self.u.root / "ui.json").read_text())["restart_required"])

    def test_restart_is_explicit_supervised_idle_unlocked_and_identity_verified(self):
        expected = self.installed_identity()
        self.u.loaded_identity = lambda: ""
        self.u.ensure_unlocked = lambda: None
        self.u.record_ui()
        result = self.u.restart_bar()
        self.assertEqual(result["restart_state"], "restarting")
        command = next(c for c in self.fixture.commands if c[0] == "systemd-run")
        self.assertIn("_restart", command)
        self.assertIn("--property=RuntimeMaxSec=600", command)
        self.assertFalse(any(c[:3] == ["omarchy", "restart", "shell"] for c in self.fixture.commands))
        calls = []
        def run(argv, **kwargs):
            if argv == ["omarchy", "restart", "shell"]:
                calls.append(argv)
                self.u.loaded_identity = lambda: expected
                return b""
            return self.fixture.run(argv, **kwargs)
        self.u.run = run
        done = self.u.restart_worker(result["job"])
        self.assertFalse(done["restart_required"])
        self.assertEqual(done["restart_state"], "idle")
        self.assertEqual(len(calls), 1)

    def test_restart_refuses_unknown_or_locked_session_and_active_work(self):
        self.installed_identity()
        self.u.loaded_identity = lambda: ""
        original = self.fixture.run
        for lock_state, monitors in [({}, []), ({"secure": True, "requested": False}, [{"solitaryBlockedBy": []}]),
                ({"locked": False, "requested": False, "pending": False, "sessionLocked": False, "secure": False}, [{"solitaryBlockedBy": ["LOCK"]}]),
                ({"locked": False, "requested": False, "pending": False, "sessionLocked": False, "secure": False}, [{"solitaryBlockedBy": ["WORKSPACE"]}])]:
            def run(argv, **kwargs):
                if argv == ["omarchy-shell", "lock", "status"]:
                    return json.dumps(lock_state).encode()
                if argv == ["hyprctl", "-j", "monitors"]:
                    return json.dumps(monitors).encode()
                return original(argv, **kwargs)
            self.u.run = run
            with self.assertRaisesRegex(update.UpdateError, "unlock|locked|session"):
                self.u.restart_bar()
        self.u.ensure_unlocked = lambda: None
        with update.lock(self.u.work / "control.lock"), self.assertRaisesRegex(update.UpdateError, "busy"):
            self.u.restart_bar()
        self.assertFalse(any(c[0] == "systemd-run" for c in self.fixture.commands))

    def test_unconfirmed_restart_keeps_pending_receipt_and_failure(self):
        self.installed_identity()
        self.u.loaded_identity = lambda: ""
        self.u.ensure_unlocked = lambda: None
        self.u.record_ui()
        result = self.u.restart_bar()
        self.u.run = lambda argv, **kw: b"codepet 0.3.0\n" if argv[-1] == "--version" else b"[]" if "list" in argv else b""
        with mock.patch.object(update.time, "monotonic", side_effect=[0, 21]), self.assertRaisesRegex(update.UpdateError, "could not be confirmed"):
            self.u.restart_worker(result["job"])
        self.assertEqual(self.u.load()["restart_state"], "error")
        self.assertTrue(self.u.check()["restart_required"])

    @unittest.skipUnless(os.environ.get("CODEPET_UPDATE_TEST_SYSTEMD") == "1", "optional user-systemd fixture not requested")
    def test_real_service_restart_worker_with_only_fixture_shell_tools(self):
        expected = self.installed_identity()
        executable(self.fixture.stubs / "omarchy-shell", '''#!/usr/bin/python3
import json,os,sys
from pathlib import Path
r=Path(os.environ['HOME'])
if sys.argv[1:]==['lock','status']: print('{"locked":false,"requested":false,"pending":false,"sessionLocked":false,"secure":false}');raise SystemExit(0)
if sys.argv[1:]==['greyforge.codepet','buildIdentity'] and (r/'restarted').exists():
 print(json.loads((r/'.local/share/codepet/current/build.json').read_text())['identity']);raise SystemExit(0)
raise SystemExit(1)
''')
        executable(self.fixture.stubs / "hyprctl", '#!/bin/sh\necho \'[{"solitaryBlockedBy":[]}]\'\n')
        executable(self.fixture.stubs / "omarchy", '#!/bin/sh\n[ "$1 $2" = "restart shell" ] || exit 1\n: > "$HOME/restarted"\n')
        env = self.fixture.env | {key: os.environ[key] for key in ("XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS") if key in os.environ}
        u = update.Updater(env, self.network)
        result = u.restart_bar()
        try:
            for _ in range(100):
                saved = u.load()
                if saved.get('restart_state') != 'restarting':
                    break
                time.sleep(.05)
            self.assertEqual(saved.get('restart_state'), 'idle', saved)
            self.assertFalse(u.status()['restart_required'])
            self.assertEqual(u.ui_fields()['loaded_identity'], expected)
            self.assertTrue((self.fixture.home / 'restarted').exists())
            print('native restart proof: independent user service verified the fixture-only shell identity')
        finally:
            subprocess.run(['systemctl', '--user', 'stop', result['unit']], capture_output=True, timeout=5)

    def test_check_only_metadata_and_same_or_ahead_never_downgrades(self):
        self.assertEqual(self.u.check()["state"], "available")
        self.assertEqual(self.network.calls, [update.API])
        for installed in ("0.4.0", "0.5.0"):
            executable(self.fixture.bin / "codepet", f"#!/bin/sh\necho 'codepet {installed}'\n")
            self.assertEqual(self.u.check()["state"], "current")
            self.assertEqual(self.u.install()["state"], "current")
        self.assertFalse(any(c[0] == "systemd-run" for c in self.fixture.commands))

    def test_metadata_rejects_binding_and_digest_failures(self):
        changes = [lambda r: r.update(draft=True), lambda r: r.update(prerelease=True),
                   lambda r: r.update(tag_name="v0.4.0-rc.1"), lambda r: r.update(html_url="https://github.com/other/repo"),
                   lambda r: r["assets"][0].update(browser_download_url="https://evil.example/file"),
                   lambda r: r["assets"][0].update(digest="sha256:bad"),
                   lambda r: r["assets"][0].update(size=update.MAX_ARCHIVE + 1),
                   lambda r: r["assets"].append(r["assets"][0]), lambda r: r["assets"].pop()]
        for change in changes:
            release, _ = metadata()
            change(release)
            with self.subTest(change=change), self.assertRaises(update.UpdateError):
                update.release_metadata(json.dumps(release))
        release, _ = metadata()
        release["assets"][0]["digest"] = None
        self.assertEqual(update.release_metadata(json.dumps(release))["version"], "0.4.0")

    def test_network_initial_url_and_redirect_restrictions(self):
        for url in ("http://github.com/GreyforgeLabs/codepet/releases/download/v0.4.0/SHA256SUMS", "https://evil.example/x", update.DOWNLOAD + "v0.4.0/other"):
            with self.assertRaises(update.UpdateError):
                update.Network().fetch(url, 10)
        handler = update.Redirects()
        request = update.urllib.request.Request(update.DOWNLOAD + "v0.4.0/SHA256SUMS")
        for url in ("http://github.com/x", "https://evil.example/x", "https://github.com/other/repo", "https://user@release-assets.githubusercontent.com/x", "https://release-assets.githubusercontent.com:444/x"):
            with self.assertRaises(update.UpdateError):
                handler.redirect_request(request, None, 302, "", {}, url)

    def test_network_error_installs_nothing(self):
        self.network.content[update.API] = update.UpdateError("offline fixture")
        before = (self.fixture.bin / "codepet").read_bytes()
        with self.assertRaisesRegex(update.UpdateError, "offline"):
            self.u.install()
        self.assertEqual((self.fixture.bin / "codepet").read_bytes(), before)
        self.assertFalse(any(c[0] == "systemd-run" for c in self.fixture.commands))

    def test_active_work_review_and_unknown_refuse_without_launch(self):
        task_id = "a" * 64
        (self.u.work / "active").write_text(task_id)
        (self.u.work / "active").chmod(0o600)
        taskdir = update.private_dir(self.u.work / task_id)
        for state in ("preparing", "running", "review", "applying", "surprise"):
            update.atomic_json(taskdir / "task.json", {"id": task_id, "status": state})
            with self.subTest(state=state), self.assertRaisesRegex(update.UpdateError, "Finish or discard"):
                self.u.install()
        self.assertEqual(self.network.calls, [])
        update.atomic_json(taskdir / "task.json", {"id": task_id, "status": "scouted"})
        self.assertEqual(self.u.install()["state"], "downloading")

    def test_unreadable_active_work_refuses(self):
        (self.u.work / "active").write_text("../bad")
        (self.u.work / "active").chmod(0o600)
        with self.assertRaisesRegex(update.UpdateError, "unreadable"):
            self.u.install()

    def test_open_window_refuses(self):
        executable(self.fixture.stubs / "qs", "#!/bin/sh\necho '" + json.dumps([{"config_path": str(self.u.data / "current/qml/work.qml"), "pid": 42}]) + "'\n")
        with self.assertRaisesRegex(update.UpdateError, "Close the CodePet Work"):
            self.u.install()

    def test_duplicate_and_locked_work_refuse(self):
        with update.lock(self.u.work / "control.lock"):
            with self.assertRaisesRegex(update.UpdateError, "busy"):
                self.u.install()
            with self.assertRaisesRegex(update.UpdateError, "busy"):
                self.u.open_work("compact")
        result = self.u.install()
        self.assertEqual(self.u.install()["job"], result["job"])
        self.assertEqual(len([c for c in self.fixture.commands if c[0] == "systemd-run"]), 1)
        command = next(c for c in self.fixture.commands if c[0] == "systemd-run")
        for required in ("--property=RuntimeMaxSec=600", "--property=MemoryMax=512M", "--property=KillMode=control-group"):
            self.assertIn(required, command)
        self.assertTrue((self.u.root / result["job"] / "updater.py").exists())

    def test_interrupted_service_has_durable_error_never_reruns(self):
        result = self.u.install()
        self.fixture.service_state = b"inactive\n"
        state = self.u.status()
        self.assertEqual(state["state"], "error")
        self.assertIn("interrupted", state["message"])
        self.assertEqual(self.u.status()["state"], "error")
        self.assertEqual(self.u.load()["state"], "error")
        self.assertEqual(len([c for c in self.fixture.commands if c[0] == "systemd-run"]), 1)
        # A deliberate fresh check recovers normally; it never retries installation.
        self.assertEqual(self.u.check()["state"], "available")

    def test_status_completion_race_does_not_overwrite_success(self):
        result = self.u.install()
        def run(argv, **kwargs):
            if argv[0] == "systemctl":
                self.u.write("succeeded", "done", latest="0.4.0", job=result["job"])
                return b"inactive\n"
            return self.fixture.run(argv, **kwargs)
        self.u.run = run
        self.assertEqual(self.u.status()["state"], "succeeded")

    def test_test_state_refuses_install(self):
        self.u.env["CODEPET_TEST_STATE"] = "1"
        with self.assertRaisesRegex(update.UpdateError, "test-state"):
            self.u.install()
        self.assertEqual(self.network.calls, [])

    def test_private_status_and_lock_symlinks_refuse(self):
        target = self.fixture.home / "sentinel"
        target.write_text("keep")
        (self.u.root / "control.lock").symlink_to(target)
        with self.assertRaises(OSError):
            self.u.check()
        self.assertEqual(target.read_text(), "keep")

    def test_bounded_command_output_timeout_and_log(self):
        with self.assertRaisesRegex(update.UpdateError, "timed out"):
            update.run_bounded([sys.executable, "-c", "import time; time.sleep(3)"], timeout=0.1)
        with self.assertRaisesRegex(update.UpdateError, "output limit"):
            update.run_bounded([sys.executable, "-c", "print('x'*1100000)"])
        log = self.u.root / "install.log"
        with self.assertRaises(update.UpdateError):
            update.run_bounded([sys.executable, "-c", "print('recovery fixture'); raise SystemExit(1)"], log=log)
        self.assertIn("recovery fixture", log.read_text())
        log.write_text("STALE PREVIOUS ATTEMPT")
        with self.assertRaisesRegex(update.UpdateError, "timed out"):
            update.run_bounded([sys.executable, "-c", "import time; print('current backup recovery', flush=True); time.sleep(3)"], timeout=0.1, log=log)
        self.assertEqual(log.read_text(), "current backup recovery\n")

    def test_total_deadline(self):
        with self.assertRaisesRegex(update.UpdateError, "timed out"), update.deadline(1):
            import time
            time.sleep(2)

    def test_malicious_tar_entries_are_refused(self):
        root = "codepet-0.4.0-x86_64-linux"
        cases = []
        for name in (root + "/../escape", "/absolute", "other/file", root + "/line\nbreak", root + "/back\\slash"):
            cases.append([(name, b"x")])
        cases.append([(root + "/same", b"x"), (root + "/same", b"y")])
        for kind in (tarfile.SYMTYPE, tarfile.LNKTYPE, tarfile.FIFOTYPE, tarfile.CHRTYPE):
            entry = tarfile.TarInfo(root + "/link")
            entry.type, entry.linkname = kind, "/tmp/outside"
            cases.append([(entry, b"")])
        special = tarfile.TarInfo(root + "/setuid")
        special.mode = 0o4755
        cases.append([(special, b"")])
        for i, entries in enumerate(cases):
            with self.subTest(case=i):
                archive = self.u.root / f"bad{i}.tar.gz"
                archive.write_bytes(archive_bytes(entries))
                out = update.private_dir(self.u.root / f"out{i}")
                with self.assertRaises(update.UpdateError):
                    update.unpack(archive, out, "0.4.0")
                self.assertFalse((self.u.root / "escape").exists())

    def test_expanded_archive_limit(self):
        archive = self.u.root / "large.tar.gz"
        archive.write_bytes(archive_bytes([("codepet-0.4.0-x86_64-linux/large", b"x" * 20)]))
        with mock.patch.object(update, "MAX_EXPANDED", 10), self.assertRaisesRegex(update.UpdateError, "size limit"):
            update.unpack(archive, update.private_dir(self.u.root / "x"), "0.4.0")


class InstallerIntegration(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory(prefix="codepet-update-archive-")
        candidate = os.environ.get("CODEPET_UPDATE_TEST_ARCHIVE")
        if candidate:
            path = Path(candidate)
            cls.tree = update.unpack(path, update.private_dir(Path(cls.tmp.name) / "extracted"), VERSION)
            cls.archive = path.read_bytes()
            return
        cls.tree = Path(cls.tmp.name) / f"codepet-{VERSION}-x86_64-linux"
        cls.tree.mkdir()
        for relative in ("packaging/omarchy", "omarchy-plugin", "qml/shared", "qml/omarchy", "qml/assets"):
            target = cls.tree / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copytree(REPO / relative, target, ignore=shutil.ignore_patterns("__pycache__"))
        for relative in ("qml/shell.qml", "qml/work.qml"):
            shutil.copy2(REPO / relative, cls.tree / relative)
        (cls.tree / "VERSION").write_text(VERSION + "\n")
        executable(cls.tree / "bin/codepet", f"#!/bin/sh\ncase \"$1\" in --version) echo 'codepet {VERSION}';; migrate) echo 'fixture migration preserves existing data';; clients-refresh) :;; *) exit 1;; esac\n")
        executable(cls.tree / "bin/codepet-pet", f"#!/bin/sh\ncase \"$1\" in --version) echo 'codepet-pet {VERSION}';; *) exit 1;; esac\n")
        cls.archive = cls.make_archive()

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    @classmethod
    def make_archive(cls):
        out = io.BytesIO()
        with tarfile.open(fileobj=out, mode="w:gz") as archive:
            archive.add(cls.tree, arcname=cls.tree.name)
        return out.getvalue()

    def fixture(self, archive=None, sums=None):
        archive = self.archive if archive is None else archive
        release, checksums = metadata(latest=VERSION, archive=archive, checksums=sums)
        network = FakeNetwork(release, archive, checksums)
        fixture = Fixture(network)
        self.addCleanup(fixture.close)
        return fixture

    def test_real_installer_preserves_saved_state_and_installs_helper(self):
        f = self.fixture()
        pet = f.updater.state / "pet-sentinel.json"
        original = b'{"name":"Fixture","born_at":1700000000000,"xp":45,"pinned":true}\n'
        pet.write_bytes(original)
        pet.chmod(0o600)
        job = f.updater.install()
        result = f.updater.worker(job["job"])
        self.assertEqual(result["state"], "succeeded")
        self.assertEqual(result["current"], VERSION)
        self.assertEqual(pet.read_bytes(), original)
        self.assertEqual((f.bin / "codepet-update").read_bytes(), SCRIPT.read_bytes())
        self.assertEqual((f.updater.data / "current").readlink(), Path(f"releases/{VERSION}"))
        self.assertTrue((f.home / ".config/omarchy/plugins/greyforge.codepet/manifest.json").exists())
        self.assertIn(f"done: CodePet {VERSION} is installed", (f.updater.root / "install.log").read_text())
        self.assertFalse((f.updater.root / job["job"] / "extracted").exists())
        self.assertEqual(f.updater.status()["state"], "succeeded")
        self.assertEqual(f.updater.check()["state"], "current")
        self.assertTrue(f.updater.status()["restart_required"], "rescan acknowledgement is not loaded identity proof")
        identity = f.updater.installed_identity()
        self.assertEqual(identity, update.build_identity(self.tree))
        for path in (f.updater.data / 'current/qml/shared/BuildInfo.js', f.home / '.config/omarchy/plugins/greyforge.codepet/BuildInfo.js'):
            self.assertIn(identity, path.read_text())

    def test_content_identity_changes_for_same_version_plugin_and_runtime_edits(self):
        original = update.build_identity(self.tree)
        for relative in ('omarchy-plugin/BuildInfo.js', 'qml/shared/BuildInfo.js'):
            path = self.tree / relative
            before = path.read_bytes()
            try:
                path.write_text('generated identity changes do not hash themselves')
                self.assertEqual(update.build_identity(self.tree), original)
            finally:
                path.write_bytes(before)
        for relative in ('omarchy-plugin/UpdatesScreen.qml', 'qml/shared/DesktopPet.qml', 'qml/assets/pets/registry.json'):
            path = self.tree / relative
            before = path.read_bytes()
            try:
                path.write_bytes(before + b'\n')
                self.assertNotEqual(update.build_identity(self.tree), original)
            finally:
                path.write_bytes(before)

    def test_checksum_mismatch_and_duplicate_never_invoke_installer(self):
        name = f"codepet-{VERSION}-x86_64-linux.tar.gz"
        for sums in (("0" * 64 + "  " + name + "\n").encode(), ((hashlib.sha256(self.archive).hexdigest() + "  " + name + "\n") * 2).encode()):
            f = self.fixture(sums=sums)
            old = (f.bin / "codepet").read_bytes()
            job = f.updater.install()
            with self.assertRaisesRegex(update.UpdateError, "checksum"):
                f.updater.worker(job["job"])
            self.assertEqual((f.bin / "codepet").read_bytes(), old)
            self.assertFalse(any(c[0] == "/bin/bash" for c in f.commands))

    def test_asset_digest_mismatch_never_installs(self):
        f = self.fixture()
        job = f.updater.install()
        url = next(url for url in f.updater.network.content if url.endswith(".tar.gz"))
        f.updater.network.content[url] = b"x" * len(self.archive)
        with self.assertRaisesRegex(update.UpdateError, "digest"):
            f.updater.worker(job["job"])
        self.assertFalse(any(c[0] == "/bin/bash" for c in f.commands))

    def test_work_launch_after_handoff_blocks_worker_without_download(self):
        f = self.fixture()
        job = f.updater.install()
        with update.lock(f.updater.work / "control.lock"), self.assertRaisesRegex(update.UpdateError, "busy"):
            f.updater.worker(job["job"])
        self.assertEqual(f.updater.network.calls, [update.API])

    @unittest.skipUnless(os.environ.get("CODEPET_UPDATE_TEST_SYSTEMD") == "1", "optional user-systemd lifecycle fixture not requested")
    def test_restored_overlay_outlives_updater_service(self):
        f = self.fixture()
        runtime = f.updater.data / "releases/0.3.0/qml"
        runtime.mkdir(parents=True)
        (runtime / "shell.qml").write_text("fixture")
        (f.updater.data / "current").symlink_to("releases/0.3.0")
        old = subprocess.Popen(["/usr/bin/sleep", "60"])
        outer = "codepet-update-test-" + uuid.uuid4().hex
        child_unit = None
        qs = """#!/usr/bin/python3
import json, os, pathlib, subprocess, sys
home = pathlib.Path(os.environ['HOME'])
pidfile = home / 'new-overlay-pid'
if sys.argv[1] == 'list':
    try:
        os.kill(OLD_PID, 0)
        print(json.dumps([{'config_path': str(home / '.local/share/codepet/current/qml/shell.qml'), 'pid': OLD_PID}], indent=2))
    except ProcessLookupError:
        print('No running instances.')
    raise SystemExit(0)
if sys.argv[1] == '-n':
    child = subprocess.Popen(['/usr/bin/sleep', '60'], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    pidfile.write_text(str(child.pid))
    raise SystemExit(0)
if sys.argv[1] == 'ipc' and pidfile.exists():
    os.kill(int(pidfile.read_text()), 0)
    print('codepet-overlay' if sys.argv[-1] == 'ping' else '{"supervisor":false,"pet":"shown"}')
    raise SystemExit(0)
raise SystemExit(1)
""".replace('OLD_PID', str(old.pid))
        executable(f.stubs / "qs", qs)
        env = f.env | {key: os.environ[key] for key in ("XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS") if key in os.environ}
        env.pop("CODEPET_QS", None)
        env["CODEPET_UPDATE_SERVICE"] = "1"
        driver = Path(f.tmp.name) / "install-driver.sh"
        executable(driver, '#!/bin/sh\n/bin/bash "$1" > "$HOME/actual-installer.log" 2>&1\necho "$?" > "$HOME/actual-installer-result"\n')
        command = ["systemd-run", "--user", "--quiet", "--collect", "--service-type=exec", "--unit=" + outer, "--property=RuntimeMaxSec=30", "--property=KillMode=control-group", "--", "/usr/bin/env", "-i"]
        command += [key + "=" + val for key, val in env.items()]
        command += [str(driver), str(self.tree / "packaging/omarchy/install.sh")]
        try:
            subprocess.run(command, check=True, capture_output=True, timeout=10)
            result = f.home / "actual-installer-result"
            for _ in range(150):
                # Reap the old fixture once the installer stops it, so qs list sees
                # it disappear instead of mistaking a zombie for a running overlay.
                old.poll()
                if result.exists():
                    break
                time.sleep(0.1)
            self.assertTrue(result.exists(), (f.home / "actual-installer.log").read_text())
            self.assertEqual(result.read_text().strip(), "0", (f.home / "actual-installer.log").read_text())
            pidfile = f.home / "new-overlay-pid"
            for _ in range(50):
                if pidfile.exists():
                    break
                time.sleep(0.1)
            self.assertTrue(pidfile.exists(), (f.home / "actual-installer.log").read_text())
            pid = int(pidfile.read_text())
            cgroup = Path(f"/proc/{pid}/cgroup").read_text()
            import re
            match = re.search(r"(codepet-overlay-restored-[^/]+\.service)", cgroup)
            self.assertIsNotNone(match, cgroup)
            child_unit = match[1]
            self.assertNotIn(outer, cgroup)
            subprocess.run(["systemctl", "--user", "stop", outer], capture_output=True, timeout=10)
            time.sleep(0.2)
            os.kill(pid, 0)
            active = subprocess.check_output(["systemctl", "--user", "show", child_unit, "--property=ActiveState", "--value"], timeout=5).decode().strip()
            self.assertEqual(active, "active")
            print("native lifecycle proof: " + json.dumps({"updater_unit_stopped": outer, "restored_unit": child_unit, "restored_state_after_updater_stop": active, "restored_pid": pid, "restored_cgroup": cgroup.strip(), "fixture": "real installer + user systemd + non-graphical qs stand-in"}))
        finally:
            subprocess.run(["systemctl", "--user", "stop", outer], capture_output=True, timeout=10)
            if child_unit:
                subprocess.run(["systemctl", "--user", "stop", child_unit], capture_output=True, timeout=10)
            elif (f.home / "new-overlay-pid").exists():
                # Recover the exact fixture-owned unit if a preceding assertion failed.
                pid = int((f.home / "new-overlay-pid").read_text())
                if Path(f"/proc/{pid}/cgroup").exists():
                    import re
                    found = re.search(r"(codepet-overlay-restored-[^/]+\.service)", Path(f"/proc/{pid}/cgroup").read_text())
                    if found:
                        subprocess.run(["systemctl", "--user", "stop", found[1]], capture_output=True, timeout=10)
            if old.poll() is None:
                old.terminate()
            old.wait(timeout=5)

    @unittest.skipUnless(os.environ.get("CODEPET_UPDATE_TEST_BINARY"), "optional qualified real backend not supplied")
    def test_real_backend_sqlite_preservation(self):
        binary = Path(os.environ["CODEPET_UPDATE_TEST_BINARY"])
        self.assertEqual(subprocess.check_output([str(binary), "--version"]).decode().strip(), f"codepet {VERSION}")
        old = (self.tree / "bin/codepet").read_bytes()
        try:
            shutil.copy2(binary, self.tree / "bin/codepet")
            f = self.fixture(archive=self.make_archive())
        finally:
            (self.tree / "bin/codepet").write_bytes(old)
        env = f.env | {"PATH": str(f.stubs) + ":/usr/bin:/bin"}
        subprocess.run([str(binary), "migrate"], env=env, check=True, capture_output=True)
        database = f.updater.state / "codepet.sqlite3"
        frame = {"v": 1, "id": "updater-fixture", "method": "state.get", "params": {}}
        adopt = {"v": 1, "id": "updater-adopt", "method": "pet.adopt", "params": {"species": "penguin", "mode": "gentle", "pace": "steady", "acknowledge_rules": True}}
        subprocess.run([str(binary), "rpc"], env=env, input=json.dumps(adopt) + "\n", text=True, check=True, capture_output=True)
        state = json.loads(subprocess.run([str(binary), "rpc"], env=env, input=json.dumps(frame) + "\n", text=True, check=True, capture_output=True).stdout)["result"]
        life_id = state["life"]["life_id"]
        self.assertGreater(state["life"]["adopted_at_ms"], 0)
        with contextlib.closing(sqlite3.connect(database)) as connection:
            # Seed a legitimate immutable five-XP ledger row in this isolated fixture;
            # the updater must preserve both it and the exact life birth timestamp.
            row = connection.execute("INSERT INTO activity_turns (session_key, turn_key, client, life_id, earned_at_ms, given_at_ms) VALUES (?, ?, 'codex', ?, 5, 9)", ("a" * 32, "b" * 32, life_id)).lastrowid
            connection.execute("INSERT INTO activity_xp (life_id, turn_row_id, amount, granted_at_ms) VALUES (?, ?, 5, 9)", (life_id, row))
            connection.commit()
            birth = connection.execute("SELECT adopted_at_ms FROM lives WHERE life_id = ?", (life_id,)).fetchone()[0]
            before = list(connection.iterdump())
        job = f.updater.install()
        self.assertEqual(f.updater.worker(job["job"])["state"], "succeeded")
        with contextlib.closing(sqlite3.connect(database)) as connection:
            self.assertEqual(list(connection.iterdump()), before)
            self.assertEqual(connection.execute("SELECT adopted_at_ms FROM lives WHERE life_id = ?", (life_id,)).fetchone()[0], birth)
            self.assertEqual(connection.execute("SELECT SUM(amount) FROM activity_xp WHERE life_id = ?", (life_id,)).fetchone()[0], 5)
        self.assertEqual(subprocess.check_output([str(f.bin / "codepet"), "--version"]).decode().strip(), f"codepet {VERSION}")

    @unittest.skipUnless(os.environ.get("CODEPET_UPDATE_TEST_ARCHIVE") and os.environ.get("CODEPET_UPDATE_TEST_PREVIOUS_BINARY"), "optional candidate archive and previous release binary not supplied")
    def test_previous_release_upgrade_preserves_companion_and_backup(self):
        previous = Path(os.environ["CODEPET_UPDATE_TEST_PREVIOUS_BINARY"])
        previous_version = subprocess.check_output([str(previous), "--version"], text=True, timeout=5).strip().removeprefix("codepet ")
        self.assertLess(update.version(previous_version), update.version(VERSION))
        f = self.fixture()
        shutil.copy2(previous, f.bin / "codepet")

        def rpc(method, params):
            frame = {"v": 1, "id": "upgrade-" + method, "method": method, "params": params}
            result = subprocess.run([str(f.bin / "codepet"), "rpc"], env=f.env,
                                    input=json.dumps(frame) + "\n", text=True, capture_output=True, timeout=15)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            return json.loads(result.stdout)["result"]

        subprocess.run([str(previous), "migrate"], env=f.env, check=True, capture_output=True, timeout=15)
        life = rpc("pet.adopt", {"species": "penguin", "mode": "gentle", "pace": "steady", "acknowledge_rules": True})["life"]
        rpc("pet.rename", {"life_id": life["life_id"], "name": "Upgrade Fixture"})
        database = f.updater.state / "codepet.sqlite3"
        tables = ("lives", "life_vitals", "pet_care", "activity_turns", "activity_xp", "client_hooks", "life_events")

        def rows(connection, table, names):
            projection = ','.join('"' + name + '"' for name in names)
            return connection.execute('SELECT ' + projection + ' FROM ' + table + ' ORDER BY ' + projection).fetchall()

        with contextlib.closing(sqlite3.connect(database)) as connection:
            old_schema = connection.execute("PRAGMA user_version").fetchone()[0]
            # Only synthetic records in this fixture: no real coding client is run.
            row = connection.execute("INSERT INTO activity_turns (session_key, turn_key, client, life_id, earned_at_ms, given_at_ms) VALUES (?, ?, 'codex', ?, 5, 9)", ("a" * 32, "b" * 32, life["life_id"])).lastrowid
            connection.execute("INSERT INTO activity_xp (life_id, turn_row_id, amount, granted_at_ms) VALUES (?, ?, 5, 9)", (life["life_id"], row))
            connection.execute("INSERT OR REPLACE INTO client_hooks (client, installed, connected, installed_version, installed_at_ms, last_outcome) VALUES ('codex', 1, 1, 'fixture', 123, 'connected')")
            connection.commit()
            columns = {table: [r[1] for r in connection.execute(f"PRAGMA table_info({table})")] for table in tables}
            before = {table: rows(connection, table, columns[table]) for table in tables}

        job = f.updater.install()
        self.assertEqual(f.updater.worker(job["job"])["state"], "succeeded")
        with contextlib.closing(sqlite3.connect(database)) as connection:
            new_schema = connection.execute("PRAGMA user_version").fetchone()[0]
            self.assertEqual(new_schema, 6)
            for table in tables:
                self.assertEqual(rows(connection, table, columns[table]), before[table], table)
            self.assertEqual(connection.execute("SELECT care_tempo FROM lives").fetchone()[0], "steady")
            self.assertEqual(connection.execute("SELECT SUM(amount) FROM activity_xp").fetchone()[0], 5)

        backups = list(f.updater.state.glob("*.bak-*"))
        if old_schema < new_schema:
            self.assertEqual(len(backups), 1)
            with contextlib.closing(sqlite3.connect(backups[0])) as connection:
                self.assertEqual(connection.execute("PRAGMA user_version").fetchone()[0], old_schema)
                for table in tables:
                    self.assertEqual(rows(connection, table, columns[table]), before[table], table)
            original = database.read_bytes()
            refused = subprocess.run([str(previous), "migrate"], env=f.env, capture_output=True, timeout=15)
            self.assertNotEqual(refused.returncode, 0)
            self.assertIn(b"SCHEMA_UNSUPPORTED", refused.stderr)
            self.assertEqual(database.read_bytes(), original)
        else:
            self.assertEqual(backups, [])
        self.assertEqual(subprocess.check_output([str(f.bin / "codepet"), "--version"], text=True, timeout=5).strip(), f"codepet {VERSION}")


if __name__ == "__main__":
    unittest.main(verbosity=2)
