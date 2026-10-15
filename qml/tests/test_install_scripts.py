#!/usr/bin/env python3
"""Install/uninstall and measurement tooling (spec §15, §17.8, §17.9 item 6).

packaging/omarchy/install.sh and uninstall.sh run against a private temporary HOME with
stub `cargo`, `systemd-run`, `omarchy` and `qs` commands that only log their arguments,
so nothing here builds, touches the live desktop, the Omarchy shell or the real
~/.config, ~/.local or ~/.cache. A sentinel pet state directory proves the scripts never
touch $XDG_STATE_HOME/codepet. tools/measure-overlay.sh is run read-only against a
`sleep` child.

Run: python3 qml/tests/test_install_scripts.py
"""
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
INSTALL = REPO / "packaging/omarchy/install.sh"
UNINSTALL = REPO / "packaging/omarchy/uninstall.sh"
MEASURE = REPO / "tools/measure-overlay.sh"
PLUGIN = REPO / "omarchy-plugin"
VERSION = "0.2.0"

STUB = """#!/bin/sh
# Test stub: logs its name and arguments, never does real work.
printf '%s' "$(basename "$0")" >> "$HOME/calls.log"
for a in "$@"; do printf ' %s' "$a" >> "$HOME/calls.log"; done
echo >> "$HOME/calls.log"
"""
# Like the real shell, the stub learns of a new plugin folder only when a rescan has been
# requested ($HOME/rescanned): until then `plugin list` omits it and `plugin enable` fails
# as "not known", which is the race a first install must wait out. $HOME/shell-down makes
# every shell call fail (no running shell); $HOME/enable-fails refuses the enable itself.
OMARCHY = STUB + """
known() { [ -e "$HOME/rescanned" ] && [ -f "$XDG_CONFIG_HOME/omarchy/plugins/$1/manifest.json" ]; }
case "$1 $2" in
  "plugin validate")
    [ -f "$3/manifest.json" ] || { echo "no manifest" >&2; exit 1; }
    if [ -n "$(find "$3" -type l)" ]; then echo "symlink" >&2; exit 1; fi
    exit 0 ;;
  "shell shell")
    [ -e "$HOME/shell-down" ] && { echo "shell is not running" >&2; exit 1; }
    [ "$3" = rescanPlugins ] && : > "$HOME/rescanned"
    exit 0 ;;
  "plugin list")
    [ -e "$HOME/shell-down" ] && exit 1
    printf '['
    if known greyforge.codepet; then printf '{"id":"greyforge.codepet","enabled":false}'; fi
    echo ']'
    exit 0 ;;
  "plugin enable")
    [ -e "$HOME/shell-down" ] && exit 1
    known "$3" || { echo "omarchy-plugin-enable: plugin '$3' is not known; run: omarchy-shell shell rescanPlugins" >&2; exit 1; }
    [ -e "$HOME/enable-fails" ] && { echo "omarchy-plugin-enable: refused" >&2; exit 1; }
    mkdir -p "$XDG_CONFIG_HOME/omarchy"
    printf '{"bar":{"layout":{"right":[{"id":"%s"},{"id":"omarchy.tray"}]}}}\\n' "$3" > "$XDG_CONFIG_HOME/omarchy/shell.json"
    exit 0 ;;
  "plugin disable") exit 0 ;;
esac
exit 3
"""
QS = STUB + """
case "$1" in
  list) echo "No running instances." ; exit 0 ;;
  *) exit 1 ;;
esac
"""
FAKE_CODEPET = """#!/bin/sh
[ "$1" = --version ] && { echo "codepet %s"; exit 0; }
exit 9
""" % VERSION


def tree(root):
    """{relative path: (kind, size, mtime_ns)} for everything under root."""
    out = {}
    for dirpath, dirnames, filenames in os.walk(root):
        for name in dirnames + filenames:
            p = Path(dirpath) / name
            st = p.lstat()
            kind = "link" if stat.S_ISLNK(st.st_mode) else "dir" if stat.S_ISDIR(st.st_mode) else "file"
            out[str(p.relative_to(root))] = (kind, st.st_size if kind == "file" else 0,
                                             st.st_mtime_ns if kind != "dir" else 0)
    return out


class Env:
    def __init__(self, test):
        self.tmp = tempfile.TemporaryDirectory(prefix="codepet-install-")
        test.addCleanup(self.tmp.cleanup)
        root = Path(self.tmp.name)
        self.home = root / "home"
        self.bin = root / "stubs"
        for d in (self.home, self.bin, root / "tmp"):
            d.mkdir()
        for name, body in (("cargo", STUB), ("systemd-run", STUB), ("omarchy", OMARCHY), ("qs", QS)):
            f = self.bin / name
            f.write_text(body)
            f.chmod(0o755)
        self.fake = root / "codepet-built"
        self.fake.write_text(FAKE_CODEPET)
        self.fake.chmod(0o755)
        self.config = self.home / ".config"
        self.data = self.home / ".local/share"
        self.cache = self.home / ".cache"
        self.state = self.home / ".local/state"
        # The pet's data: must never be touched.
        pet = self.state / "codepet"
        pet.mkdir(parents=True)
        (pet / "codepet.sqlite3").write_bytes(b"pretend database")
        (pet / "bar-state.json").write_text("{}")
        self.env = {
            "PATH": f"{self.bin}:/usr/bin:/bin", "HOME": str(self.home), "TMPDIR": str(root / "tmp"),
            "XDG_CONFIG_HOME": str(self.config), "XDG_DATA_HOME": str(self.data),
            "XDG_CACHE_HOME": str(self.cache), "XDG_STATE_HOME": str(self.state), "LC_ALL": "C",
            # Never the real tools, whatever else is on PATH.
            "CODEPET_OMARCHY": str(self.bin / "omarchy"), "CODEPET_QS": str(self.bin / "qs"),
        }

    def run(self, script, *args, **extra):
        env = dict(self.env, **extra)
        return subprocess.run([str(script), *args], env=env, capture_output=True, text=True, timeout=120)

    def calls(self):
        p = self.home / "calls.log"
        return p.read_text().splitlines() if p.exists() else []

    def state_tree(self):
        return tree(self.state)


class InstallDryRun(unittest.TestCase):
    def test_dry_run_lists_regular_staged_files_and_changes_nothing(self):
        e = Env(self)
        before_home, before_state = tree(e.home), e.state_tree()
        r = e.run(INSTALL, "--dry-run")
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        out = r.stdout
        plugin = sorted(l.split(" ", 1)[1] for l in out.splitlines() if l.startswith("plugin "))
        expected = sorted(f"greyforge.codepet/{p.name}" for p in PLUGIN.iterdir() if p.is_file())
        expected = sorted(expected + ["greyforge.codepet/LifeText.js"])
        self.assertEqual(plugin, expected)
        runtime = {l.split(" ", 1)[1] for l in out.splitlines() if l.startswith("runtime ")}
        for must in ("qml/shell.qml", "qml/shared/PetCatalog.js", "qml/shared/PetRegistry.js",
                     "qml/omarchy/BarStateWatcher.qml", "qml/assets/pets/registry.json",
                     "qml/assets/pets/cat/s1/b00.webp", "qml/assets/pets/_memorial.webp"):
            self.assertIn(must, runtime)
        self.assertFalse(any(p.startswith(("qml/tests", "qml/standalone")) for p in runtime))
        # Every listed path is a regular file in the repository (none is a symlink).
        for rel in runtime:
            p = REPO / rel
            self.assertTrue(p.is_file() and not p.is_symlink(), rel)
        self.assertIn("staged files: %d, symlinks: 0" % (len(runtime) + len(plugin)), out)
        self.assertIn("would enable: omarchy plugin enable greyforge.codepet --section right --before omarchy.tray", out)
        self.assertIn(f"state: {e.state}/codepet is never touched", out)
        # Nothing ran and nothing changed: no build, no omarchy/qs, HOME and state untouched.
        self.assertEqual(e.calls(), [])
        self.assertEqual(tree(e.home), before_home)
        self.assertEqual(e.state_tree(), before_state)
        self.assertEqual(list((Path(e.env["TMPDIR"])).iterdir()), [], "the dry-run stage is removed")

    def test_dry_run_without_plugin(self):
        e = Env(self)
        r = e.run(INSTALL, "--dry-run", "--no-plugin")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertNotIn("\nplugin ", r.stdout)
        self.assertNotIn("omarchy plugin", r.stdout)

    def test_usage(self):
        e = Env(self)
        self.assertEqual(e.run(INSTALL, "--bogus").returncode, 2)
        self.assertEqual(e.run(UNINSTALL, "--bogus").returncode, 2)

    def test_a_symlink_in_the_sources_is_refused(self):
        with tempfile.TemporaryDirectory(prefix="codepet-repo-") as tmp:
            repo = Path(tmp)
            for rel in ("Cargo.toml", "packaging", "omarchy-plugin", "qml/shell.qml", "qml/shared",
                        "qml/omarchy", "qml/assets"):
                src, dst = REPO / rel, repo / rel
                dst.parent.mkdir(parents=True, exist_ok=True)
                (shutil.copytree if src.is_dir() else shutil.copy2)(src, dst)
            (repo / "omarchy-plugin/evil.qml").symlink_to("/etc/passwd")
            e = Env(self)
            r = e.run(repo / "packaging/omarchy/install.sh", "--dry-run")
            self.assertEqual(r.returncode, 1, r.stdout)
            self.assertIn("refusing to install a symlink", r.stderr)


class InstallForReal(unittest.TestCase):
    """A full install, an update and an uninstall into the temporary HOME."""

    def test_install_update_uninstall(self):
        e = Env(self)
        state0 = e.state_tree()
        r = e.run(INSTALL, CODEPET_BIN_SOURCE=str(e.fake))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        bin_dir = e.home / ".local/bin"
        for name in ("codepet", "codepet-overlay"):
            p = bin_dir / name
            self.assertTrue(p.is_file() and not p.is_symlink(), name)
            self.assertEqual(stat.S_IMODE(p.stat().st_mode), 0o755, name)
        self.assertEqual((bin_dir / "codepet-overlay").read_bytes(),
                         (REPO / "packaging/omarchy/codepet-overlay").read_bytes())
        self.assertEqual(list(bin_dir.glob("*.tmp-*")), [])
        runtime = e.data / "codepet"
        current = runtime / "current"
        self.assertTrue(current.is_symlink())
        self.assertEqual(os.readlink(current), f"releases/{VERSION}")
        qml = runtime / "releases" / VERSION / "qml"
        self.assertEqual((qml / "shell.qml").read_bytes(), (REPO / "qml/shell.qml").read_bytes())
        self.assertTrue((qml / "assets/pets/registry.json").is_file())
        self.assertEqual([p for p in qml.rglob("*") if p.is_symlink()], [])
        plugin = e.config / "omarchy/plugins/greyforge.codepet"
        self.assertEqual([p for p in plugin.rglob("*") if p.is_symlink() or not (p.is_file() or p.is_dir())], [])
        for p in PLUGIN.iterdir():
            self.assertEqual((plugin / p.name).read_bytes(), p.read_bytes(), p.name)
        self.assertEqual((plugin / "LifeText.js").read_bytes(), (REPO / "qml/shared/LifeText.js").read_bytes())
        calls = e.calls()
        self.assertFalse(any(c.startswith(("cargo", "systemd-run")) for c in calls), "prebuilt: no build")
        self.assertTrue(any(c.startswith("omarchy plugin validate ") and "/stage-" in c for c in calls), calls)
        enable = "omarchy plugin enable greyforge.codepet --section right --before omarchy.tray"
        self.assertIn(enable, calls)
        # The shell is asked to rescan and has listed the plugin before the enable is sent.
        rescan = calls.index("omarchy shell shell rescanPlugins")
        listed = calls.index("omarchy plugin list --json")
        self.assertLess(rescan, listed)
        self.assertLess(listed, calls.index(enable))
        self.assertIn("plugin: enabled in the right section", r.stdout)
        self.assertEqual(e.state_tree(), state0, "the pet state is never touched")
        self.assertEqual([p.name for p in (e.cache / "codepet").iterdir()], [], "the stage is moved, not left")

        # Update: one swap of the plugin, the previous copy kept, enable not repeated.
        (plugin / "marker").write_text("old copy")
        r = e.run(INSTALL, CODEPET_BIN_SOURCE=str(e.fake))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertFalse((plugin / "marker").exists())
        backups = list((e.cache / "codepet").glob("plugin-backup-*"))
        self.assertEqual(len(backups), 1)
        self.assertEqual((backups[0] / "marker").read_text(), "old copy")
        self.assertEqual(len(list((e.cache / "codepet").glob("release-backup-*"))), 1)
        self.assertEqual(os.readlink(current), f"releases/{VERSION}")
        enables = [c for c in e.calls() if c.startswith("omarchy plugin enable")]
        self.assertEqual(len(enables), 1, "already listed in shell.json: not enabled twice")
        self.assertIn("already listed", r.stdout)
        self.assertEqual(e.state_tree(), state0)

        # A binary reporting another version is refused (and replaced atomically).
        wrong = e.home / "wrong"
        wrong.write_text('#!/bin/sh\necho "codepet 9.9.9"\n')
        wrong.chmod(0o755)
        r = e.run(INSTALL, "--no-plugin", CODEPET_BIN_SOURCE=str(wrong))
        self.assertEqual(r.returncode, 1)
        self.assertIn("expected 'codepet 0.2.0'", r.stderr)

        # Uninstall dry run: lists the moves, changes nothing.
        def home_tree():  # the stubs' own log aside
            return {k: v for k, v in tree(e.home).items() if k != "calls.log"}
        before = home_tree()
        r = e.run(UNINSTALL, "--dry-run")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("would disable: omarchy plugin disable greyforge.codepet", r.stdout)
        self.assertEqual(sum(1 for l in r.stdout.splitlines() if l.startswith("would move: ")), 3)
        self.assertEqual(home_tree(), before)
        self.assertNotIn("omarchy plugin disable greyforge.codepet", e.calls())
        # Uninstall: disable, move plugin and runtime aside, keep state and binaries.
        r = e.run(UNINSTALL)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("omarchy plugin disable greyforge.codepet", e.calls())
        self.assertFalse(plugin.exists())
        self.assertFalse(current.exists() or current.is_symlink())
        self.assertFalse((runtime / "releases").exists())
        moved = list((e.cache / "codepet").glob("uninstall-*"))
        self.assertEqual(len(moved), 1)
        self.assertTrue((moved[0] / "plugin-greyforge.codepet/manifest.json").is_file())
        self.assertTrue((moved[0] / "releases" / VERSION / "qml/shell.qml").is_file())
        self.assertTrue((bin_dir / "codepet").is_file())
        self.assertEqual(e.state_tree(), state0, "uninstall never touches the pet state")

    def test_without_a_running_shell_the_enable_command_is_printed(self):
        e = Env(self)
        (e.home / "shell-down").write_text("")
        r = e.run(INSTALL, CODEPET_BIN_SOURCE=str(e.fake))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertTrue((e.config / "omarchy/plugins/greyforge.codepet/manifest.json").is_file())
        self.assertFalse(any(c.startswith("omarchy plugin enable") for c in e.calls()), e.calls())
        self.assertIn("has not discovered greyforge.codepet yet", r.stdout)
        self.assertIn("  omarchy plugin enable greyforge.codepet --section right --before omarchy.tray", r.stdout)
        self.assertIn("state: ", r.stdout.splitlines()[-1], "the install runs to the end")

    def test_a_refused_enable_prints_the_command_and_finishes(self):
        e = Env(self)
        (e.home / "enable-fails").write_text("")
        r = e.run(INSTALL, CODEPET_BIN_SOURCE=str(e.fake))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("plugin: not enabled; enable it with:", r.stdout)
        self.assertIn("state: ", r.stdout.splitlines()[-1])

    def test_no_plugin_install_needs_no_omarchy(self):
        e = Env(self)
        (e.bin / "omarchy").unlink()  # CODEPET_OMARCHY now names a missing command
        r = e.run(INSTALL, "--no-plugin", CODEPET_BIN_SOURCE=str(e.fake))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertFalse((e.config / "omarchy/plugins").exists())
        r = e.run(INSTALL, CODEPET_BIN_SOURCE=str(e.fake))
        self.assertEqual(r.returncode, 1)
        self.assertIn("rerun with --no-plugin", r.stderr)


class Static(unittest.TestCase):
    def test_scripts_are_executable_bash_without_eval(self):
        for p in (INSTALL, UNINSTALL, MEASURE):
            text = p.read_text()
            self.assertTrue(text.startswith("#!/bin/bash\n"), p.name)
            self.assertIn("set -euo pipefail", text, p.name)
            self.assertNotRegex(text, r"(?m)^[^#]*\beval\b", p.name)
            self.assertTrue(os.access(p, os.X_OK), p.name)
            r = subprocess.run(["bash", "-n", str(p)], capture_output=True, text=True)
            self.assertEqual(r.returncode, 0, r.stderr)

    def test_state_dir_is_only_ever_printed(self):
        for p in (INSTALL, UNINSTALL):
            for line in p.read_text().splitlines():
                if "STATE_HOME" in line and not line.lstrip().startswith("#"):
                    self.assertTrue("STATE_HOME=$(xdg" in line or line.lstrip().startswith("echo"), line)

    @unittest.skipUnless(shutil.which("shellcheck"), "shellcheck unavailable")
    def test_shellcheck(self):
        r = subprocess.run(["shellcheck", str(INSTALL), str(UNINSTALL), str(MEASURE)],
                           capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stdout)


class Measure(unittest.TestCase):
    def test_reads_proc_and_prints_one_row_per_pid(self):
        child = subprocess.Popen(["sleep", "30"])
        self.addCleanup(child.wait)
        self.addCleanup(child.kill)
        r = subprocess.run([str(MEASURE), "--seconds", "1", "--label", "idle", str(child.pid)],
                           capture_output=True, text=True, timeout=30)
        self.assertEqual(r.returncode, 0, r.stderr)
        rows = [l for l in r.stdout.splitlines() if l.startswith(f"| {child.pid} |")]
        self.assertEqual(len(rows), 1, r.stdout)
        cells = [c.strip() for c in rows[0].strip("|").split("|")]
        self.assertEqual(cells[1], "sleep 30")
        self.assertTrue(cells[2].isdigit() and int(cells[2]) > 0, cells)  # PSS kB
        self.assertTrue(cells[3].isdigit() and cells[5].isdigit(), cells)
        self.assertIn("Window: 1 s; idle", r.stdout)
        self.assertIn("| pid | command | PSS (kB) |", r.stdout)

    def test_rejects_bad_input(self):
        for args in (["abc"], ["--seconds", "0", "1"], ["--seconds", "x", "1"], []):
            r = subprocess.run([str(MEASURE), *args], capture_output=True, text=True, timeout=10)
            self.assertEqual(r.returncode, 2, args)
        r = subprocess.run([str(MEASURE), "--seconds", "1", "999999999"], capture_output=True, text=True)
        self.assertEqual(r.returncode, 1)


if __name__ == "__main__":
    unittest.main(verbosity=2)
