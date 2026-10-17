#!/bin/bash
# uninstall.sh: stop watching every coding app, disable the CodePet bar plugin and move
# the plugin and the overlay runtime out of the way (v0.3, SPEC-v0.3 §5.1, §7.3).
#
#   packaging/omarchy/uninstall.sh [--dry-run]
#
# First every coding app is disconnected with `codepet disconnect <app>` (Claude Code and
# Hermes through their own CLIs, Codex without renumbering other hooks); an app CodePet was
# never connected to and has no files in is only reported ("nothing to stop"), never
# recorded or warned about. Then the hook
# shim ~/.local/share/codepet/bin/codepet-hook is replaced by the tombstone (it drains
# stdin and exits 0, printing nothing), never deleted: a coding app may still reference it
# (a Codex line that other hooks follow, or an app whose disconnect failed; those are
# listed). Nothing else is deleted: the plugin folder and ~/.local/share/codepet/
# {current,releases} move to ~/.cache/codepet/uninstall-<timestamp>/. The pet's data in
# $XDG_STATE_HOME/codepet is kept (the disconnects record themselves there), and the
# `codepet`, `codepet-overlay` `codepet-work` and `codepet-update` stay in ~/.local/bin (remove them by hand if you
# want).
# A running desktop overlay is left alone and the uninstall refuses until it is stopped
# (`codepet-overlay quit`). If `omarchy plugin disable` fails (no running shell, or the
# shell never loaded the plugin), a warning names the command to run later and the
# uninstall continues.
set -euo pipefail

PROG=uninstall.sh
PLUGIN_ID=greyforge.codepet
OMARCHY=${CODEPET_OMARCHY:-omarchy}

usage() {
    echo "usage: $PROG [--dry-run]"
    exit "${1:-2}"
}
die() {
    echo "$PROG: $*" >&2
    exit 1
}

DRY=0
for arg in "$@"; do
    case "$arg" in
        --dry-run) DRY=1 ;;
        -h|--help) usage 0 ;;
        *) usage 2 ;;
    esac
done

[ -n "${HOME:-}" ] || die "HOME is not set"
case "$HOME" in /*) ;; *) die "HOME must be absolute" ;; esac
xdg() {
    case "${1:-}" in /*) printf '%s\n' "$1" ;; *) printf '%s\n' "$2" ;; esac
}
DATA_HOME=$(xdg "${XDG_DATA_HOME:-}" "$HOME/.local/share")
CONFIG_HOME=$(xdg "${XDG_CONFIG_HOME:-}" "$HOME/.config")
CACHE_HOME=$(xdg "${XDG_CACHE_HOME:-}" "$HOME/.cache")
STATE_HOME=$(xdg "${XDG_STATE_HOME:-}" "$HOME/.local/state")
RUNTIME="$DATA_HOME/codepet"
PLUGIN_DIR="$CONFIG_HOME/omarchy/plugins/$PLUGIN_ID"
SHELL_JSON="$CONFIG_HOME/omarchy/shell.json"
BACKUP="$CACHE_HOME/codepet/uninstall-$(date +%Y%m%d-%H%M%S)-$$"
LAUNCHER="$HOME/.local/bin/codepet-overlay"
CODEPET="$HOME/.local/bin/codepet"
SHIM="$RUNTIME/bin/codepet-hook"
CLIENTS=(claude-code codex opencode hermes grok-cli)
TOMBSTONE='#!/bin/sh
# managed by codepet: tombstone
exec >/dev/null 2>&1
timeout -s KILL 0.3 cat >/dev/null 2>&1; exit 0
'

if [ -x "$LAUNCHER" ] && "$LAUNCHER" status >/dev/null 2>&1; then
    die "the desktop overlay is running; stop it first (codepet-overlay quit), then rerun"
fi

moves=()
[ -e "$PLUGIN_DIR" ] && moves+=("$PLUGIN_DIR:plugin-$PLUGIN_ID")
{ [ -e "$RUNTIME/current" ] || [ -L "$RUNTIME/current" ]; } && moves+=("$RUNTIME/current:current")
[ -e "$RUNTIME/releases" ] && moves+=("$RUNTIME/releases:releases")
enabled=0
[ -f "$SHELL_JSON" ] && grep -q "\"$PLUGIN_ID\"" "$SHELL_JSON" && enabled=1

if [ "$DRY" -eq 1 ]; then
    echo "CodePet uninstall plan (dry run: nothing is changed)"
    if [ -e "$SHIM" ]; then
        echo "would stop watching: ${CLIENTS[*]} (codepet disconnect <app>)"
        echo "would tombstone: $SHIM"
    fi
    if [ "$enabled" -eq 1 ]; then
        echo "would disable: omarchy plugin disable $PLUGIN_ID (if that fails, a warning is printed and the uninstall continues)"
    fi
    for m in "${moves[@]}"; do echo "would move: ${m%:*} -> $BACKUP/${m##*:}"; done
    echo "state: $STATE_HOME/codepet is kept"
    exit 0
fi

# Stop watching every coding app, then tombstone the shim (never delete it).
if [ -e "$SHIM" ] || [ -L "$SHIM" ]; then
    still=()
    if [ -x "$CODEPET" ]; then
        for app in "${CLIENTS[@]}"; do
            if out=$("$CODEPET" disconnect "$app" 2>&1); then
                printf 'apps: %s\n' "$out"
            else
                still+=("$app")
                printf '%s: warning: %s\n' "$PROG" "${out//$'\n'/ }" >&2
            fi
        done
    else
        still=("${CLIENTS[@]}")
        echo "$PROG: warning: $CODEPET is missing; coding apps were not disconnected" >&2
    fi
    shim_dir=$(dirname -- "$SHIM")
    printf '%s' "$TOMBSTONE" >"$shim_dir/.codepet-hook.tombstone-$$"
    chmod 0755 "$shim_dir/.codepet-hook.tombstone-$$"
    mv -f -- "$shim_dir/.codepet-hook.tombstone-$$" "$SHIM"
    echo "tombstoned: $SHIM (it does nothing; kept because a coding app may still name it)"
    if [ "${#still[@]}" -gt 0 ]; then
        echo "warning: could not stop watching: ${still[*]}. Their hooks now reach the tombstone and do nothing;"
        echo "  see codepet doctor, or remove CodePet from those apps by hand (README, Help)."
    fi
fi

DISABLE_FAILED=0
if [ "$enabled" -eq 1 ]; then
    if command -v "$OMARCHY" >/dev/null 2>&1 && "$OMARCHY" plugin disable "$PLUGIN_ID"; then
        echo "disabled: $PLUGIN_ID"
    else
        DISABLE_FAILED=1
        echo "$PROG: warning: could not disable $PLUGIN_ID (omarchy missing, shell not running or plugin not loaded); continuing" >&2
    fi
fi
if [ "${#moves[@]}" -gt 0 ]; then
    mkdir -p -- "$BACKUP"
    for m in "${moves[@]}"; do
        mv -T -- "${m%:*}" "$BACKUP/${m##*:}"
        echo "moved: ${m%:*} -> $BACKUP/${m##*:}"
    done
else
    echo "nothing to move: CodePet's plugin and overlay runtime are not installed"
fi
if [ "${#moves[@]}" -gt 0 ]; then
    echo "backup: $BACKUP (delete it when you no longer need it)"
fi
echo "kept: $HOME/.local/bin/codepet, codepet-overlay, codepet-work and codepet-update (remove by hand if you want)"
if [ "$DISABLE_FAILED" -eq 1 ]; then
    echo "warning: $PLUGIN_ID may still be listed in $SHELL_JSON. Once the shell runs, try:"
    echo "  omarchy plugin disable $PLUGIN_ID"
    echo "or remove \"$PLUGIN_ID\" from the bar layout in $SHELL_JSON."
fi
echo "state: $STATE_HOME/codepet was kept"
