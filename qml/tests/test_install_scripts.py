#!/usr/bin/env python3
"""Install/uninstall and measurement tooling (spec §15, §17.8, §17.9 item 6; v0.3:
SPEC-v0.3 §5.1, §6.6, §7.3).

packaging/omarchy/install.sh and uninstall.sh run against a private temporary HOME with
stub `cargo`, `systemd-run`, `omarchy` and `qs` commands that only log their arguments,
and a stub `codepet` that logs `migrate`, `clients-refresh` and `disconnect` without
touching anything, so nothing here builds, touches the live desktop, the Omarchy shell,
any coding app or the real ~/.config, ~/.local or ~/.cache. A sentinel pet state
directory proves the scripts themselves never write $XDG_STATE_HOME/codepet (only the
`codepet` commands they run may). tools/measure-overlay.sh is run read-only against a
`sleep` child.

The v0.3 installer ships the 0.3 bar plugin and refuses a tree whose version is not
0.3.x; until the release bumps the workspace version, the tests install from a copy of
the tree whose Cargo.toml says 0.3.0 (VERSION below).

Covered for v0.3: the upgrade sequence (overlay and 0.2 supervisor stopped by exact pid,
swap, rescan, migrate, refresh, overlay started again), the icon copy, the 0.3.x gate
(checked before anything is replaced), the prune exclusion of
~/.local/share/codepet/{bin,integrations,staging} and the uninstall tombstone.

Run: python3 qml/tests/test_install_scripts.py
"""
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import threading
import time
import tomllib
import unittest
from pathlib import Path

REAL_REPO = Path(__file__).resolve().parents[2]
REAL_VERSION = tomllib.loads((REAL_REPO / "Cargo.toml").read_text())["workspace"]["package"]["version"]
MEASURE = REAL_REPO / "tools/measure-overlay.sh"
VERSION = REAL_VERSION if REAL_VERSION.startswith("0.3.") else "0.3.0"
TOMBSTONE = (REAL_REPO / "docs/v0.3/contracts/shim-tombstone.sh").read_bytes()
SOURCES = ("Cargo.toml", "packaging", "omarchy-plugin", "qml/shell.qml", "qml/shared", "qml/omarchy",
           "qml/assets")


def copy_tree(dst, version=VERSION, sources=SOURCES):
    """A copy of the shipped sources whose workspace version is `version`."""
    for rel in sources:
        src, out = REAL_REPO / rel, dst / rel
        out.parent.mkdir(parents=True, exist_ok=True)
        (shutil.copytree if src.is_dir() else shutil.copy2)(src, out)
    if (dst / "Cargo.toml").exists():
        text = (dst / "Cargo.toml").read_text()
        text = text.replace(f'version = "{REAL_VERSION}"', f'version = "{version}"', 1)
        (dst / "Cargo.toml").write_text(text)


_REPO_TMP = tempfile.TemporaryDirectory(prefix="codepet-repo-v03-")
REPO = Path(_REPO_TMP.name)
copy_tree(REPO)
INSTALL = REPO / "packaging/omarchy/install.sh"
UNINSTALL = REPO / "packaging/omarchy/uninstall.sh"
PLUGIN = REPO / "omarchy-plugin"

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
    [ "$3" = --help ] && { [ -e "$HOME/no-plugin-api" ] && exit 1; exit 0; }
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
  "plugin disable")
    [ -e "$HOME/disable-fails" ] && { echo "omarchy-plugin-disable: plugin '$3' is not known" >&2; exit 1; }
    exit 0 ;;
esac
exit 3
"""
# $HOME/overlays lists running overlays as "<pid> <config path>" lines; `-n -d -p` (a
# launch) creates $HOME/launched, after which ping answers like a started overlay.
QS = STUB + """
case "$1" in
  list)
    if [ -s "$HOME/overlays" ]; then
      echo "["
      while read -r pid path; do
        kill -0 "$pid" 2>/dev/null || continue
        printf '  {\\n    "config_path": "%s",\\n    "pid": %s\\n  },\\n' "$path" "$pid"
      done < "$HOME/overlays"
      echo "]"
    else
      echo "No running instances."
    fi
    exit 0 ;;
  -n) : > "$HOME/launched"; exit 0 ;;
  ipc) [ -e "$HOME/launched" ] && [ "$6" = codepet ] && [ "$7" = ping ] && { echo "codepet-overlay shown"; exit 0; }
       [ -e "$HOME/launched" ] && exit 0
       exit 1 ;;
  *) exit 1 ;;
esac
"""
# The stub codepet: reports the version, logs every other call to $HOME/calls.log and
# never touches state. `rpc` sleeps (a lingering 0.2 task supervisor); $HOME/migrate-fails
# fails the migration; $HOME/disconnect-fails-<app> fails that disconnect.
FAKE_CODEPET = STUB + """
case "$1" in
  --version) echo "codepet %s" ;;
  migrate) [ -e "$HOME/migrate-fails" ] && { echo "SCHEMA_UNSUPPORTED: database schema 9" >&2; exit 1; }
           echo "CodePet data created (schema 5)." ;;
  clients-refresh) ;;
  disconnect) [ -e "$HOME/disconnect-fails-$2" ] && { echo "$2: couldn't stop watching" >&2; exit 1; }
              echo "$2: stopped watching" ;;
  rpc) trap 'kill $! 2>/dev/null; exit 0' TERM; sleep 30 & wait ;;
  *) exit 9 ;;
esac
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
        expected = sorted(f"greyforge.codepet/{p.relative_to(PLUGIN).as_posix()}"
                          for p in PLUGIN.rglob("*") if p.is_file())
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
        self.assertIn(f"would run: codepet migrate (the pet's data in {e.state}/codepet", out)
        self.assertIn("would run: codepet clients-refresh", out)
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
            copy_tree(repo)
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
        for p in (q for q in PLUGIN.rglob("*") if q.is_file()):
            rel = p.relative_to(PLUGIN)
            self.assertEqual((plugin / rel).read_bytes(), p.read_bytes(), str(rel))
        self.assertEqual((plugin / "LifeText.js").read_bytes(), (REPO / "qml/shared/LifeText.js").read_bytes())
        calls = e.calls()
        self.assertFalse(any(c.startswith(("cargo", "systemd-run")) for c in calls), "prebuilt: no build")
        # Staged beside plugins/ (same filesystem, outside the watched folder).
        self.assertTrue(any(c.startswith(f"omarchy plugin validate {e.config}/omarchy/.codepet-stage-")
                            for c in calls), calls)
        self.assertEqual([p.name for p in (e.config / "omarchy").iterdir() if p.name.startswith(".codepet-stage")],
                         [], "the stage directory is removed")
        enable = "omarchy plugin enable greyforge.codepet --section right --before omarchy.tray"
        self.assertIn(enable, calls)
        # The shell is asked to rescan and has listed the plugin before the enable is sent.
        rescan = calls.index("omarchy shell shell rescanPlugins")
        listed = calls.index("omarchy plugin list --json")
        self.assertLess(rescan, listed)
        self.assertLess(listed, calls.index(enable))
        self.assertIn("plugin: enabled in the right section", r.stdout)
        # v0.3: the data is migrated after the swap, then connected apps are refreshed.
        self.assertLess(calls.index(enable), calls.index("codepet migrate"))
        self.assertLess(calls.index("codepet migrate"), calls.index("codepet clients-refresh"))
        self.assertIn("data: CodePet data created (schema 5).", r.stdout)
        self.assertEqual(r.stdout.splitlines()[-1], f"done: CodePet {VERSION} is installed")
        self.assertEqual(e.state_tree(), state0, "the script itself never writes the pet state")
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
        # Another reinstall keeps only the newest backup of each kind, not one per run.
        (plugin / "marker").write_text("second copy")
        (qml / "marker").write_text("second runtime")
        r = e.run(INSTALL, CODEPET_BIN_SOURCE=str(e.fake))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        backups = list((e.cache / "codepet").glob("plugin-backup-*"))
        self.assertEqual(len(backups), 1, backups)
        self.assertEqual((backups[0] / "marker").read_text(), "second copy")
        releases = list((e.cache / "codepet").glob("release-backup-*"))
        self.assertEqual(len(releases), 1, releases)
        self.assertEqual((releases[0] / "qml/marker").read_text(), "second runtime")
        self.assertEqual(r.stdout.count("removed older backup: "), 2, r.stdout)
        enables = [c for c in e.calls() if c.startswith("omarchy plugin enable")]
        self.assertEqual(len(enables), 1, "already listed in shell.json: not enabled twice")
        self.assertIn("already listed", r.stdout)
        self.assertEqual(e.state_tree(), state0)

        # A binary reporting another version is refused before anything is replaced:
        # another series (the 0.3.x gate) or another 0.3 release.
        installed = (bin_dir / "codepet").read_bytes()
        n_calls = len(e.calls())
        for reported, why in (("codepet 0.2.1", "needs a codepet 0.3.x"), ("codepet 9.9.9", "needs a codepet 0.3.x"),
                              ("codepet 0.3.99", f"expected 'codepet {VERSION}'")):
            wrong = e.home / "wrong"
            wrong.write_text(f'#!/bin/sh\necho "{reported}"\n')
            wrong.chmod(0o755)
            r = e.run(INSTALL, CODEPET_BIN_SOURCE=str(wrong))
            self.assertEqual(r.returncode, 1, reported)
            self.assertIn(why, r.stderr)
            self.assertIn("nothing was changed", r.stderr)
            self.assertEqual((bin_dir / "codepet").read_bytes(), installed, reported)
        self.assertEqual([c for c in e.calls()[n_calls:] if c != "omarchy plugin validate --help"], [],
                         "only the plugin-API preflight ran: no swap, no qs, no codepet command")

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
        self.assertFalse(any(c.startswith("codepet disconnect") for c in e.calls()), "no shim: nothing to disconnect")

    def test_first_install_prints_next_steps(self):
        e = Env(self)
        r = e.run(INSTALL, CODEPET_BIN_SOURCE=str(e.fake))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("Next: click CodePet on the bar", r.stdout)
        self.assertIn("Desktop pet: codepet-overlay show", r.stdout)
        # An update asks the running shell to reload the plugin.
        r = e.run(INSTALL, CODEPET_BIN_SOURCE=str(e.fake))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("plugin: the running shell was asked to reload it", r.stdout)
        self.assertNotIn("restart shell", r.stdout)
        self.assertNotIn("Next: click", r.stdout)
        # Without a running shell (and no live signature) it prints a pasteable restart line.
        (e.home / "shell-down").write_text("")
        r = e.run(INSTALL, CODEPET_BIN_SOURCE=str(e.fake))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertNotIn("<", r.stdout)
        self.assertIn("  omarchy restart shell", r.stdout)
        r = e.run(INSTALL, CODEPET_BIN_SOURCE=str(e.fake), HYPRLAND_INSTANCE_SIGNATURE="abc_123")
        self.assertIn("  HYPRLAND_INSTANCE_SIGNATURE=abc_123 omarchy restart shell", r.stdout)

    def test_older_release_trees_are_pruned(self):
        e = Env(self)
        releases = e.data / "codepet/releases"
        for old in ("0.1.0", "0.1.5"):
            (releases / old / "qml").mkdir(parents=True)
        (e.data / "codepet/current").symlink_to("releases/0.1.5")
        (releases / ".staging-leftover").mkdir()
        r = e.run(INSTALL, "--no-plugin", CODEPET_BIN_SOURCE=str(e.fake))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertEqual(sorted(p.name for p in releases.iterdir()),
                         sorted([".staging-leftover", "0.1.5", VERSION]))
        self.assertIn(f"removed older release: {releases}/0.1.0", r.stdout)

    def test_uninstall_continues_when_disable_fails(self):
        e = Env(self)
        r = e.run(INSTALL, CODEPET_BIN_SOURCE=str(e.fake))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        state0 = e.state_tree()
        (e.home / "disable-fails").write_text("")
        r = e.run(UNINSTALL)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("warning: could not disable greyforge.codepet", r.stderr)
        self.assertIn("omarchy plugin disable greyforge.codepet", r.stdout)
        self.assertFalse((e.config / "omarchy/plugins/greyforge.codepet").exists())
        self.assertFalse((e.data / "codepet/releases").exists())
        moved = list((e.cache / "codepet").glob("uninstall-*"))
        self.assertEqual(len(moved), 1)
        self.assertTrue((moved[0] / "plugin-greyforge.codepet/manifest.json").is_file())
        self.assertEqual(e.state_tree(), state0)

    def test_uninstall_refuses_while_the_overlay_runs_and_names_quit(self):
        e = Env(self)
        r = e.run(INSTALL, "--no-plugin", CODEPET_BIN_SOURCE=str(e.fake))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        launcher = e.home / ".local/bin/codepet-overlay"
        launcher.write_text("#!/bin/sh\n[ \"$1\" = status ] && [ ! -e \"$HOME/quit\" ] && exit 0\nexit 1\n")
        r = e.run(UNINSTALL)
        self.assertEqual(r.returncode, 1)
        self.assertIn("codepet-overlay quit", r.stderr)
        self.assertNotIn("hide", r.stderr)
        self.assertTrue((e.data / "codepet/current").is_symlink())
        (e.home / "quit").write_text("")  # what `codepet-overlay quit` achieves
        r = e.run(UNINSTALL)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertFalse((e.data / "codepet/current").is_symlink())

    def test_upgrade_stops_the_overlay_and_a_02_supervisor_by_exact_pid(self):
        e = Env(self)
        r = e.run(INSTALL, CODEPET_BIN_SOURCE=str(e.fake))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        runtime = e.data / "codepet"
        # A running overlay (a stand-in process listed by `qs list`), a lingering 0.2
        # `codepet rpc` task supervisor, and an unrelated process with similar words.
        overlay = subprocess.Popen(["sleep", "30"])
        self.addCleanup(overlay.wait)
        self.addCleanup(overlay.kill)
        (e.home / "overlays").write_text(f"{overlay.pid} {runtime}/current/qml/shell.qml\n"
                                         f"1 /elsewhere/shell.qml\n")
        supervisor = subprocess.Popen([str(e.home / ".local/bin/codepet"), "rpc"], env=dict(e.env))
        self.addCleanup(supervisor.wait)
        self.addCleanup(supervisor.kill)
        bystander = subprocess.Popen(["sh", "-c", "sleep 30 # codepet rpc", "codepet", "rpc"])
        self.addCleanup(bystander.wait)
        self.addCleanup(bystander.kill)
        # Reap the stand-ins as soon as they end, as init would for a real overlay.
        for child in (overlay, supervisor):
            threading.Thread(target=child.wait, daemon=True).start()
        time.sleep(0.2)
        old = runtime / "releases/0.2.1/qml"
        old.mkdir(parents=True)
        r = e.run(INSTALL, CODEPET_BIN_SOURCE=str(e.fake))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn(f"stopped the desktop overlay (pid {overlay.pid}", r.stdout)
        self.assertIn(f"stopped a CodePet 0.2 task supervisor (pid {supervisor.pid}", r.stdout)
        self.assertIsNotNone(overlay.wait(timeout=5))
        self.assertIsNotNone(supervisor.wait(timeout=5))
        self.assertIsNone(bystander.poll(), "only exact pids are stopped")
        self.assertNotIn("stopped the desktop overlay (pid 1)", r.stdout)
        # Swap, reload, migrate and refresh happen after the stop; the overlay comes back.
        calls = e.calls()
        self.assertLess(calls.index("codepet migrate"), calls.index("codepet clients-refresh"))
        launch = [c for c in calls if c.startswith("qs -n -d -p")]
        self.assertEqual(len(launch), 1, calls)
        self.assertLess(calls.index("codepet clients-refresh"), calls.index(launch[0]))
        self.assertIn("overlay: started again", r.stdout)
        self.assertFalse(any(c.startswith("qs kill") for c in calls), "no launcher quit, no pattern kill")
        (e.home / "overlays").unlink()
        self.assertFalse(old.exists(), "the stopped overlay no longer pins older trees")

    def test_icon_copies_are_installed_and_must_match(self):
        e = Env(self)
        r = e.run(INSTALL, CODEPET_BIN_SOURCE=str(e.fake))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        repo_icons = REPO / "qml/assets/icons"
        self.assertTrue((repo_icons / "index.json").is_file())
        files = sorted(p.relative_to(repo_icons) for p in repo_icons.rglob("*") if p.is_file())
        self.assertGreater(len(files), 10)
        plugin_icons = e.config / "omarchy/plugins/greyforge.codepet/assets/icons"
        runtime_icons = e.data / f"codepet/releases/{VERSION}/qml/assets/icons"
        for rel in files:
            want = (repo_icons / rel).read_bytes()
            self.assertEqual((plugin_icons / rel).read_bytes(), want, str(rel))
            self.assertEqual((runtime_icons / rel).read_bytes(), want, str(rel))
        with tempfile.TemporaryDirectory(prefix="codepet-repo-") as tmp:
            repo = Path(tmp)
            copy_tree(repo)
            one = next(p for p in (repo / "omarchy-plugin/assets/icons").rglob("*.webp"))
            one.write_bytes(one.read_bytes() + b"x")
            for args in (["--dry-run"], []):
                r = e.run(repo / "packaging/omarchy/install.sh", *args, CODEPET_BIN_SOURCE=str(e.fake))
                self.assertEqual(r.returncode, 1, args)
                self.assertIn("icon copies", r.stderr)

    def test_a_tree_that_is_not_03_is_refused(self):
        with tempfile.TemporaryDirectory(prefix="codepet-repo-") as tmp:
            repo = Path(tmp)
            copy_tree(repo, version="0.2.1")
            e = Env(self)
            r = e.run(repo / "packaging/omarchy/install.sh", "--dry-run")
            self.assertEqual(r.returncode, 1)
            self.assertIn("ships the CodePet 0.3 bar plugin but its version is 0.2.1", r.stderr)
            self.assertEqual(e.calls(), [])

    def test_a_failed_migration_fails_the_install_and_skips_the_refresh(self):
        e = Env(self)
        (e.home / "migrate-fails").write_text("")
        r = e.run(INSTALL, CODEPET_BIN_SOURCE=str(e.fake))
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("codepet migrate failed", r.stderr)
        self.assertIn("SCHEMA_UNSUPPORTED", r.stderr)
        self.assertNotIn("codepet clients-refresh", e.calls())
        self.assertNotIn("done: ", r.stdout)

    def test_pruning_never_touches_the_shim_integrations_or_staging(self):
        e = Env(self)
        runtime = e.data / "codepet"
        keep = {"bin/codepet-hook": b"#!/bin/sh\n# managed by codepet: shim\n",
                "integrations/claude-code/.claude-plugin/marketplace.json": b"{}",
                "staging/.x.partial-1/plugin.yaml": b"x"}
        for rel, data in keep.items():
            (runtime / rel).parent.mkdir(parents=True, exist_ok=True)
            (runtime / rel).write_bytes(data)
        for old in ("0.1.0", "0.2.0", "0.2.1"):
            (runtime / "releases" / old / "qml").mkdir(parents=True)
        (runtime / "current").symlink_to("releases/0.2.1")
        for _ in range(2):
            r = e.run(INSTALL, CODEPET_BIN_SOURCE=str(e.fake))
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertEqual(sorted(p.name for p in (runtime / "releases").iterdir()), [VERSION])
        for rel, data in keep.items():
            self.assertEqual((runtime / rel).read_bytes(), data, rel)
        r = e.run(UNINSTALL)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        for rel in ("integrations/claude-code/.claude-plugin/marketplace.json", "staging/.x.partial-1/plugin.yaml"):
            self.assertEqual((runtime / rel).read_bytes(), keep[rel], rel)
        self.assertEqual((runtime / "bin/codepet-hook").read_bytes(), TOMBSTONE)

    def test_uninstall_disconnects_every_app_and_tombstones_the_shim(self):
        e = Env(self)
        r = e.run(INSTALL, CODEPET_BIN_SOURCE=str(e.fake))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        shim = e.data / "codepet/bin/codepet-hook"
        shim.parent.mkdir(parents=True)
        shim.write_text("#!/bin/sh\n# managed by codepet: observe-only bridge; prints nothing; always exits 0\n")
        shim.chmod(0o755)
        state0 = e.state_tree()
        r = e.run(UNINSTALL, "--dry-run")
        self.assertIn("would stop watching: claude-code codex opencode hermes grok-cli", r.stdout)
        self.assertIn(f"would tombstone: {shim}", r.stdout)
        self.assertNotIn("tombstone", shim.read_text())
        (e.home / "disconnect-fails-hermes").write_text("")
        r = e.run(UNINSTALL)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        disconnects = [c for c in e.calls() if c.startswith("codepet disconnect")]
        self.assertEqual(disconnects, [f"codepet disconnect {a}" for a in
                                       ("claude-code", "codex", "opencode", "hermes", "grok-cli")])
        self.assertLess(e.calls().index(disconnects[-1]), e.calls().index("omarchy plugin disable greyforge.codepet"))
        self.assertEqual(shim.read_bytes(), TOMBSTONE)
        self.assertEqual(stat.S_IMODE(shim.stat().st_mode), 0o755)
        self.assertIn("could not stop watching: hermes", r.stdout)
        self.assertEqual(e.state_tree(), state0, "the script itself never writes the pet state")
        # The tombstone is silent and exits 0 for every client, stdin open or closed.
        for app in ("claude-code", "codex", "opencode", "hermes", "grok-cli"):
            out = subprocess.run([str(shim), app], input=b'{"x":1}', capture_output=True, timeout=5)
            self.assertEqual((out.returncode, out.stdout, out.stderr), (0, b"", b""), app)
            out = subprocess.run(["sh", "-c", f"{shim} {app} >/dev/null 2>&1 || true"], capture_output=True,
                                 stdin=subprocess.DEVNULL, timeout=5)
            self.assertEqual((out.returncode, out.stdout, out.stderr), (0, b"", b""), app)
        # A missing codepet: still tombstoned, every app listed.
        e2 = Env(self)
        shim2 = e2.data / "codepet/bin/codepet-hook"
        shim2.parent.mkdir(parents=True)
        shim2.write_text("#!/bin/sh\nexit 0\n")
        r = e2.run(UNINSTALL)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertEqual(shim2.read_bytes(), TOMBSTONE)
        self.assertIn("could not stop watching: claude-code codex opencode hermes grok-cli", r.stdout)

    def test_without_a_running_shell_the_enable_command_is_printed(self):
        e = Env(self)
        (e.home / "shell-down").write_text("")
        r = e.run(INSTALL, CODEPET_BIN_SOURCE=str(e.fake))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertTrue((e.config / "omarchy/plugins/greyforge.codepet/manifest.json").is_file())
        self.assertFalse(any(c.startswith("omarchy plugin enable") for c in e.calls()), e.calls())
        self.assertIn("has not discovered greyforge.codepet yet", r.stdout)
        self.assertIn("  omarchy plugin enable greyforge.codepet --section right --before omarchy.tray", r.stdout)
        self.assertIn("done: ", r.stdout.splitlines()[-1], "the install runs to the end")

    def test_a_refused_enable_prints_the_command_and_finishes(self):
        e = Env(self)
        (e.home / "enable-fails").write_text("")
        r = e.run(INSTALL, CODEPET_BIN_SOURCE=str(e.fake))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("plugin: not enabled; enable it with:", r.stdout)
        self.assertIn("done: ", r.stdout.splitlines()[-1])

    def test_without_plugin_support_nothing_is_installed(self):
        e = Env(self)
        (e.home / "no-plugin-api").write_text("")
        r = e.run(INSTALL, CODEPET_BIN_SOURCE=str(e.fake))
        self.assertEqual(r.returncode, 1)
        self.assertIn("Omarchy 4 or later is required", r.stderr)
        self.assertFalse((e.home / ".local/bin").exists())
        self.assertFalse((e.data / "codepet").exists())

    def test_no_plugin_install_needs_no_omarchy(self):
        e = Env(self)
        (e.bin / "omarchy").unlink()  # CODEPET_OMARCHY now names a missing command
        r = e.run(INSTALL, "--no-plugin", CODEPET_BIN_SOURCE=str(e.fake))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertFalse((e.config / "omarchy/plugins").exists())
        r = e.run(INSTALL, CODEPET_BIN_SOURCE=str(e.fake))
        self.assertEqual(r.returncode, 1)
        self.assertIn("rerun with --no-plugin", r.stderr)


def archive_tree(root):
    """The release archive layout (tools/release/package.sh): no Cargo.toml or crates/."""
    copy_tree(root, sources=("LICENSE", "README.md", "packaging", "omarchy-plugin", "qml/shell.qml",
                             "qml/shared", "qml/omarchy", "qml/assets"))
    (root / "VERSION").write_text(VERSION + "\n")
    (root / "bin").mkdir()


class InstallFromArchive(unittest.TestCase):
    """An extracted release archive installs its bundled binary and never runs cargo."""

    def test_archive_dry_run_and_install_use_the_bundled_binary(self):
        e = Env(self)
        (e.bin / "cargo").unlink()  # cargo is not needed (and not on PATH) for an archive
        with tempfile.TemporaryDirectory(prefix="codepet-archive-") as tmp:
            root = Path(tmp) / f"codepet-{VERSION}-x86_64-linux"
            archive_tree(root)
            shutil.copy2(e.fake, root / "bin/codepet")
            script = root / "packaging/omarchy/install.sh"
            r = e.run(script, "--dry-run")
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
            self.assertIn(f"CodePet {VERSION} install plan", r.stdout)
            self.assertIn(f"would use prebuilt: {root}/bin/codepet", r.stdout)
            r = e.run(script)
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
            self.assertEqual((e.home / ".local/bin/codepet").read_bytes(), e.fake.read_bytes())
            self.assertEqual(os.readlink(e.data / "codepet/current"), f"releases/{VERSION}")
            self.assertFalse(any(c.startswith(("cargo", "systemd-run")) for c in e.calls()))
            r = e.run(root / "packaging/omarchy/uninstall.sh")
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)

    def test_archive_without_a_binary_or_version_fails_cleanly(self):
        e = Env(self)
        with tempfile.TemporaryDirectory(prefix="codepet-archive-") as tmp:
            root = Path(tmp)
            archive_tree(root)
            r = e.run(root / "packaging/omarchy/install.sh", "--no-plugin")
            self.assertEqual(r.returncode, 1)
            self.assertIn("no codepet binary at", r.stderr)
            (root / "VERSION").unlink()
            r = e.run(root / "packaging/omarchy/install.sh", "--dry-run")
            self.assertEqual(r.returncode, 1)
            self.assertIn("found neither Cargo.toml", r.stderr)


class Static(unittest.TestCase):
    def test_the_tests_install_this_trees_scripts(self):
        for rel in ("packaging/omarchy/install.sh", "packaging/omarchy/uninstall.sh",
                    "packaging/omarchy/codepet-overlay"):
            self.assertEqual((REPO / rel).read_bytes(), (REAL_REPO / rel).read_bytes(), rel)

    def test_the_uninstall_tombstone_is_the_frozen_contract_file(self):
        text = (REAL_REPO / "packaging/omarchy/uninstall.sh").read_text()
        body = text[text.index("TOMBSTONE='") + len("TOMBSTONE='"):]
        body = body[:body.index("'\n")]
        self.assertEqual(body.encode(), TOMBSTONE)

    def test_scripts_are_executable_bash_without_eval(self):
        for p in (REAL_REPO / "packaging/omarchy/install.sh", REAL_REPO / "packaging/omarchy/uninstall.sh", MEASURE):
            text = p.read_text()
            self.assertTrue(text.startswith("#!/bin/bash\n"), p.name)
            self.assertIn("set -euo pipefail", text, p.name)
            self.assertNotRegex(text, r"(?m)^[^#]*\beval\b", p.name)
            self.assertTrue(os.access(p, os.X_OK), p.name)
            r = subprocess.run(["bash", "-n", str(p)], capture_output=True, text=True)
            self.assertEqual(r.returncode, 0, r.stderr)

    def test_state_dir_is_only_ever_printed(self):
        for p in (REAL_REPO / "packaging/omarchy/install.sh", REAL_REPO / "packaging/omarchy/uninstall.sh"):
            for line in p.read_text().splitlines():
                if "STATE_HOME" in line and not line.lstrip().startswith("#"):
                    self.assertTrue("STATE_HOME=$(xdg" in line or line.lstrip().startswith("echo"), line)

    @unittest.skipUnless(shutil.which("shellcheck"), "shellcheck unavailable")
    def test_shellcheck(self):
        r = subprocess.run(["shellcheck", str(REAL_REPO / "packaging/omarchy/install.sh"),
                            str(REAL_REPO / "packaging/omarchy/uninstall.sh"), str(MEASURE)],
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
