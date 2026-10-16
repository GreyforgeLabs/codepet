#!/usr/bin/env python3
"""Test stand-in for the `claude` CLI (Claude Code 2.1.294), offline.

It reproduces what the day-0 gate captured from the real binary against a throwaway
CLAUDE_CONFIG_DIR (tests/fixtures/claude-cli/): `plugin install <name> --marketplace <dir>
--json` records the plugin in plugins/installed_plugins.json and writes enabledPlugins and
extraKnownMarketplaces into settings.json (rewriting the whole file and moving
`permissions` first, as the real CLI does); a second install answers "already
installed"; `plugin uninstall` and `plugin marketplace remove` answer outcome failed with
failureCode not_installed / not_configured when there is nothing to remove, and leave
`extraKnownMarketplaces: {}` behind. It never reads anything outside CLAUDE_CONFIG_DIR
(else $HOME/.claude). Knobs: FAKE_CLAUDE_TAMPER (also edits hooks), FAKE_CLAUDE_FAIL
(install fails), FAKE_CLAUDE_HANG (sleeps past the CLI timeout), $HOME/claude-slow
(install takes 2 s). Every call is logged to
$HOME/claude-calls.log with the environment's variable names.
"""
import json, os, sys, time

home = os.environ.get("HOME", "")
cfg = os.environ.get("CLAUDE_CONFIG_DIR") or os.path.join(home, ".claude")
with open(os.path.join(home, "claude-calls.log"), "a") as log:
    log.write(json.dumps({"argv": sys.argv[1:], "env": sorted(os.environ)}) + "\n")
args = sys.argv[1:]
if os.environ.get("FAKE_CLAUDE_HANG") or os.path.exists(os.path.join(home, "claude-hang")):
    time.sleep(60)
if args == ["--version"]:
    print("2.1.294 (Claude Code)")
    sys.exit(0)
PID = "codepet@codepet-local"
settings_p = os.path.join(cfg, "settings.json")
plugins_dir = os.path.join(cfg, "plugins")
installed_p = os.path.join(plugins_dir, "installed_plugins.json")


def load(p, default):
    try:
        with open(p) as f:
            return json.load(f)
    except FileNotFoundError:
        return default


def save_settings(s):
    order = ["permissions"] + [k for k in s if k != "permissions"]
    out = {k: s[k] for k in order if k in s}
    os.makedirs(cfg, exist_ok=True)
    with open(settings_p + ".tmp", "w") as f:
        f.write(json.dumps(out, indent=2) + "\n")
    os.replace(settings_p + ".tmp", settings_p)


def emit(obj, code=0):
    print(json.dumps(obj, separators=(",", ":")))
    sys.exit(code)


if args[:2] == ["plugin", "install"] and "--marketplace" in args:
    if os.path.exists(os.path.join(home, "claude-slow")):
        time.sleep(2)
    name = args[2]
    mdir = args[args.index("--marketplace") + 1]
    market = load(os.path.join(mdir, ".claude-plugin", "marketplace.json"), None)
    if os.environ.get("FAKE_CLAUDE_FAIL") or os.path.exists(os.path.join(home, "claude-fail")):
        emit({"command": "install", "outcome": "failed", "message": "blocked by policy", "failureCode": "policy"}, 1)
    if not market or market.get("name") != "codepet-local":
        emit({"command": "install", "outcome": "failed", "message": "no marketplace"}, 1)
    pid = f"{name}@{market['name']}"
    inst = load(installed_p, {"version": 2, "plugins": {}})
    s = load(settings_p, {})
    already = pid in inst["plugins"]
    if not already:
        inst["plugins"][pid] = [{"scope": "user", "installPath": os.path.join(plugins_dir, "cache", "codepet-local", "codepet", "0.3.0"), "version": "0.3.0"}]
        os.makedirs(plugins_dir, exist_ok=True)
        with open(installed_p, "w") as f:
            f.write(json.dumps(inst, indent=2))
    # Like the real CLI's "already installed" answer, a plugin the user turned off in
    # /plugin stays off.
    if not (already and s.get("enabledPlugins", {}).get(pid) is False):
        s.setdefault("enabledPlugins", {})[pid] = True
    s.setdefault("extraKnownMarketplaces", {})["codepet-local"] = {"source": {"source": "directory", "path": mdir}}
    if os.environ.get("FAKE_CLAUDE_TAMPER") or os.path.exists(os.path.join(home, "claude-tamper")):
        s.setdefault("hooks", {})["Stop"] = []
    save_settings(s)
    msg = f'Plugin "{pid}" is already installed (scope: user)' if already else f"Successfully installed plugin: {pid} (scope: user)"
    emit({"command": "install", "outcome": "ok", "plugin": pid, "pluginId": pid, "scope": "user", "message": msg})
if args[:2] == ["plugin", "uninstall"]:
    pid = args[2]
    inst = load(installed_p, {"version": 2, "plugins": {}})
    if pid not in inst["plugins"]:
        print(json.dumps({"command": "uninstall", "outcome": "failed", "plugin": pid, "scope": "user", "message": "not found", "failureCode": "not_installed"}))
        print("Failed to uninstall", file=sys.stderr)
        sys.exit(1)
    del inst["plugins"][pid]
    with open(installed_p, "w") as f:
        f.write(json.dumps(inst, indent=2))
    s = load(settings_p, {})
    s.get("enabledPlugins", {}).pop(pid, None)
    save_settings(s)
    emit({"command": "uninstall", "outcome": "ok", "plugin": pid, "pluginId": pid, "scope": "user", "keptData": False})
if args[:3] == ["plugin", "marketplace", "remove"]:
    name = args[3]
    s = load(settings_p, {})
    if name not in s.get("extraKnownMarketplaces", {}):
        emit({"command": "marketplace-remove", "marketplace": name, "outcome": "failed", "message": "not found", "failureCode": "not_configured"}, 1)
    del s["extraKnownMarketplaces"][name]
    save_settings(s)
    emit({"command": "marketplace-remove", "outcome": "ok", "marketplace": name})
print("unexpected claude call: " + " ".join(args), file=sys.stderr)
sys.exit(3)
