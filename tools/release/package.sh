#!/bin/bash
# package.sh: build and check the CodePet release archive for x86_64 Linux.
#
#   tools/release/package.sh [--out DIR] [--art-tar PATH]
#
# Writes into DIR (default target/release-dist):
#   codepet-<version>-x86_64-linux.tar.gz   the archive (layout below)
#   SHA256SUMS                              the archive, plus --art-tar when given
#   release-notes.md                        the CHANGELOG section, links pinned to the tag
#
# Archive layout (an explicit allowlist): bin/codepet, bin/codepet-pet, VERSION, LICENSE, README.md,
# CHANGELOG.md, THIRD-PARTY-LICENSES.md, docs/ (without the art-library source sheets),
# packaging/omarchy/, omarchy-plugin/, qml/{README.md,shell.qml,shared,omarchy,assets}
# and tools/measure-overlay.sh. packaging/omarchy/install.sh installs bin/ from it.
#
# The binaries are built with `cargo build --release --locked -p codepet -p codepet-pet -j 2` and
# --remap-path-prefix for the Cargo registry and the checkout, so no build-machine path
# is embedded; release builds strip symbols (Cargo.toml). Run this script inside a
# memory-capped scope (for example systemd-run --user --scope -p MemoryMax=4G); it
# starts no scope of its own. CODEPET_RELEASE_BIN skips the build and packages that
# binary instead (it must pass the same checks). CODEPET_RELEASE_OFFLINE=1 adds
# --offline to cargo. The archive is reproducible for a given commit: sorted names,
# owner 0:0, mtime = the HEAD commit time, gzip without a name or timestamp.
#
# Release gates (any failure stops the script): `git status --porcelain --ignored` reports
# nothing (no change, untracked or ignored file) under the shipped paths or the sources
# of the binary, so the archive holds exactly the commit; THIRD-PARTY-LICENSES.md is current;
# `bin/codepet --version` is `codepet <version>`; the binary is stripped and contains
# no /home/ path; every relative Markdown link in the archive resolves (missing
# targets are rewritten to tag-pinned GitHub URLs); every archive entry is owned by
# 0/0; the extracted archive passes `install.sh --dry-run`, a full install into a
# throwaway HOME (stub omarchy and qs; the plugin is validated with the real
# `omarchy plugin validate` when it exists; v0.3: `codepet migrate` creates the pet's data
# there and `codepet doctor --json` answers) and `uninstall.sh` (v0.3: with a stand-in hook
# shim, which it must replace by the frozen tombstone after disconnecting every app).
# The binary embeds the client integration templates (integrations/), so they count as
# its sources for the clean-tree gate.
set -euo pipefail

PROG=package.sh
die() {
    echo "$PROG: $*" >&2
    exit 1
}

REPO=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
OUT="$REPO/target/release-dist"
ART_TAR=
while [ "$#" -gt 0 ]; do
    case "$1" in
        --out) OUT=${2:?--out needs a directory}; shift 2 ;;
        --art-tar) ART_TAR=${2:?--art-tar needs a path}; shift 2 ;;
        -h|--help) sed -n '2,33p' "${BASH_SOURCE[0]}"; exit 0 ;;
        *) die "unknown argument: $1" ;;
    esac
done
case "$OUT" in /*) ;; *) OUT="$PWD/$OUT" ;; esac

VERSION=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' \
    "$REPO/Cargo.toml" | head -n 1) || VERSION=
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "cannot read the workspace version"
NAME="codepet-$VERSION-x86_64-linux"
EPOCH=$(git -C "$REPO" log -1 --format=%ct)
OFFLINE=()
[ "${CODEPET_RELEASE_OFFLINE:-0}" = 1 ] && OFFLINE=(--offline)
if [ -n "$ART_TAR" ]; then
    [ -f "$ART_TAR" ] || die "no art tar at $ART_TAR"
    [ "$(basename -- "$ART_TAR")" = codepet-art-sources-v0.2.tar ] || die "unexpected art tar name: $ART_TAR"
fi

echo "== CodePet $VERSION release archive"
# 0. the shipped paths hold exactly the commit: nothing modified, untracked or ignored
# (a stray __pycache__/ or editor file would otherwise be copied into the archive)
SHIPPED=(LICENSE README.md CHANGELOG.md THIRD-PARTY-LICENSES.md docs packaging/omarchy
    omarchy-plugin qml/README.md qml/shell.qml qml/work.qml qml/shared qml/omarchy qml/assets
    tools/measure-overlay.sh Cargo.toml Cargo.lock crates integrations)
dirty=$(git -C "$REPO" status --porcelain --ignored --untracked-files=all -- "${SHIPPED[@]}") ||
    die "git status failed"
if [ -n "$dirty" ]; then
    printf '%s\n' "$dirty" >&2
    die "the shipped paths are not exactly the commit (listed above); commit or remove them"
fi
# 1. third-party notices match the lock graph
python3 "$REPO/tools/release/third_party_licenses.py" --check "${OFFLINE[@]}"

# 2. binary
if [ -n "${CODEPET_RELEASE_BIN:-}" ]; then
    BIN=$CODEPET_RELEASE_BIN
else
    CARGO_HOME_DIR=${CARGO_HOME:-$HOME/.cargo}
    (cd "$REPO" && RUSTFLAGS="--remap-path-prefix=$CARGO_HOME_DIR/registry/src=/cargo/registry/src --remap-path-prefix=$CARGO_HOME_DIR/git/checkouts=/cargo/git --remap-path-prefix=$REPO=/codepet" \
        cargo build --release --locked -p codepet -p codepet-pet -j 2 "${OFFLINE[@]}")
    BIN="${CARGO_TARGET_DIR:-$REPO/target}/release/codepet"
fi
PET=${CODEPET_RELEASE_PET:-$(dirname -- "$BIN")/codepet-pet}
[ -f "$BIN" ] && [ ! -L "$BIN" ] || die "no codepet binary at $BIN"
got=$("$BIN" --version)
[ "$got" = "codepet $VERSION" ] || die "binary reports '$got', expected 'codepet $VERSION'"
if LC_ALL=C grep -a -q -e /home/ -e "$HOME" -- "$BIN"; then
    die "the binary embeds a home-directory path (build with --remap-path-prefix)"
fi
if command -v file >/dev/null 2>&1; then
    case "$(file -b -- "$BIN")" in
        *"not stripped"*) die "the binary is not stripped" ;;
    esac
fi
echo "binary: $got, stripped, no home paths"
[ -f "$PET" ] && [ ! -L "$PET" ] || die "no codepet-pet binary at $PET"
got_pet=$("$PET" --version)
[ "$got_pet" = "codepet-pet $VERSION" ] || die "pet binary reports '$got_pet', expected 'codepet-pet $VERSION'"
if LC_ALL=C grep -a -q -e /home/ -e "$HOME" -- "$PET"; then
    die "the pet binary embeds a home-directory path (build with --remap-path-prefix)"
fi
if command -v file >/dev/null 2>&1; then
    case "$(file -b -- "$PET")" in
        *"not stripped"*) die "the pet binary is not stripped" ;;
    esac
fi
echo "pet binary: $got_pet, stripped, no home paths"

# 3. stage the allowlist
rm -rf -- "$OUT/stage"
STAGE="$OUT/stage/$NAME"
mkdir -p -- "$STAGE/bin" "$STAGE/packaging" "$STAGE/qml" "$STAGE/tools"
install -m 0755 -- "$BIN" "$STAGE/bin/codepet"
install -m 0755 -- "$PET" "$STAGE/bin/codepet-pet"
printf '%s\n' "$VERSION" > "$STAGE/VERSION"
for f in LICENSE README.md CHANGELOG.md THIRD-PARTY-LICENSES.md; do
    cp -- "$REPO/$f" "$STAGE/$f"
done
cp -R -- "$REPO/docs" "$STAGE/docs"
rm -rf -- "$STAGE/docs/art-library/source"
cp -R -- "$REPO/packaging/omarchy" "$STAGE/packaging/omarchy"
cp -R -- "$REPO/omarchy-plugin" "$STAGE/omarchy-plugin"
cp -- "$REPO/qml/README.md" "$REPO/qml/shell.qml" "$REPO/qml/work.qml" "$STAGE/qml/"
for d in shared omarchy assets; do
    cp -R -- "$REPO/qml/$d" "$STAGE/qml/$d"
done
install -m 0755 -- "$REPO/tools/measure-overlay.sh" "$STAGE/tools/measure-overlay.sh"
bad=$(find "$STAGE" ! -type f ! -type d -print)
[ -z "$bad" ] || die "staged tree holds a non-regular file: $bad"
python3 "$REPO/tools/release/release_docs.py" stage-links --version "$VERSION" --stage "$STAGE"

# 4. archive, notes, checksums
ARCHIVE="$OUT/$NAME.tar.gz"
tar --owner=0 --group=0 --numeric-owner --sort=name --mtime="@$EPOCH" \
    --mode='u+rwX,go+rX,go-w' -C "$OUT/stage" -cf - "$NAME" | gzip -n -9 > "$ARCHIVE.tmp"
mv -f -- "$ARCHIVE.tmp" "$ARCHIVE"
owners=$(tar -tvzf "$ARCHIVE" | awk '$2 != "0/0" { print $6 }')
[ -z "$owners" ] || die "archive entries not owned by 0/0: $owners"
python3 "$REPO/tools/release/release_docs.py" notes --version "$VERSION" --out "$OUT/release-notes.md"
(cd "$OUT" && sha256sum -- "$NAME.tar.gz") > "$OUT/SHA256SUMS"
if [ -n "$ART_TAR" ]; then
    (cd "$(dirname -- "$ART_TAR")" && sha256sum -- "$(basename -- "$ART_TAR")") >> "$OUT/SHA256SUMS"
fi

# 5. install gate: the extracted archive in a throwaway HOME, stub omarchy and qs
GATE=$(mktemp -d "${TMPDIR:-/tmp}/codepet-release-gate.XXXXXX")
trap 'rm -rf -- "$GATE"' EXIT
mkdir -p -- "$GATE/home" "$GATE/stubs" "$GATE/x"
tar -xzf "$ARCHIVE" -C "$GATE/x"
cat > "$GATE/stubs/omarchy" <<'STUB'
#!/bin/sh
# Release-gate stand-in: validates with the real validator when present; no shell.
case "$1 $2" in
  "plugin validate")
    [ "$3" = --help ] && exit 0
    if [ -x /usr/share/omarchy/bin/omarchy-plugin-validate ]; then
        exec /usr/share/omarchy/bin/omarchy-plugin-validate "$3"
    fi
    [ -f "$3/manifest.json" ] ;;
  *) exit 1 ;;
esac
STUB
cat > "$GATE/stubs/qs" <<'STUB'
#!/bin/sh
[ "$1" = list ] && { echo "No running instances."; exit 0; }
exit 1
STUB
chmod 0755 "$GATE/stubs/omarchy" "$GATE/stubs/qs"
H="$GATE/home"
gate() {
    env -i PATH="$GATE/stubs:/usr/bin:/bin" HOME="$H" LC_ALL=C TMPDIR="$GATE" \
        XDG_CONFIG_HOME="$H/.config" XDG_DATA_HOME="$H/.local/share" XDG_CACHE_HOME="$H/.cache" \
        XDG_STATE_HOME="$H/.local/state" CODEPET_OMARCHY="$GATE/stubs/omarchy" CODEPET_QS="$GATE/stubs/qs" \
        "$@"
}
X="$GATE/x/$NAME"
gate "$X/packaging/omarchy/install.sh" --dry-run > "$GATE/dry.log" ||
    die "archive install.sh --dry-run failed: $(cat "$GATE/dry.log")"
grep -q "would use prebuilt: $X/bin/codepet" "$GATE/dry.log" || die "the dry run does not use the bundled binary"
gate "$X/packaging/omarchy/install.sh" > "$GATE/install.log" 2>&1 ||
    die "archive install failed: $(cat "$GATE/install.log")"
[ "$(gate "$H/.local/bin/codepet" --version)" = "codepet $VERSION" ] || die "installed binary has the wrong version"
[ "$(gate "$H/.local/bin/codepet-pet" --version)" = "codepet-pet $VERSION" ] || die "installed pet binary has the wrong version"
[ -f "$H/.config/omarchy/plugins/greyforge.codepet/manifest.json" ] || die "the bar plugin was not installed"
[ -f "$H/.local/state/codepet/codepet.sqlite3" ] || die "the install did not migrate the pet's data"
gate "$H/.local/bin/codepet" doctor --json | grep -q '"schema":"codepet.doctor.v1"' ||
    die "codepet doctor --json did not answer"
mkdir -p -- "$H/.local/share/codepet/bin"
printf '#!/bin/sh\n# managed by codepet: release-gate stand-in\nexit 0\n' > "$H/.local/share/codepet/bin/codepet-hook"
chmod 0755 "$H/.local/share/codepet/bin/codepet-hook"
gate "$X/packaging/omarchy/uninstall.sh" > "$GATE/uninstall.log" 2>&1 ||
    die "archive uninstall failed: $(cat "$GATE/uninstall.log")"
[ ! -e "$H/.config/omarchy/plugins/greyforge.codepet" ] || die "uninstall left the plugin in place"
cmp -s -- "$H/.local/share/codepet/bin/codepet-hook" "$X/docs/v0.3/contracts/shim-tombstone.sh" ||
    die "uninstall did not replace the hook shim by the tombstone"
echo "gate: archive dry run, install (migrate, doctor) and uninstall (disconnect, tombstone) in a throwaway HOME passed"

rm -rf -- "$OUT/stage"
echo "== done"
cat "$OUT/SHA256SUMS"
echo "notes: $OUT/release-notes.md"
