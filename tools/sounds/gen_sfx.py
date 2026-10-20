#!/usr/bin/env python3
"""Original CodePet care sounds.

Synthesized in this file. No recordings, sample packs or third-party audio.
Writes omarchy-plugin/assets/sounds/action/{food,water,play,wash,pet,treat}.wav.

0.6 redesign (owner review: the 0.5.1 clicks sounded odd): the square-wave and noise
"animal voices" are gone. Every cue is one short, soft, musical sound: mallet notes
(marimba-like partials with exponential decay), a water-drop bloop and bubble pops.
Each cue stays under half a second and well below full scale.
"""
from __future__ import annotations

import math
import struct
import wave
from pathlib import Path

SR = 22050
ROOT = Path(__file__).resolve().parents[2] / "omarchy-plugin" / "assets" / "sounds"
PEAK = 0.55  # normalised peak; pw-play adds its own 0.4 volume on top


def note(name: str) -> float:
    """Frequency of a note name such as C5, F#5 or Bb4 (A4 = 440 Hz)."""
    steps = {"C": -9, "D": -7, "E": -5, "F": -4, "G": -2, "A": 0, "B": 2}
    letter, rest = name[0], name[1:]
    shift = 0
    while rest and rest[0] in "#b":
        shift += 1 if rest[0] == "#" else -1
        rest = rest[1:]
    octave = int(rest)
    return 440.0 * 2 ** ((steps[letter] + shift + 12 * (octave - 4)) / 12)


def blank(dur: float) -> list[float]:
    return [0.0] * int(dur * SR)


def add(buf: list[float], part: list[float], at: float) -> None:
    start = int(at * SR)
    need = start + len(part)
    if need > len(buf):
        buf.extend([0.0] * (need - len(buf)))
    for i, s in enumerate(part):
        buf[start + i] += s


def mallet(freq: float, dur: float = 0.32, vol: float = 0.5, bright: float = 0.35) -> list[float]:
    """A soft marimba-like note: fundamental plus a quieter 4x partial, 3 ms attack."""
    n = int(dur * SR)
    out = []
    for i in range(n):
        t = i / SR
        attack = min(1.0, t / 0.003)
        body = math.sin(2 * math.pi * freq * t) * math.exp(-t * 9.0)
        ring = math.sin(2 * math.pi * freq * 3.98 * t) * math.exp(-t * 28.0) * bright
        out.append(vol * attack * (body + ring))
    return out


def bell(freq: float, dur: float = 0.45, vol: float = 0.35) -> list[float]:
    """A small glassy bell: inharmonic partials with long-ish decay, for the treat sparkle."""
    partials = ((1.0, 1.0, 6.0), (2.76, 0.35, 10.0), (5.4, 0.15, 16.0))
    n = int(dur * SR)
    out = []
    for i in range(n):
        t = i / SR
        attack = min(1.0, t / 0.002)
        s = sum(a * math.sin(2 * math.pi * freq * m * t) * math.exp(-t * d) for m, a, d in partials)
        out.append(vol * attack * s)
    return out


def bloop(f0: float, f1: float, dur: float = 0.12, vol: float = 0.5) -> list[float]:
    """A water-drop / bubble pop: a sine whose pitch rises quickly, with a fast decay."""
    n = int(dur * SR)
    out = []
    phase = 0.0
    for i in range(n):
        t = i / SR
        k = min(1.0, t / (dur * 0.55))
        f = f0 + (f1 - f0) * (1 - (1 - k) ** 2)
        phase += 2 * math.pi * f / SR
        env = min(1.0, t / 0.002) * math.exp(-t * 26.0)
        out.append(vol * env * math.sin(phase))
    return out


def lowpass(samples: list[float], cutoff: float = 5200.0) -> list[float]:
    """One-pole low-pass to take the digital edge off."""
    rc = 1.0 / (2 * math.pi * cutoff)
    a = (1.0 / SR) / (rc + 1.0 / SR)
    out, y = [], 0.0
    for s in samples:
        y += a * (s - y)
        out.append(y)
    return out


def finish(samples: list[float]) -> list[float]:
    samples = lowpass(samples)
    fade = int(0.012 * SR)
    for i in range(min(fade, len(samples))):
        samples[-1 - i] *= i / fade
    peak = max(1e-6, max(abs(s) for s in samples))
    return [s * PEAK / peak for s in samples]


def action(name: str) -> list[float]:
    buf = blank(0.05)
    if name == "pet":  # a warm two-note "boop-boop", rising a fourth
        add(buf, mallet(note("G5"), 0.28, 0.45), 0.0)
        add(buf, mallet(note("C6"), 0.34, 0.45), 0.09)
    elif name == "food":  # three quick happy notes: yum
        for k, (n, at) in enumerate((("E5", 0.0), ("G5", 0.075), ("C6", 0.15))):
            add(buf, mallet(note(n), 0.26 if k < 2 else 0.34, 0.45), at)
    elif name == "water":  # one clean water drop
        add(buf, bloop(420, 1250, 0.13, 0.6), 0.0)
        add(buf, bloop(900, 1500, 0.06, 0.15), 0.07)
    elif name == "play":  # a bouncy rising arpeggio
        for k, n in enumerate(("C5", "E5", "G5", "C6")):
            add(buf, mallet(note(n), 0.22, 0.4, 0.5), k * 0.06)
    elif name == "wash":  # three soft bubble pops at different heights
        for f0, f1, at in ((500, 1100, 0.0), (700, 1500, 0.08), (600, 1300, 0.15)):
            add(buf, bloop(f0, f1, 0.09, 0.45), at)
    elif name == "treat":  # a small sparkle
        for k, n in enumerate(("C6", "E6", "G6", "C7")):
            add(buf, bell(note(n), 0.4 if k == 3 else 0.25, 0.3), k * 0.055)
    else:
        raise KeyError(name)
    return finish(buf)


def save(path: Path, samples: list[float]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), "w") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(b"".join(struct.pack("<h", int(max(-1.0, min(1.0, s)) * 32767)) for s in samples))


def main() -> None:
    for name in ("food", "water", "play", "wash", "pet", "treat"):
        save(ROOT / "action" / f"{name}.wav", action(name))
    print(f"wrote sounds under {ROOT / 'action'}")


if __name__ == "__main__":
    main()
