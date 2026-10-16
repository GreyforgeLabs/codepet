#!/bin/bash
# claude-day0-gate.sh: the day-0 Claude Code gate of SPEC-v0.3 §5.3, offline.
#
#   tools/integrations/claude-day0-gate.sh <claude binary> <scratch dir>
#
# Builds CodePet's codepet-local marketplace (integrations/claude-code, rendered with a
# throwaway shim path) inside <scratch dir>, then runs the real `claude plugin validate`,
# `plugin install --json` (twice), `plugin list --json`, `plugin uninstall --json` (twice)
# and `plugin marketplace remove --json` (twice) against a throwaway CLAUDE_CONFIG_DIR and
# HOME in <scratch dir>, under the client CLI rules (minimal environment, stdin
# /dev/null, DISABLE_AUTOUPDATER=1, CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1) and inside a
# network namespace without any network (`unshare -rn`), so no model call or other network
# use is possible. It prints each command's --json line with the scratch path replaced by
# /home/u, plus the throwaway settings.json before and after, and checks that the
# environment override is honoured (everything lands under <scratch dir>). It never reads
# or writes the caller's real ~/.claude.
set -euo pipefail
[ "$#" -eq 2 ] || { echo "usage: $0 <claude binary> <scratch dir>" >&2; exit 2; }
CLAUDE=$1
D=$(mkdir -p -- "$2" && cd -- "$2" && pwd -P)
REPO=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
command -v unshare >/dev/null || { echo "unshare is required" >&2; exit 1; }
H="$D/home"
rm -rf -- "$H"
M="$H/.local/share/codepet/integrations/claude-code"
SHIM="$H/.local/share/codepet/bin/codepet-hook"
mkdir -p -- "$M/.claude-plugin" "$M/plugin/.claude-plugin" "$M/plugin/hooks" "$H/.claude"
cp -- "$REPO/integrations/claude-code/.claude-plugin/marketplace.json" "$M/.claude-plugin/"
sed 's/@CODEPET_VERSION@/0.3.0/' "$REPO/integrations/claude-code/plugin/.claude-plugin/plugin.json.in" >"$M/plugin/.claude-plugin/plugin.json"
sed "s|@CODEPET_SHIM@|$SHIM|g" "$REPO/integrations/claude-code/plugin/hooks/hooks.json.in" >"$M/plugin/hooks/hooks.json"
printf '{\n  "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "slopbrake-hook stop", "timeout": 600}]}]},\n  "permissions": {"allow": ["Bash(ls)"]},\n  "enabledPlugins": {"x@y": true},\n  "theme": "dark"\n}\n' >"$H/.claude/settings.json"
redact() { sed "s|$D/home|/home/u|g"; }
C() {
    local out rc
    echo "\$ claude $*" | redact
    rc=0
    out=$(cd "$D" && timeout 30 unshare -rn env -i HOME="$H" PATH=/usr/bin:/bin DISABLE_AUTOUPDATER=1 \
        CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1 CLAUDE_CONFIG_DIR="$H/.claude" "$CLAUDE" "$@" </dev/null 2>&1) || rc=$?
    printf '%s\n' "$out" | redact
    echo "[exit $rc]"
}
show() { echo "== $1"; redact <"$2"; echo; }
show "settings.json before" "$H/.claude/settings.json"
C plugin validate "$M"
C plugin validate "$M/plugin"
C plugin install codepet --marketplace "$M" --json
C plugin install codepet --marketplace "$M" --json
C plugin list --json
show "settings.json after install" "$H/.claude/settings.json"
show "installed_plugins.json" "$H/.claude/plugins/installed_plugins.json"
C plugin uninstall codepet@codepet-local --json
C plugin uninstall codepet@codepet-local --json
C plugin marketplace remove codepet-local --json
C plugin marketplace remove codepet-local --json
show "settings.json after uninstall" "$H/.claude/settings.json"
show "installed_plugins.json after uninstall" "$H/.claude/plugins/installed_plugins.json"
# The override was honoured: the install records live in the throwaway config dir.
grep -q '"plugins": {}' "$H/.claude/plugins/installed_plugins.json"
echo "gate: passed (CLAUDE_CONFIG_DIR honoured; install, already-installed, list, uninstall, marketplace remove captured)"
