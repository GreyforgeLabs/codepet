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


def is_sha256(value):
    return isinstance(value, str) and len(value) == 64 and set(value) <= HEX_DIGITS


class Checks:
    def __init__(self):
        self.errors = []

    def require(self, condition, message):
        if not condition:
            self.errors.append(message)
        return bool(condition)


def verify(root):
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
        try:
            data = path.read_bytes()
        except OSError as error:
            checks.require(False, f"{label}: unreadable: {error}")
            continue
        checks.require(data[:8] == PNG_SIGNATURE, f"{label}: not a PNG signature")
        checks.require(data[12:16] == b"IHDR", f"{label}: first chunk is not IHDR")
        dimensions = struct.unpack(">II", data[16:24]) if len(data) >= 24 else None
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
        total += len(data)

    on_disk = {p.relative_to(root).as_posix() for p in root.rglob("*.png")}
    checks.require(
        on_disk == listed,
        f"PNG files on disk differ from the manifest: "
        f"unlisted={sorted(on_disk - listed)} missing={sorted(listed - on_disk)}",
    )
    checks.require(
        total == manifest.get("total_selected_png_bytes"),
        f"total bytes {total} do not match total_selected_png_bytes",
    )
    return checks.errors, len(assets), total


def main():
    root = Path(__file__).resolve().parent
    errors, count, total = verify(root)
    if errors:
        for error in errors:
            print(f"FAIL: {error}", file=sys.stderr)
        print(f"FAIL: {len(errors)} source-art catalog check(s) failed", file=sys.stderr)
        return 1
    print(f"PASS: {count} source PNGs; {total:,} bytes; dimensions, paths and SHA-256 verified")
    return 0


if __name__ == "__main__":
    sys.exit(main())
