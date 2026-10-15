"""Validate source catalog integrity; does not validate animation readiness.

Every check is an explicit comparison that records an error, never a bare
``assert``, so ``python3 -O`` and ``PYTHONOPTIMIZE`` cannot disable them.
The script exits 1 and lists every failure when anything does not match.
"""

import hashlib
import json
from pathlib import Path
import struct
import sys

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
UNAVAILABLE_SOURCE = "not in repository; hash unavailable"
HEX_DIGITS = frozenset("0123456789abcdef")
# IHDR colour type 6 is truecolour with alpha; the manifest records it as RGBA.
PNG_COLOUR_TYPES = {"RGBA": 6, "RGB": 2, "LA": 4, "L": 0}
# Text files that belong to the catalog itself; anything else must be a listed PNG.
CATALOG_FILES = frozenset({"README.md", "manifest.json", "verify.py", "test_verify.py"})


def is_sha256(value):
    return isinstance(value, str) and len(value) == 64 and set(value) <= HEX_DIGITS


class Checks:
    def __init__(self):
        self.errors = []

    def require(self, condition, message):
        if not condition:
            self.errors.append(message)
        return bool(condition)


def alpha_statistics(path):
    """Recompute the manifest's alpha block with Pillow (optional --alpha check)."""
    from PIL import Image  # imported lazily: the default checks need only the stdlib

    with Image.open(path) as image:
        histogram = image.getchannel("A").histogram()
    pixels = sum(histogram)
    used = [value for value, count in enumerate(histogram) if count]
    transparent, opaque = histogram[0], histogram[255]
    return {
        "present": True,
        "extrema": [used[0], used[-1]],
        "fully_transparent_pixels": transparent,
        "fully_opaque_pixels": opaque,
        "fully_transparent_fraction": transparent / pixels,
        "partially_transparent_pixels": pixels - transparent - opaque,
    }


def catalog_inventory(root):
    """Every file under the catalog except Python bytecode caches."""
    return {
        p.relative_to(root).as_posix()
        for p in root.rglob("*")
        if not p.is_dir() and "__pycache__" not in p.relative_to(root).parts
    }


def verify(root, check_alpha=False):
    checks = Checks()
    try:
        manifest = json.loads((root / "manifest.json").read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        checks.require(False, f"manifest.json unreadable: {error}")
        return checks.errors, 0, 0

    assets = manifest.get("assets")
    if not checks.require(isinstance(assets, list), "manifest assets must be a list"):
        return checks.errors, 0, 0
    checks.require(
        len(assets) == manifest.get("selected_png_count") == 15,
        f"expected 15 selected PNGs, manifest lists {len(assets)} "
        f"and selected_png_count={manifest.get('selected_png_count')!r}",
    )
    checks.require(
        len({a.get("species") for a in assets}) == manifest.get("species_count"),
        "species count does not match species_count",
    )
    checks.require(
        len({a.get("relative_path") for a in assets}) == len(assets),
        "relative paths are not unique",
    )

    pose_total = 0
    total = 0
    listed = set()
    for index, asset in enumerate(assets):
        label = asset.get("relative_path") or f"assets[{index}]"
        relative = Path(str(asset.get("relative_path", "")))
        if not checks.require(
            asset.get("relative_path")
            and not relative.is_absolute()
            and ".." not in relative.parts,
            f"{label}: path must be relative and stay inside the catalog",
        ):
            continue
        path = (root / relative).resolve()
        if not checks.require(path.is_relative_to(root), f"{label}: path escapes the catalog"):
            continue
        listed.add(relative.as_posix())
        checks.require(
            relative.parts[:3] == ("source", asset.get("species"), asset.get("version"))
            and len(relative.parts) == 4,
            f"{label}: path must be source/<species>/<version>/<file> matching the manifest",
        )
        grid = asset.get("intended_grid") or {}
        roles = grid.get("roles")
        columns, rows, poses = grid.get("columns"), grid.get("rows"), grid.get("pose_count")
        checks.require(
            isinstance(roles, list)
            and isinstance(columns, int)
            and isinstance(rows, int)
            and len(roles) == columns * rows == poses,
            f"{label}: grid roles, columns x rows and pose_count disagree",
        )
        pose_total += poses if isinstance(poses, int) else 0
        try:
            data = path.read_bytes()
        except OSError as error:
            checks.require(False, f"{label}: unreadable: {error}")
            continue
        checks.require(data[:8] == PNG_SIGNATURE, f"{label}: not a PNG signature")
        checks.require(data[12:16] == b"IHDR", f"{label}: first chunk is not IHDR")
        dimensions = struct.unpack(">II", data[16:24]) if len(data) >= 24 else None
        bit_depth, colour_type = (data[24], data[25]) if len(data) >= 26 else (None, None)
        checks.require(bit_depth == 8, f"{label}: IHDR bit depth {bit_depth} is not 8")
        checks.require(
            colour_type is not None and colour_type == PNG_COLOUR_TYPES.get(asset.get("mode")),
            f"{label}: IHDR colour type {colour_type} does not match mode {asset.get('mode')!r}",
        )
        checks.require(
            dimensions == (asset.get("width"), asset.get("height")),
            f"{label}: dimensions {dimensions} do not match the manifest",
        )
        checks.require(len(data) == asset.get("size_bytes"), f"{label}: size does not match")
        checks.require(
            hashlib.sha256(data).hexdigest() == asset.get("sha256"),
            f"{label}: SHA-256 does not match",
        )
        checks.require(asset.get("runtime_ready") is False, f"{label}: runtime_ready must be false")
        checks.require(
            bool(asset.get("provenance")) and bool(asset.get("visual_qa_from_generator")),
            f"{label}: provenance and QA notes are required",
        )
        for step_index, step in enumerate(asset.get("provenance") or []):
            if step.get("kind") != "imagegen_edit":
                continue
            where = f"{label}: provenance[{step_index}]"
            source = step.get("edit_source")
            digest = step.get("edit_source_sha256")
            checks.require(
                isinstance(source, str) and source.strip(),
                f"{where}: edit steps must name their edit_source",
            )
            checks.require(
                digest == UNAVAILABLE_SOURCE or is_sha256(digest),
                f"{where}: edit_source_sha256 must be a SHA-256 or {UNAVAILABLE_SOURCE!r}",
            )
            if isinstance(source, str) and source.startswith("source/"):
                checks.require(
                    (root / source).is_file(),
                    f"{where}: edit_source {source} does not exist in the catalog",
                )
        if check_alpha and data[:8] == PNG_SIGNATURE:
            try:
                measured = alpha_statistics(path)
            except Exception as error:  # noqa: BLE001 - any decode failure is a failed check
                checks.require(False, f"{label}: alpha recheck failed: {error}")
            else:
                checks.require(
                    measured == asset.get("alpha"),
                    f"{label}: alpha statistics {measured} do not match the manifest",
                )
        total += len(data)

    checks.require(
        pose_total == manifest.get("intended_pose_count"),
        f"grid pose counts sum to {pose_total}, not intended_pose_count",
    )
    on_disk = catalog_inventory(root)
    expected = listed | CATALOG_FILES
    folded = {}
    for name in on_disk:
        folded.setdefault(name.casefold(), []).append(name)
    checks.require(
        all(len(names) == 1 for names in folded.values()),
        f"file names differ only by case: {sorted(n for n in folded.values() if len(n) > 1)}",
    )
    expected_folded = {name.casefold() for name in expected}
    unlisted = sorted(name for name in on_disk if name.casefold() not in expected_folded)
    missing = sorted(name for name in expected if name.casefold() not in folded)
    checks.require(
        not unlisted and not missing and on_disk == expected,
        f"files on disk differ from the manifest: unlisted={unlisted} missing={missing}",
    )
    checks.require(
        total == manifest.get("total_selected_png_bytes"),
        f"total bytes {total} do not match total_selected_png_bytes",
    )
    return checks.errors, len(assets), total


def main(argv):
    check_alpha = "--alpha" in argv[1:]
    unknown = [arg for arg in argv[1:] if arg != "--alpha"]
    if unknown:
        print(f"usage: verify.py [--alpha]; unknown arguments {unknown}", file=sys.stderr)
        return 2
    if check_alpha:
        try:
            import PIL  # noqa: F401
        except ImportError:
            print("FAIL: --alpha needs Pillow, which is not installed", file=sys.stderr)
            return 1
    root = Path(__file__).resolve().parent
    errors, count, total = verify(root, check_alpha)
    if errors:
        for error in errors:
            print(f"FAIL: {error}", file=sys.stderr)
        print(f"FAIL: {len(errors)} source-art catalog check(s) failed", file=sys.stderr)
        return 1
    alpha = "; alpha statistics recomputed" if check_alpha else ""
    print(
        f"PASS: {count} source PNGs; {total:,} bytes; dimensions, paths and SHA-256 verified{alpha}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
