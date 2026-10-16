#!/bin/bash
# hermes-offline-gate.sh: the offline Hermes checks of SPEC-v0.3 §5.6 / §9.2.
#
#   tools/integrations/hermes-offline-gate.sh <hermes-agent checkout> <scratch dir>
#
# Renders CodePet's Hermes plugin (integrations/hermes) into a throwaway HERMES_HOME in
# <scratch dir> (its shim is a stand-in that only logs), then runs the real Hermes CLI
# from <hermes-agent checkout> (its venv python): `plugins validate`, `plugins enable
# codepet`, `plugins doctor codepet`, `plugins disable codepet`, printing
# the throwaway config.yaml before and after. Every run uses a minimal environment
# (HOME and HERMES_HOME in <scratch dir>, PYTHONDONTWRITEBYTECODE=1 so nothing is written
# into the checkout) inside a network namespace without any network (`unshare -rn`). It
# never reads or writes the caller's real ~/.hermes and never calls `hermes --version`.
set -euo pipefail
[ "$#" -eq 2 ] || { echo "usage: $0 <hermes-agent checkout> <scratch dir>" >&2; exit 2; }
HA=$(cd -- "$1" && pwd -P)
D=$(mkdir -p -- "$2" && cd -- "$2" && pwd -P)
REPO=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
command -v unshare >/dev/null || { echo "unshare is required" >&2; exit 1; }
H="$D/home"
rm -rf -- "$H"
P="$H/.hermes/plugins/codepet"
SHIM="$D/codepet-hook"
mkdir -p -- "$P"
printf '#!/bin/sh\ncat >> "%s/events.log"\n' "$D" >"$SHIM"
chmod 0755 "$SHIM"
cp -- "$REPO/integrations/hermes/plugin.yaml" "$P/plugin.yaml"
sed "s|@CODEPET_SHIM@|$SHIM|" "$REPO/integrations/hermes/__init__.py.in" >"$P/__init__.py"
printf '# throwaway\nmodel:\n  default: m1\nplugins:\n  enabled:\n  - other\nhooks:\n  pre_tool_call:\n  - command: echo hi\n' >"$H/.hermes/config.yaml"
redact() { sed "s|$D|/scratch|g"; }
X() {
    local out rc
    echo "\$ hermes $*" | redact
    rc=0
    out=$(cd "$D" && timeout 60 unshare -rn env -i HOME="$H" HERMES_HOME="$H/.hermes" PATH=/usr/bin:/bin \
        PYTHONDONTWRITEBYTECODE=1 "$HA/venv/bin/python" "$HA/hermes" "$@" </dev/null 2>&1) || rc=$?
    printf '%s\n' "$out" | redact
    echo "[exit $rc]"
}
echo "== config.yaml before"; redact <"$H/.hermes/config.yaml"
X plugins validate "$P"
X plugins enable codepet
X plugins doctor codepet

echo "== config.yaml after enable"; redact <"$H/.hermes/config.yaml"
X plugins disable codepet
echo "== config.yaml after disable"; redact <"$H/.hermes/config.yaml"
grep -q 'codepet' "$H/.hermes/config.yaml" && echo "gate: done (read the doctor output above)"
