#!/bin/bash
# install.sh: install or update CodePet for the current user on Omarchy (v0.5.0, schema 6).
#
#   packaging/omarchy/install.sh [--dry-run] [--no-plugin]
#
# It runs from a source checkout (Cargo.toml present: the version comes from the
# workspace and `codepet` is built) or from an extracted release archive (VERSION and
# bin/codepet present, no Cargo.toml: the bundled binary is installed, nothing is built).
#
# Steps, all idempotent:
#   1. build the `codepet` binary (memory-capped), unless CODEPET_BIN_SOURCE names a
#      prebuilt one or this is a release archive (its bin/codepet); before anything is
#      replaced, that binary must report exactly `codepet <version>` of this tree, and this
#      tree's plugin needs a matching 0.4.x backend (an older binary refuses schema 6 data);
#   2. upgrade only: stop a running desktop overlay and any CodePet 0.2 task supervisor,
#      each by its exact PID (never a pattern kill);
#   3. install `codepet`, `codepet-overlay` `codepet-work` and `codepet-update` into ~/.local/bin atomically (temporary
#      name, then rename);
#   4. copy qml/{shell.qml,work.qml,shared,omarchy,assets} to ~/.local/share/codepet/releases/
#      <version>/qml and atomically repoint the `current` symlink there;
#   5. stage the bar plugin (omarchy-plugin/ plus qml/shared/LifeText.js, byte for byte, no
#      symlinks) beside the plugins folder (~/.config/omarchy/.codepet-stage-*, the same
#      filesystem, outside the watched folder), run `omarchy plugin validate`, and move it
#      into ~/.config/omarchy/plugins/greyforge.codepet with one rename (`mv --exchange` on
#      update), so the shell sees one change and hot-reloads once; on an update the running
#      shell is asked to rescan its plugins; on a first install the plugin is enabled once
#      (right section, before the tray) after the shell lists it (without a running shell
#      the enable command is printed instead);
#   6. run `codepet migrate` (creates the pet's data at schema 6, or upgrades a v0.2
#      database after a `VACUUM INTO …bak-v4-<ms>` backup; a newer or corrupt database is
#      refused untouched);
#   7. refresh the CodePet-owned files of already connected coding apps (`codepet
#      clients-refresh`: only files that carry CodePet's marker and differ; it never runs a
#      coding app and never edits a shared file such as Codex's hooks.json);
#   8. upgrade only: start the desktop overlay again if it was running.
#
# The bar icons ship twice (omarchy-plugin/assets/icons for the bar, qml/assets/icons for
# the overlay); the install refuses when the two copies differ.
#
# An update keeps the replaced binary, release tree and plugin under ~/.cache/codepet as
# codepet-backup-<old version>-<ts>, release-backup-<version>-<ts> and plugin-backup-<ts>;
# only the newest of each is kept (the binary's once the data is migrated). When
# `codepet migrate` fails, the exact commands that put the previous binary, plugin and
# overlay runtime back are printed (the data itself was not changed).
# Under ~/.local/share/codepet/releases it keeps the new release and the one `current`
# pointed to before; older release trees are removed, unless an overlay is running (it may
# still read its own release tree). Pruning never touches ~/.local/share/codepet/{bin,
# integrations,staging} (the hook shim and the coding-app integrations).
#
# The pet's data in $XDG_STATE_HOME/codepet is touched only by `codepet migrate` and
# `codepet clients-refresh` (step 6-7). The installer never connects a coding app, never
# arms a pet and never restarts the shell (it prints the restart command when needed).
#
# --dry-run stages the files in a private temporary directory, lists them, checks none
#   is a symlink, removes the temporary directory and changes nothing else: no build,
#   no install, no omarchy, qs or codepet command.
# --no-plugin skips step 5 (binaries and overlay runtime only).
#
# Environment (optional): CODEPET_BIN_SOURCE (prebuilt codepet), CODEPET_OMARCHY (the
# omarchy command, default `omarchy`), CODEPET_QS (the Quickshell command, default `qs`),
# CARGO_TARGET_DIR, XDG_DATA_HOME, XDG_CONFIG_HOME, XDG_CACHE_HOME, XDG_STATE_HOME
# (absolute paths only).
set -euo pipefail

PROG=install.sh
PLUGIN_ID=greyforge.codepet
OMARCHY=${CODEPET_OMARCHY:-omarchy}
QS=${CODEPET_QS:-qs}
# The release series this tree's plugin and overlay speak to (schema 6, protocol v1).
SERIES=0.4

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
# A source checkout has Cargo.toml; a release archive has VERSION and bin/codepet.
ARCHIVE=0
if [ -f "$REPO/Cargo.toml" ]; then
    VERSION=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' \
        "$REPO/Cargo.toml" | head -n 1) || VERSION=
    VERSION_FROM="the workspace version in Cargo.toml"
elif [ -f "$REPO/VERSION" ]; then
    ARCHIVE=1
    VERSION=$(head -n 1 -- "$REPO/VERSION") || VERSION=
    VERSION_FROM="the release archive's VERSION file"
else
    die "found neither Cargo.toml (source checkout) nor VERSION (release archive) in $REPO"
fi
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "cannot read $VERSION_FROM"
ARCHIVE_BIN="$REPO/bin/codepet"

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

RUNTIME_SOURCES=(qml/shell.qml qml/work.qml qml/shared qml/omarchy qml/assets)
PLUGIN_SOURCES=(omarchy-plugin qml/shared/LifeText.js)
LAUNCHER_SOURCE=packaging/omarchy/codepet-overlay
WORK_LAUNCHER_SOURCE=packaging/omarchy/codepet-work
UPDATER_SOURCE=packaging/omarchy/codepet-update

# A symlink in what we ship could point anywhere; refuse instead of copying it.
for src in "${RUNTIME_SOURCES[@]}" "${PLUGIN_SOURCES[@]}" "$LAUNCHER_SOURCE" "$WORK_LAUNCHER_SOURCE" "$UPDATER_SOURCE"; do
    [ -e "$REPO/$src" ] || die "missing $src"
    links=$(find "$REPO/$src" -type l -print)
    [ -z "$links" ] || die "refusing to install a symlink: ${links//$'\n'/, }"
done

# The two shipped copies of the icon set must be the same files.
if [ -d "$REPO/qml/assets/icons" ] || [ -d "$REPO/omarchy-plugin/assets/icons" ]; then
    diff -r -q -- "$REPO/qml/assets/icons" "$REPO/omarchy-plugin/assets/icons" >/dev/null 2>&1 ||
        die "the icon copies in qml/assets/icons and omarchy-plugin/assets/icons differ"
fi
case "$VERSION" in
    "$SERIES".*) ;;
    *) die "this tree ships the CodePet $SERIES bar plugin but its version is $VERSION" ;;
esac
command -v python3 >/dev/null 2>&1 || die "Python 3.11 or later is required for updates and the Work launcher"
python3 -c 'import sys; sys.exit(sys.version_info < (3, 11))' || die "Python 3.11 or later is required"
BUILD_ID=$(python3 "$REPO/$UPDATER_SOURCE" _identity "$REPO") || die "could not identify the complete UI content"
[[ "$BUILD_ID" =~ ^[0-9a-f]{64}$ ]] || die "invalid UI content identity"

stage_build_info() {
    printf '.pragma library\nvar identity = "%s"\n' "$BUILD_ID" > "$1"
}

# stage_runtime <dir>: <dir>/qml/{shell.qml,work.qml,shared,omarchy,assets}
stage_runtime() {
    mkdir -p -- "$1/qml"
    cp -- "$REPO/qml/shell.qml" "$1/qml/shell.qml"
    cp -- "$REPO/qml/work.qml" "$1/qml/work.qml"
    for d in shared omarchy assets; do
        cp -R -- "$REPO/qml/$d" "$1/qml/$d"
    done
    stage_build_info "$1/qml/shared/BuildInfo.js"
    local plugin=true
    [ "$NO_PLUGIN" -eq 0 ] || plugin=false
    printf '{"schema":"codepet.build.v1","identity":"%s","plugin":%s}\n' "$BUILD_ID" "$plugin" > "$1/build.json"
    chmod 600 "$1/build.json"
}
# stage_plugin <dir>: <dir> = omarchy-plugin/ plus LifeText.js and generated BuildInfo.js
stage_plugin() {
    mkdir -p -- "$1"
    cp -R -- "$REPO/omarchy-plugin/." "$1/"
    cp -- "$REPO/qml/shared/LifeText.js" "$1/LifeText.js"
    stage_build_info "$1/BuildInfo.js"
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
    elif [ "$ARCHIVE" -eq 1 ]; then
        echo "would use prebuilt: $ARCHIVE_BIN"
    else
        echo "would build: cargo build --release --locked -p codepet -j 2 (memory-capped scope)"
    fi
    echo "would install: $BIN_DIR/codepet"
    echo "would install: $BIN_DIR/codepet-overlay"
    echo "would install: $BIN_DIR/codepet-work"
    echo "would install: $BIN_DIR/codepet-update"
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
    echo "would run: codepet migrate (the pet's data in $STATE_HOME/codepet: created, or upgraded after a backup)"
    echo "would run: codepet clients-refresh (connected coding apps: CodePet-owned files only)"
    echo "staged files: $count, symlinks: 0"
    exit 0
fi

UPDATE=0
{ [ -e "$RUNTIME/current" ] || [ -L "$RUNTIME/current" ] || [ -e "$PLUGIN_DIR" ]; } && UPDATE=1

# Preflight, before anything is installed: the bar plugin needs Omarchy 4's plugin CLI.
if [ "$NO_PLUGIN" -eq 0 ]; then
    command -v "$OMARCHY" >/dev/null 2>&1 || die "omarchy not found; rerun with --no-plugin"
    "$OMARCHY" plugin validate --help >/dev/null 2>&1 ||
        die "this omarchy has no plugin support (Omarchy 4 or later is required); rerun with --no-plugin"
fi
# The pet sprites are WebP: without Qt's WebP image plugin they render blank.
webp_plugin=0
for d in /usr/lib/qt6/plugins /usr/lib64/qt6/plugins /usr/lib/x86_64-linux-gnu/qt6/plugins; do
    [ -e "$d/imageformats/libqwebp.so" ] && webp_plugin=1
done
[ "$webp_plugin" -eq 1 ] ||
    echo "$PROG: warning: Qt's WebP image plugin (qt6-imageformats) was not found; the pet will render blank without it" >&2

# 1. build (or use a prebuilt binary), then check it before anything is replaced
if [ -n "${CODEPET_BIN_SOURCE:-}" ]; then
    BIN_SRC=$CODEPET_BIN_SOURCE
elif [ "$ARCHIVE" -eq 1 ]; then
    BIN_SRC=$ARCHIVE_BIN
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
if [ ! -f "$BIN_SRC" ] || [ -L "$BIN_SRC" ]; then
    die "no codepet binary at $BIN_SRC"
fi
got=$("$BIN_SRC" --version 2>/dev/null || true)
case "$got" in
    "codepet $SERIES".*) ;;
    *) die "$BIN_SRC reports '$got': this plugin needs a codepet $SERIES.x (the bundled UI and backend must match); nothing was changed" ;;
esac
[ "$got" = "codepet $VERSION" ] || die "$BIN_SRC reports '$got', expected 'codepet $VERSION'; nothing was changed"

# prune_backups <prefix> <kept>: after a successful swap, remove this script's earlier
# backups named $WORK/<prefix>* (trees, or the binary's single file), keeping only <kept>
# (the one just made), so repeated installs do not grow ~/.cache each time.
prune_backups() {
    local old
    for old in "$WORK/$1"*; do
        if [ "$old" = "$2" ] || [ -L "$old" ] || { [ ! -d "$old" ] && [ ! -f "$old" ]; }; then
            continue
        fi
        rm -rf -- "$old"
        echo "removed older backup: $old"
    done
}

# overlay_pids: only the desktop shell.qml entry, never the separate Work window.
overlay_pids() {
    command -v "$QS" >/dev/null 2>&1 || return 0
    "$QS" list --all --json 2>/dev/null | CP_DATA="$RUNTIME" awk '
        BEGIN { cur = ENVIRON["CP_DATA"] "/current/qml/shell.qml"; rel = ENVIRON["CP_DATA"] "/releases/" }
        /^[ \t]*\{/ { path = ""; pid = "" }
        /"config_path"[ \t]*:/ { l = $0; sub(/^[^:]*:[ \t]*"/, "", l); sub(/",?[ \t]*$/, "", l); path = l }
        /"pid"[ \t]*:/ { l = $0; sub(/^[^:]*:[ \t]*/, "", l); sub(/,?[ \t]*$/, "", l); if (l ~ /^[0-9]+$/) pid = l }
        /^[ \t]*\}/ { tail = substr(path, length(rel) + 1); if (pid != "" && (path == cur || (index(path, rel) == 1 && tail ~ /^[^/]+\/qml\/shell\.qml$/))) print pid }'
}
# supervisor_pids: CodePet 0.2 task supervisors, i.e. this user's processes running
# `<~/.local/bin/codepet> rpc` (a v0.2 `rpc` that started a task stays on as its
# supervisor). Matched on exact argv tokens from /proc, never by pattern.
supervisor_pids() {
    local d pid uid me args i
    me=$(id -u)
    for d in /proc/[0-9]*; do
        pid=${d#/proc/}
        [ "$pid" != "$$" ] || continue
        uid=$(awk '/^Uid:/ { print $2 }' "$d/status" 2>/dev/null) || continue
        [ "$uid" = "$me" ] || continue
        mapfile -d '' -t args <"$d/cmdline" 2>/dev/null || continue
        for ((i = 0; i + 1 < ${#args[@]} && i < 2; i++)); do
            if [ "${args[$i]}" = "$BIN_DIR/codepet" ] && [ "${args[$((i + 1))]}" = rpc ]; then
                echo "$pid"
                break
            fi
        done
    done
}
# stop_pid <pid> <what>: TERM, then KILL after about 3 s; exact pid only.
stop_pid() {
    local n
    kill -TERM "$1" 2>/dev/null || return 0
    for ((n = 0; n < 30; n++)); do
        kill -0 "$1" 2>/dev/null || { echo "stopped $2 (pid $1)"; return 0; }
        sleep 0.1
    done
    kill -KILL "$1" 2>/dev/null || true
    echo "stopped $2 (pid $1, forced)"
}

# 2. upgrade: stop the overlay and any 0.2 supervisor before files move under them
OVERLAY_RUNNING=0
if [ "$UPDATE" -eq 1 ]; then
    for pid in $(overlay_pids); do
        OVERLAY_RUNNING=1
        stop_pid "$pid" "the desktop overlay"
    done
    for pid in $(supervisor_pids); do
        stop_pid "$pid" "a CodePet 0.2 task supervisor"
    done
fi

# 3. binaries, each by temporary name then rename
install_file() { # install_file <src> <dest>
    install -m 0755 -- "$1" "$2.tmp-$TS"
    mv -f -- "$2.tmp-$TS" "$2"
}
mkdir -p -- "$BIN_DIR" "$WORK"
# The binary being replaced is kept, so a failed migration can be rolled back by hand.
BIN_BACKUP=
if [ -f "$BIN_DIR/codepet" ] && [ ! -L "$BIN_DIR/codepet" ]; then
    old=$("$BIN_DIR/codepet" --version 2>/dev/null || true)
    old=${old#codepet }
    [[ "$old" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || old=unknown
    BIN_BACKUP="$WORK/codepet-backup-$old-$TS"
    cp -p -- "$BIN_DIR/codepet" "$BIN_BACKUP"
    echo "binary: previous codepet ($old) kept at $BIN_BACKUP"
fi
install_file "$BIN_SRC" "$BIN_DIR/codepet"
got=$("$BIN_DIR/codepet" --version 2>/dev/null || true)
[ "$got" = "codepet $VERSION" ] || die "installed codepet reports '$got', expected 'codepet $VERSION'"
install_file "$REPO/$LAUNCHER_SOURCE" "$BIN_DIR/codepet-overlay"
install_file "$REPO/$WORK_LAUNCHER_SOURCE" "$BIN_DIR/codepet-work"
install_file "$REPO/$UPDATER_SOURCE" "$BIN_DIR/codepet-update"
echo "installed: $BIN_DIR/codepet ($got), $BIN_DIR/codepet-overlay, $BIN_DIR/codepet-work, $BIN_DIR/codepet-update"

# 4. overlay runtime
PREVIOUS=$(readlink -- "$RUNTIME/current" 2>/dev/null || true)
PREVIOUS=${PREVIOUS#releases/}
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
# Keep the new release and the previous one; remove older release trees, but never
# while an overlay still runs (it may be reading an older tree). Only release trees under
# releases/ are candidates: bin/, integrations/ and staging/ are never pruned.
STILL_RUNNING=$(overlay_pids)
for old in "$RUNTIME/releases/"*; do
    name=${old##*/}
    if [ "$name" = "$VERSION" ] || [ "$name" = "$PREVIOUS" ] || [ -L "$old" ] || [ ! -d "$old" ] ||
        ! [[ "$name" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        continue
    fi
    if [ -n "$STILL_RUNNING" ]; then
        echo "runtime: kept older release $old while an overlay runs"
        continue
    fi
    rm -rf -- "$old"
    echo "removed older release: $old"
done

# 5. bar plugin
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
PLUGIN_BACKUP=
if [ "$NO_PLUGIN" -eq 0 ]; then
    # Staged beside plugins/ (same filesystem, so every move below is one rename) but
    # outside it, so the shell's plugin watcher never sees a partial tree.
    stage_root="$CONFIG_HOME/omarchy/.codepet-stage-$TS"
    stage="$stage_root/$PLUGIN_ID"
    stage_plugin "$stage"
    no_symlinks "$stage"
    "$OMARCHY" plugin validate "$stage" || die "omarchy plugin validate refused the staged plugin ($stage)"
    mkdir -p -- "$PLUGINS"
    if [ -e "$PLUGIN_DIR" ]; then
        # One rename swaps old and new; the old tree, now at the stage path, is kept.
        if ! mv --exchange -T -- "$stage" "$PLUGIN_DIR" 2>/dev/null; then
            echo "$PROG: atomic exchange failed (unsupported by mv or the filesystem); replacing the plugin with two renames" >&2
            mv -T -- "$PLUGIN_DIR" "$stage.old"
            mv -T -- "$stage" "$PLUGIN_DIR"
            stage="$stage.old"
        fi
        mv -T -- "$stage" "$WORK/plugin-backup-$TS"
        PLUGIN_BACKUP="$WORK/plugin-backup-$TS"
        echo "plugin: updated $PLUGIN_DIR (previous copy: $WORK/plugin-backup-$TS)"
        prune_backups plugin-backup- "$WORK/plugin-backup-$TS"
        # Ask the running shell to rescan, so it reloads the swapped plugin now.
        if "$OMARCHY" shell shell rescanPlugins >/dev/null 2>&1; then
            echo "plugin: the running shell was asked to reload it"
        fi
    else
        mv -T -- "$stage" "$PLUGIN_DIR"
        echo "plugin: installed $PLUGIN_DIR"
    fi
    rmdir -- "$stage_root" 2>/dev/null || true
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

# 6. the pet's data: create or upgrade (schema 6); a refusal leaves it untouched
FAILED=0
if migrated=$("$BIN_DIR/codepet" migrate 2>&1); then
    echo "data: $migrated"
    [ -z "$BIN_BACKUP" ] || prune_backups codepet-backup- "$BIN_BACKUP"
else
    FAILED=1
    echo "$PROG: codepet migrate failed; the pet's data in $STATE_HOME/codepet was not changed:" >&2
    printf '%s\n' "$migrated" >&2
    # The new binary, runtime and plugin are in place; this is the way back.
    if [ -n "$BIN_BACKUP" ] || [ -n "$PLUGIN_BACKUP" ] || { [ -n "$PREVIOUS" ] && [ "$PREVIOUS" != "$VERSION" ]; }; then
        echo "To go back to the CodePet you had before, run:" >&2
        [ -z "$BIN_BACKUP" ] || printf '  install -m 0755 -- %q %q\n' "$BIN_BACKUP" "$BIN_DIR/codepet" >&2
        if [ -n "$PLUGIN_BACKUP" ]; then
            printf '  mv -T -- %q %q && mv -T -- %q %q\n' "$PLUGIN_DIR" "$WORK/plugin-$VERSION-$TS" \
                "$PLUGIN_BACKUP" "$PLUGIN_DIR" >&2
        fi
        if [ -n "$PREVIOUS" ] && [ "$PREVIOUS" != "$VERSION" ]; then
            printf '  ln -sfn -- %q %q\n' "releases/$PREVIOUS" "$RUNTIME/current" >&2
        fi
    else
        echo "Nothing to go back to: this was a first install. Fix the problem above and run the installer again." >&2
    fi
fi

# 7. connected coding apps: refresh CodePet-owned files whose bytes changed
if [ "$FAILED" -eq 0 ]; then
    if refreshed=$("$BIN_DIR/codepet" clients-refresh 2>&1); then
        [ -z "$refreshed" ] || printf 'apps: %s\n' "$refreshed"
    else
        echo "$PROG: warning: could not refresh every connected app's files (run: codepet doctor)" >&2
        [ -z "$refreshed" ] || printf '%s\n' "$refreshed" >&2
    fi
fi

# 8. upgrade: the overlay comes back if it was running
if [ "$OVERLAY_RUNNING" -eq 1 ]; then
    restart=("$BIN_DIR/codepet-overlay" show)
    if [ "${CODEPET_UPDATE_SERVICE:-0}" = 1 ]; then
        # An updater's control group is removed when it exits. Put the restored
        # desktop in its own service, whose lifetime follows every cgroup process,
        # including Quickshell after its launcher daemonizes and exits.
        restart=(systemd-run --user --quiet --collect --service-type=exec
            --unit="codepet-overlay-restored-$TS" --property=ExitType=cgroup
            --property=KillMode=control-group -- /usr/bin/env -i)
        for key in HOME PATH XDG_DATA_HOME XDG_CONFIG_HOME XDG_CACHE_HOME XDG_STATE_HOME XDG_RUNTIME_DIR DBUS_SESSION_BUS_ADDRESS WAYLAND_DISPLAY DISPLAY HYPRLAND_INSTANCE_SIGNATURE OMARCHY_PATH; do
            [ -z "${!key+x}" ] || restart+=("$key=${!key}")
        done
        restart+=("$BIN_DIR/codepet-overlay" show)
    fi
    if "${restart[@]}" >/dev/null 2>&1; then
        echo "overlay: started again"
    else
        echo "overlay: not started again; start it with: codepet-overlay show"
    fi
fi

if [ "$FAILED" -eq 0 ] && [ "$NO_PLUGIN" -eq 0 ]; then
    # Rescan acknowledges a request; only the loaded component can prove its bytes.
    if ui=$("$BIN_DIR/codepet-update" _record-ui) &&
        python3 -c 'import json,sys; sys.exit(json.load(sys.stdin).get("restart_required") is not False)' <<<"$ui"; then
        echo "plugin: the running bar confirmed UI build $BUILD_ID"
    else
        echo "plugin: installed files are ready; the running bar has not confirmed this UI build"
        echo "Open CodePet Updates and choose Restart bar to finish (only in an unlocked, idle session)."
        echo "Or run: codepet-update restart-bar"
    fi
fi
if [ "$UPDATE" -eq 0 ]; then
    if [ "$NO_PLUGIN" -eq 0 ]; then
        echo "Next: click CodePet on the bar (right section), pick a pet and tap Let's go!"
    else
        echo "Next: without the bar widget the pet's time stays frozen (see the README)."
    fi
    echo "Desktop pet: codepet-overlay show"
fi
if [ "$FAILED" -eq 0 ]; then
    echo "done: CodePet $VERSION is installed"
fi
exit "$FAILED"
