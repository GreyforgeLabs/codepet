#!/usr/bin/env python3
"""Validate original artwork and passive renderer without desktop integration."""
from pathlib import Path
from PySide6.QtSvg import QSvgRenderer
root = Path(__file__).resolve().parents[1]
assets = root / 'assets'
files = sorted(assets.glob('ginger-*.svg')) + sorted(assets.glob('prop-*.svg'))
assert len(files) == 36
for path in files:
    assert QSvgRenderer(str(path)).isValid(), path
    text = path.read_text()
    assert '<script' not in text and 'href=' not in text and '<animate' not in text, path
renderer = (root / 'shared/PetSprite.qml').read_text()
assert 'Timer {' not in renderer and '.request(' not in renderer
print('PASS: 36 self-contained SVG assets and passive renderer')
