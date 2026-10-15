#!/bin/bash
# install.sh: install or update CodePet for the current user on Omarchy (spec §15).
#
#   packaging/omarchy/install.sh [--dry-run] [--no-plugin]
#
# Steps, all idempotent:
#   1. build the `codepet` binary (memory-capped), unless CODEPET_BIN_SOURCE names a
#      prebuilt one (release archives);
#   2. install `codepet` and `codepet-overlay` into ~/.local/bin atomically
#      (temporary name, then rename) and check `codepet --version`;
#   3. record whether an overlay is running, copy qml/{shell.qml,shared,omarchy,assets}
#      to ~/.local/share/codepet/releases/<version>/qml and atomically repoint the
#      `current` symlink there (it lives outside the plugin folder);
#   4. stage the bar plugin (omarchy-plugin/ plus qml/shared/LifeText.js, byte for
#      byte, no symlinks) under ~/.cache/codepet, run `omarchy plugin validate`, and
#      move it into ~/.config/omarchy/plugins/greyforge.codepet with one rename
#      (`mv --exchange` on update), so the shell sees one change and hot-reloads once;
#      enable it once (right section, before the tray) if shell.json does not list it,
#      after asking the running shell to rescan and waiting until it lists the plugin
#      (without a running shell, the enable command is printed instead);
#   5. on update, print restart advice for a running overlay and the shell.
#
# An update keeps the replaced release tree and plugin under ~/.cache/codepet as
# release-backup-<version>-<ts> and plugin-backup-<ts>; only the newest of each is kept.
#
# It never reads, writes, moves or deletes $XDG_STATE_HOME/codepet (the pet's data):
# the first `codepet state` after install migrates the database itself and keeps a
# backup. It never restarts the shell or the overlay, and never arms a pet.
#
# --dry-run stages the files in a private temporary directory, lists them, checks none
#   is a symlink, removes the temporary directory and changes nothing else: no build,
#   no install, no omarchy or qs command.
# --no-plugin skips step 4 (binaries and overlay runtime only).
#
# Environment (optional): CODEPET_BIN_SOURCE (prebuilt codepet), CODEPET_OMARCHY (the
# omarchy command, default `omarchy`), CARGO_TARGET_DIR,
# XDG_DATA_HOME, XDG_CONFIG_HOME, XDG_CACHE_HOME, XDG_STATE_HOME (absolute paths only).
set -euo pipefail

PROG=install.sh
PLUGIN_ID=greyforge.codepet
OMARCHY=${CODEPET_OMARCHY:-omarchy}

usage() {
    echo "usage: $PROG [--dry-run] [--no-plugin]"
    exit "${1:-2}"
}
die() {
    echo "$PROG: $*" >&2
    exit 1
}

DRY=0
NO_PLUGIN=0
for arg in "$@"; do
    case "$arg" in
        --dry-run) DRY=1 ;;
        --no-plugin) NO_PLUGIN=1 ;;
        -h|--help) usage 0 ;;
        *) usage 2 ;;
    esac
done

[ -n "${HOME:-}" ] || die "HOME is not set"
case "$HOME" in /*) ;; *) die "HOME must be absolute" ;; esac

REPO=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
VERSION=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' \
    "$REPO/Cargo.toml" | head -n 1)
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "cannot read the workspace version from Cargo.toml"

xdg() { # xdg <value> <default>: an absolute XDG value, else the default
    case "${1:-}" in /*) printf '%s\n' "$1" ;; *) printf '%s\n' "$2" ;; esac
}
DATA_HOME=$(xdg "${XDG_DATA_HOME:-}" "$HOME/.local/share")
CONFIG_HOME=$(xdg "${XDG_CONFIG_HOME:-}" "$HOME/.config")
CACHE_HOME=$(xdg "${XDG_CACHE_HOME:-}" "$HOME/.cache")
STATE_HOME=$(xdg "${XDG_STATE_HOME:-}" "$HOME/.local/state")

BIN_DIR="$HOME/.local/bin"
RUNTIME="$DATA_HOME/codepet"
RELEASE="$RUNTIME/releases/$VERSION"
PLUGINS="$CONFIG_HOME/omarchy/plugins"
PLUGIN_DIR="$PLUGINS/$PLUGIN_ID"
SHELL_JSON="$CONFIG_HOME/omarchy/shell.json"
WORK="$CACHE_HOME/codepet"
TS="$(date +%Y%m%d-%H%M%S)-$$"

RUNTIME_SOURCES=(qml/shell.qml qml/shared qml/omarchy qml/assets)
PLUGIN_SOURCES=(omarchy-plugin qml/shared/LifeText.js)
LAUNCHER_SOURCE=packaging/omarchy/codepet-overlay

# A symlink in what we ship could point anywhere; refuse instead of copying it.
for src in "${RUNTIME_SOURCES[@]}" "${PLUGIN_SOURCES[@]}" "$LAUNCHER_SOURCE"; do
    [ -e "$REPO/$src" ] || die "missing $src"
    links=$(find "$REPO/$src" -type l -print)
    [ -z "$links" ] || die "refusing to install a symlink: ${links//$'\n'/, }"
done

# stage_runtime <dir>: <dir>/qml/{shell.qml,shared,omarchy,assets}
stage_runtime() {
    mkdir -p -- "$1/qml"
    cp -- "$REPO/qml/shell.qml" "$1/qml/shell.qml"
    for d in shared omarchy assets; do
        cp -R -- "$REPO/qml/$d" "$1/qml/$d"
    done
}
# stage_plugin <dir>: <dir> = omarchy-plugin/ plus LifeText.js (spec §14.1)
stage_plugin() {
    mkdir -p -- "$1"
    cp -R -- "$REPO/omarchy-plugin/." "$1/"
    cp -- "$REPO/qml/shared/LifeText.js" "$1/LifeText.js"
}
# no_symlinks <dir>: the staged copy holds regular files and directories only.
no_symlinks() {
    local bad
    bad=$(find "$1" ! -type f ! -type d -print)
    [ -z "$bad" ] || die "staged copy holds a non-regular file: ${bad//$'\n'/, }"
}

if [ "$DRY" -eq 1 ]; then
    tmp=$(mktemp -d "${TMPDIR:-/tmp}/codepet-install-dry.XXXXXX")
    trap 'rm -rf -- "$tmp"' EXIT
    stage_runtime "$tmp/runtime"
    [ "$NO_PLUGIN" -eq 1 ] || stage_plugin "$tmp/plugin/$PLUGIN_ID"
    no_symlinks "$tmp"
    echo "CodePet $VERSION install plan (dry run: nothing is built, installed or changed)"
    if [ -n "${CODEPET_BIN_SOURCE:-}" ]; then
        echo "would use prebuilt: $CODEPET_BIN_SOURCE"
    else
        echo "would build: cargo build --release --locked -p codepet -j 2 (memory-capped scope)"
    fi
    echo "would install: $BIN_DIR/codepet"
    echo "would install: $BIN_DIR/codepet-overlay"
    echo "would copy runtime to: $RELEASE/qml"
    echo "would point: $RUNTIME/current -> releases/$VERSION"
    (cd "$tmp/runtime" && find . -type f | sed 's|^\./|runtime |' | LC_ALL=C sort)
    count=$(find "$tmp" -type f | wc -l)
    if [ "$NO_PLUGIN" -eq 0 ]; then
        echo "would validate: omarchy plugin validate <stage>/$PLUGIN_ID"
        echo "would install plugin: $PLUGIN_DIR"
        (cd "$tmp/plugin" && find . -type f | sed 's|^\./|plugin |' | LC_ALL=C sort)
        if [ -f "$SHELL_JSON" ] && grep -q "\"$PLUGIN_ID\"" "$SHELL_JSON"; then
            echo "already enabled in: $SHELL_JSON"
        else
            echo "would enable: omarchy plugin enable $PLUGIN_ID --section right --before omarchy.tray"
        fi
    fi
    echo "state: $STATE_HOME/codepet is never touched"
    echo "staged files: $count, symlinks: 0"
    exit 0
fi

UPDATE=0
{ [ -e "$RUNTIME/current" ] || [ -L "$RUNTIME/current" ] || [ -e "$PLUGIN_DIR" ]; } && UPDATE=1

# 1. build (or use a prebuilt binary)
if [ -n "${CODEPET_BIN_SOURCE:-}" ]; then
    BIN_SRC=$CODEPET_BIN_SOURCE
else
    command -v cargo >/dev/null 2>&1 || die "cargo not found (or set CODEPET_BIN_SOURCE to a built codepet)"
    if command -v systemd-run >/dev/null 2>&1; then
        (cd "$REPO" && systemd-run --user --scope --quiet -p MemoryMax=4G -p MemorySwapMax=2G -- \
            cargo build --release --locked -p codepet -j 2)
    else
        (cd "$REPO" && cargo build --release --locked -p codepet -j 2)
    fi
    BIN_SRC="${CARGO_TARGET_DIR:-$REPO/target}/release/codepet"
fi
[ -f "$BIN_SRC" ] && [ ! -L "$BIN_SRC" ] || die "no built codepet binary at $BIN_SRC"

# prune_backups <prefix> <kept>: after a successful swap, remove this script's earlier
# backups named $WORK/<prefix>*, keeping only <kept> (the one just made), so repeated
# installs do not grow ~/.cache by a release tree each time.
prune_backups() {
    local old
    for old in "$WORK/$1"*; do
        [ "$old" != "$2" ] && [ -d "$old" ] && [ ! -L "$old" ] || continue
        rm -rf -- "$old"
        echo "removed older backup: $old"
    done
}

# 2. binaries, each by temporary name then rename
install_file() { # install_file <src> <dest>
    install -m 0755 -- "$1" "$2.tmp-$TS"
    mv -f -- "$2.tmp-$TS" "$2"
}
mkdir -p -- "$BIN_DIR"
install_file "$BIN_SRC" "$BIN_DIR/codepet"
got=$("$BIN_DIR/codepet" --version 2>/dev/null || true)
[ "$got" = "codepet $VERSION" ] || die "installed codepet reports '$got', expected 'codepet $VERSION'"
install_file "$REPO/$LAUNCHER_SOURCE" "$BIN_DIR/codepet-overlay"
echo "installed: $BIN_DIR/codepet ($got), $BIN_DIR/codepet-overlay"

# 3. overlay runtime; record a running overlay before `current` moves
OVERLAY_RUNNING=0
if [ "$UPDATE" -eq 1 ] && "$BIN_DIR/codepet-overlay" status >/dev/null 2>&1; then
    OVERLAY_RUNNING=1
fi
mkdir -p -- "$RUNTIME/releases" "$WORK"
next="$RUNTIME/releases/.$VERSION.new-$TS"
stage_runtime "$next"
no_symlinks "$next"
if [ -e "$RELEASE" ]; then
    # Same version again: swap in one rename where possible; keep the old copy.
    if ! mv --exchange -T -- "$next" "$RELEASE" 2>/dev/null; then
        mv -T -- "$RELEASE" "$next.old"
        mv -T -- "$next" "$RELEASE"
        next="$next.old"
    fi
    mv -T -- "$next" "$WORK/release-backup-$VERSION-$TS"
    echo "runtime: previous copy of $VERSION kept at $WORK/release-backup-$VERSION-$TS"
    prune_backups release-backup- "$WORK/release-backup-$VERSION-$TS"
else
    mv -T -- "$next" "$RELEASE"
fi
ln -s -- "releases/$VERSION" "$RUNTIME/.current-$TS"
mv -T -- "$RUNTIME/.current-$TS" "$RUNTIME/current"
echo "runtime: $RUNTIME/current -> releases/$VERSION"

# 4. bar plugin
ENABLE_HINT="omarchy plugin enable $PLUGIN_ID --section right --before omarchy.tray"
# discovered: ask the running shell to rescan, then wait (up to about 2 s) until it lists
# the plugin, as omarchy-plugin-add does. The shell otherwise learns of a new folder only
# after a file-watch debounce and an asynchronous scan, and an enable sent before that
# fails as "not known". False when the shell is not running or never lists the id.
discovered() {
    local attempt list
    "$OMARCHY" shell shell rescanPlugins >/dev/null 2>&1 || return 1
    for ((attempt = 0; attempt < 40; attempt++)); do
        if list=$("$OMARCHY" plugin list --json 2>/dev/null); then
            if command -v jq >/dev/null 2>&1; then
                jq -e --arg id "$PLUGIN_ID" 'any(.[]; .id == $id)' >/dev/null 2>&1 <<<"$list" && return 0
            else
                grep -q "\"$PLUGIN_ID\"" <<<"$list" && return 0
            fi
        fi
        sleep 0.05
    done
    return 1
}
if [ "$NO_PLUGIN" -eq 0 ]; then
    command -v "$OMARCHY" >/dev/null 2>&1 || die "omarchy not found; rerun with --no-plugin"
    stage="$WORK/stage-$TS/$PLUGIN_ID"
    stage_plugin "$stage"
    no_symlinks "$stage"
    "$OMARCHY" plugin validate "$stage" || die "omarchy plugin validate refused the staged plugin ($stage)"
    mkdir -p -- "$PLUGINS"
    if [ -e "$PLUGIN_DIR" ]; then
        # One rename swaps old and new; the old tree, now at the stage path, is kept.
        if ! mv --exchange -T -- "$stage" "$PLUGIN_DIR" 2>/dev/null; then
            echo "$PROG: mv --exchange unavailable; replacing the plugin with two renames" >&2
            mv -T -- "$PLUGIN_DIR" "$stage.old"
            mv -T -- "$stage" "$PLUGIN_DIR"
            stage="$stage.old"
        fi
        mv -T -- "$stage" "$WORK/plugin-backup-$TS"
        echo "plugin: updated $PLUGIN_DIR (previous copy: $WORK/plugin-backup-$TS)"
        prune_backups plugin-backup- "$WORK/plugin-backup-$TS"
    else
        mv -T -- "$stage" "$PLUGIN_DIR"
        echo "plugin: installed $PLUGIN_DIR"
    fi
    rmdir -- "$WORK/stage-$TS" 2>/dev/null || true
    if [ -f "$SHELL_JSON" ] && grep -q "\"$PLUGIN_ID\"" "$SHELL_JSON"; then
        echo "plugin: already listed in $SHELL_JSON"
    elif ! discovered; then
        echo "plugin: the running shell has not discovered $PLUGIN_ID yet; once it has, enable it with:"
        echo "  $ENABLE_HINT"
    elif "$OMARCHY" plugin enable "$PLUGIN_ID" --section right --before omarchy.tray; then
        echo "plugin: enabled in the right section"
    else
        echo "plugin: not enabled; enable it with:"
        echo "  $ENABLE_HINT"
    fi
fi

# 5. advice only; nothing is restarted here
if [ "$UPDATE" -eq 1 ]; then
    if [ "$OVERLAY_RUNNING" -eq 1 ]; then
        echo "The desktop overlay is still running the previous release. Once no task is executing:"
        echo "  codepet-overlay hide && codepet-overlay show"
    fi
    if [ "$NO_PLUGIN" -eq 0 ]; then
        echo "If the bar does not pick up the plugin by itself (never while the session is locked):"
        echo "  HYPRLAND_INSTANCE_SIGNATURE=${HYPRLAND_INSTANCE_SIGNATURE:-<live signature>} omarchy restart shell"
    fi
fi
echo "state: $STATE_HOME/codepet was not touched"
