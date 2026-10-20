#!/usr/bin/env python3
"""Original CodePet care sounds.

Synthesized in this file. No recordings, sample packs or third-party audio.
Writes omarchy-plugin/assets/sounds/voice/<species>/{0,1,2}.wav and
assets/sounds/action/{food,water,play,wash,pet,treat}.wav.
"""
from __future__ import annotations

import math
import struct
import wave
from pathlib import Path

SR = 22050
ROOT = Path(__file__).resolve().parents[2] / "omarchy-plugin" / "assets" / "sounds"
TAKES = (0.90, 1.0, 1.12)


def clamp(x: float) -> float:
    return max(-1.0, min(1.0, x))


def env(i: int, n: int, attack: float = 0.012, release: float = 0.04) -> float:
    t = i / SR
    dur = n / SR
    a = min(1.0, t / attack) if attack else 1.0
    r = min(1.0, max(0.0, dur - t) / release) if release else 1.0
    return a * r


def save(path: Path, samples: list[float]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    peak = max(1e-6, max(abs(s) for s in samples))
    gain = min(1.0, 0.9 / peak)
    with wave.open(str(path), "w") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        frames = b"".join(
            struct.pack("<h", int(clamp(s * gain) * 32767)) for s in samples
        )
        w.writeframes(frames)


def mix(*layers: list[float]) -> list[float]:
    n = max(len(layer) for layer in layers)
    out = [0.0] * n
    for layer in layers:
        for i, s in enumerate(layer):
            out[i] += s
    return out


def tone(freq: float, dur: float, vol: float = 0.4, kind: str = "sine", slide: float = 0.0) -> list[float]:
    n = max(1, int(dur * SR))
    out = []
    phase = 0.0
    for i in range(n):
        f = freq * (1.0 + slide * (i / n))
        phase += 2 * math.pi * f / SR
        if kind == "square":
            s = 1.0 if math.sin(phase) >= 0 else -1.0
        elif kind == "tri":
            s = 2 * abs((phase / math.pi) % 2 - 1) - 1
        else:
            s = math.sin(phase)
        out.append(s * vol * env(i, n, 0.008, min(0.08, dur * 0.45)))
    return out


def noise(dur: float, vol: float = 0.25, seed: int = 1, hipass: float = 0.0) -> list[float]:
    n = max(1, int(dur * SR))
    state = seed & 0x7FFFFFFF or 1
    out = []
    prev = 0.0
    for i in range(n):
        state = (1103515245 * state + 12345) & 0x7FFFFFFF
        s = (state / 0x3FFFFFFF) - 1.0
        if hipass:
            s = s - prev
            prev = (state / 0x3FFFFFFF) - 1.0
            s *= hipass
        out.append(s * vol * env(i, n, 0.005, min(0.06, dur * 0.4)))
    return out


def silence(dur: float) -> list[float]:
    return [0.0] * int(dur * SR)


def seq(*parts: list[float]) -> list[float]:
    out: list[float] = []
    for part in parts:
        out.extend(part)
    return out


def pitched(samples: list[float], factor: float) -> list[float]:
    if factor == 1.0:
        return samples
    n = max(1, int(len(samples) / factor))
    out = []
    for i in range(n):
        x = i * factor
        j = int(x)
        f = x - j
        a = samples[j] if j < len(samples) else 0.0
        b = samples[j + 1] if j + 1 < len(samples) else 0.0
        out.append(a * (1 - f) + b * f)
    return out


def voice(kind: str, take: int) -> list[float]:
    factor = TAKES[take]
    extra = take == 2
    if kind == "cat":
        base = seq(tone(720, 0.09, 0.45, slide=-0.25), tone(540, 0.16, 0.4, slide=-0.15))
        if extra:
            base = seq(base, silence(0.04), tone(880, 0.08, 0.3, slide=0.2))
    elif kind == "dog":
        base = seq(noise(0.07, 0.55, 3, 0.8), tone(180, 0.12, 0.5, "tri", -0.3))
        if extra:
            base = seq(base, silence(0.05), noise(0.06, 0.4, 9, 0.8))
    elif kind == "capybara":
        click = seq(tone(220, 0.04, 0.45, "tri"), silence(0.04))
        base = click * (4 if extra else 3)
    elif kind == "penguin":
        base = seq(tone(420, 0.12, 0.5, "square", -0.1), tone(300, 0.1, 0.35, "square"))
    elif kind == "robot":
        notes = (660, 880, 1174) if extra else (880, 1174)
        base = seq(*(seq(tone(f, 0.06, 0.4, "square"), silence(0.03)) for f in notes))
    elif kind == "iguana":
        base = noise(0.28 if extra else 0.2, 0.35, 11, 1.2)
    elif kind == "spider":
        tick = seq(tone(2400, 0.02, 0.35), silence(0.035))
        base = tick * (5 if extra else 3)
    elif kind == "snake":
        base = noise(0.32 if extra else 0.22, 0.3, 21, 1.4)
    elif kind == "goldfish":
        blip = seq(tone(500, 0.07, 0.4, slide=0.8), silence(0.05))
        base = blip * (3 if extra else 2)
    elif kind == "rabbit":
        base = seq(tone(1400, 0.05, 0.35, slide=0.4), tone(1800, 0.07, 0.3, slide=-0.3))
    elif kind == "turtle":
        base = tone(160, 0.34 if extra else 0.26, 0.45, "sine", -0.08)
    elif kind == "frog":
        base = seq(tone(240, 0.08, 0.5, "tri", -0.4), silence(0.04), tone(160, 0.12, 0.45, "tri", -0.2))
        if extra:
            base = seq(base, silence(0.05), tone(200, 0.08, 0.35, "tri"))
    elif kind == "owl":
        base = seq(tone(340, 0.18, 0.45, slide=-0.05), silence(0.06), tone(280, 0.2, 0.4))
    elif kind == "red_panda":
        chirp = seq(tone(2100, 0.04, 0.35, slide=0.5), silence(0.03))
        base = chirp * (5 if extra else 3)
    elif kind == "axolotl":
        base = seq(tone(480, 0.14, 0.4, slide=0.35), tone(360, 0.12, 0.3, slide=-0.2))
    else:
        base = tone(440, 0.12, 0.3)
    return pitched(base, factor)


def action(name: str) -> list[float]:
    if name == "food":
        return seq(noise(0.05, 0.5, 4, 0.6), silence(0.03), noise(0.07, 0.4, 8, 0.5), tone(180, 0.06, 0.2))
    if name == "water":
        return seq(tone(700, 0.06, 0.3, slide=0.6), tone(520, 0.08, 0.35, slide=-0.4), noise(0.1, 0.15, 5, 0.4))
    if name == "play":
        return tone(280, 0.22, 0.45, "sine", 1.8)
    if name == "wash":
        return seq(noise(0.16, 0.28, 15, 0.9), noise(0.1, 0.18, 16, 0.5))
    if name == "pet":
        return seq(tone(880, 0.06, 0.25), tone(1174, 0.1, 0.22))
    if name == "treat":
        return seq(tone(660, 0.07, 0.35), tone(880, 0.07, 0.35), tone(1320, 0.12, 0.3))
    raise KeyError(name)


def main() -> None:
    species = [
        "cat", "dog", "capybara", "penguin", "robot", "iguana", "spider", "snake",
        "goldfish", "rabbit", "turtle", "frog", "owl", "red_panda", "axolotl",
    ]
    for name in species:
        for take in range(3):
            save(ROOT / "voice" / name / f"{take}.wav", voice(name, take))
    for name in ("food", "water", "play", "wash", "pet", "treat"):
        save(ROOT / "action" / f"{name}.wav", action(name))
    print(f"wrote sounds under {ROOT}")


if __name__ == "__main__":
    main()
