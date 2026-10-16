#!/usr/bin/env bash
# End-to-end hook test (SPEC-v0.3 §9.2). Runs entirely inside a throwaway HOME and XDG dirs:
# it never touches the real ~/.local/{bin,share,state}/codepet, never reads or writes a
# coding client's config and never starts a client. No network.
#
#   scripts/e2e-hooks.sh [path/to/codepet] [--keep]
#
# 1. install: `codepet migrate` creates the v5 database, `pet.start` adopts a pet, then the
#    inbox and the frozen shim;
# 2. pipes each client's fixture sequence (start -> tools -> waiting -> resume -> done -> end)
#    through its stored command string (sh -c for Claude Code/Codex/Grok, argv for
#    OpenCode/Hermes);
# 3. waits for the detached fold after every event;
# 4. asserts the bar-state.json transitions, finished_seq, hello_seq, the jar and
#    last_seen_ms; then bursts (500 events from one writer, 20 concurrent writers) with
#    the fold process count sampled every 5 ms.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/.." && pwd)
bin=${1:-$repo/target/debug/codepet}
keep=0
[ "${2:-}" = --keep ] && keep=1
[ -x "$bin" ] || { echo "e2e: no codepet binary at $bin (cargo build first)" >&2; exit 2; }
bin=$(cd "$(dirname "$bin")" && pwd)/$(basename "$bin")

work=$(mktemp -d "${TMPDIR:-/tmp}/codepet-e2e.XXXXXX")
cleanup() { if [ "$keep" = 1 ]; then echo "e2e: kept $work"; else rm -rf "$work"; fi; }
trap cleanup EXIT
for d in home state data config; do mkdir -m 0700 "$work/$d"; done

export HOME="$work/home" XDG_STATE_HOME="$work/state" XDG_DATA_HOME="$work/data"
export XDG_CONFIG_HOME="$work/config" PATH=/usr/bin:/bin LC_ALL=C
unset CLAUDE_CODE_ENTRYPOINT GROK_HOOK_EVENT CODEPET_FAULT

S="$XDG_STATE_HOME/codepet"
"$bin" migrate >/dev/null
# A fresh v0.3 database has no pet: adopt one so finished turns earn treats.
started=$(printf '%s\n' '{"v":1,"id":"e2e-start","method":"pet.start","params":{"species":"cat","mode":"gentle","acknowledge_rules":true}}' | "$bin" rpc)
case "$started" in *'"ok":true'*) ;; *) echo "e2e: pet.start failed: $started" >&2; exit 1;; esac
for d in inbox clients; do [ -d "$S/$d" ] || mkdir -m 0700 "$S/$d"; done

shim="$XDG_DATA_HOME/codepet/bin/codepet-hook"
mkdir -p "$(dirname "$shim")"
case "$S$bin" in *"'"*|*$'\n'*) echo "e2e: paths the shim cannot quote" >&2; exit 2;; esac
python3 -I - "$repo/docs/v0.3/contracts/shim.sh.template" "$S" "$bin" "$shim" <<'PY'
import os, sys
t, s, b, out = sys.argv[1:]
text = open(t).read().replace("@CODEPET_STATE_DIR@", s, 1).replace("@CODEPET_BIN@", b, 1)
tmp = out + ".partial"
with open(tmp, "w") as f:
    f.write(text)
os.chmod(tmp, 0o755)
os.replace(tmp, out)
PY

python3 -I "$here/e2e_hooks.py" --state "$S" --shim "$shim"
