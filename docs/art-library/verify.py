"""Validate source catalog integrity; does not validate animation readiness."""

import hashlib
import json
from pathlib import Path
import struct


def main():
    root = Path(__file__).resolve().parent
    manifest = json.loads((root / "manifest.json").read_text())
    assets = manifest["assets"]
    assert len(assets) == manifest["selected_png_count"] == 15
    assert len({a["species"] for a in assets}) == manifest["species_count"]
    assert len({a["relative_path"] for a in assets}) == len(assets)
    total = 0
    for asset in assets:
        relative = Path(asset["relative_path"])
        assert not relative.is_absolute() and ".." not in relative.parts
        path = (root / relative).resolve()
        assert path.is_relative_to(root)
        data = path.read_bytes()
        assert data[:8] == b"\x89PNG\r\n\x1a\n", relative
        assert data[12:16] == b"IHDR", relative
        assert struct.unpack(">II", data[16:24]) == (asset["width"], asset["height"]), relative
        assert len(data) == asset["size_bytes"], relative
        assert hashlib.sha256(data).hexdigest() == asset["sha256"], relative
        assert asset["runtime_ready"] is False, relative
        assert asset["provenance"] and asset["visual_qa_from_generator"], relative
        total += len(data)
    assert {p.relative_to(root).as_posix() for p in root.rglob("*.png")} == {
        a["relative_path"] for a in assets
    }
    assert total == manifest["total_selected_png_bytes"]
    print(f"PASS: {len(assets)} source PNGs; {total:,} bytes; dimensions, paths and SHA-256 verified")


if __name__ == "__main__":
    main()
