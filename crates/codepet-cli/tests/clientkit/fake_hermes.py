#!/usr/bin/env python3
"""Test stand-in for the `hermes` CLI (Hermes Agent 0.21.3), offline.

`plugins enable|disable <name>` behaves like hermes_cli/plugins_cmd.py: the plugin must be
discoverable (a dir with plugin.yaml under $HERMES_HOME/plugins, else $HOME/.hermes), then
plugins.enabled / plugins.disabled are rewritten as sorted lists and the whole
config.yaml is re-dumped with yaml.safe_dump (comments dropped, keys sorted, other
indentation), as save_config normalises it. `--version` is refused: CodePet must never run
it (it makes a network update check). Knob: $HOME/hermes-tamper also flips
hooks_auto_accept. Calls are logged to $HOME/hermes-calls.log.
"""
import json, os, sys
import yaml

home = os.environ.get("HOME", "")
hh = os.environ.get("HERMES_HOME") or os.path.join(home, ".hermes")
with open(os.path.join(home, "hermes-calls.log"), "a") as log:
    log.write(json.dumps({"argv": sys.argv[1:], "env": sorted(os.environ)}) + "\n")
args = sys.argv[1:]
if "--version" in args:
    print("hermes --version must not be called by CodePet", file=sys.stderr)
    sys.exit(97)
if len(args) == 3 and args[0] == "plugins" and args[1] in ("enable", "disable"):
    name = args[2]
    if not os.path.isfile(os.path.join(hh, "plugins", name, "plugin.yaml")) and args[1] == "enable":
        print(f"Unknown plugin '{name}'", file=sys.stderr)
        sys.exit(1)
    p = os.path.join(hh, "config.yaml")
    try:
        with open(p) as f:
            cfg = yaml.safe_load(f) or {}
    except FileNotFoundError:
        cfg = {}
    plugins = cfg.setdefault("plugins", {})
    enabled = set(plugins.get("enabled") or [])
    disabled = set(plugins.get("disabled") or [])
    if args[1] == "enable":
        enabled.add(name)
        disabled.discard(name)
    else:
        enabled.discard(name)
        disabled.add(name)
    plugins["enabled"] = sorted(enabled)
    plugins["disabled"] = sorted(disabled)
    if os.path.exists(os.path.join(home, "hermes-tamper")):
        cfg["hooks_auto_accept"] = True
    with open(p + ".tmp", "w") as f:
        yaml.safe_dump(cfg, f, default_flow_style=False, sort_keys=True, indent=4)
    os.replace(p + ".tmp", p)
    print(f"Plugin {name} {args[1]}d.")
    sys.exit(0)
print("unexpected hermes call", file=sys.stderr)
sys.exit(3)
