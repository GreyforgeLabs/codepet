#!/usr/bin/env python3
"""Build the release asset codepet-art-sources-v0.2.tar from the pet-art source sheets.

    python3 tools/release/art_sources.py --src <sheet root> --out <dir>
        [--run-root <generation run dir>]... [--mtime EPOCH]

<sheet root> holds one directory per species (`cat/`, `goldfish-in-jar/`, ...), each with
the six stage sheets, `provenance.json`, the PR #7 reference and prompts; rejected
attempts (`*.reject-*`) are left out, as the extractor ignores them. The source tree is
only read. The tar holds `v0.2/<species>/...` plus
`v0.2/LICENSE` and `v0.2/README.md`, so `tar xf ... -C art-src` gives the default source
root of tools/petart/extract.py.

Each provenance.json is published as a cleaned copy: the `wrapper` key is dropped, the
`tool` key gets neutral wording, and any absolute path into a generation run directory
given with --run-root becomes relative to it: everything up to and including the run
directory's name is stripped, whatever absolute prefix precedes it. Sheet names, stage
names and every sha256 stay as they are, so `extract.py --check` gives the same result.
The tar is reproducible: sorted names, root:root numeric owner, fixed mode and mtime
(default: SOURCE_DATE_EPOCH, else the newest source file's mtime). The build fails if a
sheet's sha256 does not match its provenance.json, or if any text member still contains
a home-directory path (`/home/<name>/`, `~/`), the word "operator", or one of the extra
terms listed (comma or space separated) in CODEPET_LEAK_TERMS, matched case-insensitively.

Python 3 standard library only.
"""
import argparse
import hashlib
import io
import json
import os
import re
import sys
import tarfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
NAME = "codepet-art-sources-v0.2.tar"
TOP = "v0.2"
TOOL = "gpt-image-2 via Codex CLI imagegen"
LEAK_PATTERNS = (r"/home/[^/\s]+/", r"~/", r"\boperator\b")
LEAK_TERMS_ENV = "CODEPET_LEAK_TERMS"
TEXT_SUFFIXES = (".json", ".txt", ".md")

README = """# CodePet pet-art source sheets (v0.2)

The GPT Image 2 source sheets that `tools/petart/extract.py` turns into the 192x192 WebP
frames shipped in CodePet (`qml/assets/pets/`). One directory per species:

- `stage{1,2,3}-{base,care}.png`: the six 4x3 chroma-key sheets the frames come from;
- `provenance.json`: prompts, chroma, stage names and the sha256 of every sheet;
- `ref-pr7.png`: the PR #7 style anchor the sheets were generated from;
- `prompts/`: the generation prompts.

Rejected attempts are not included; `provenance.json` still describes them.

Generated with GPT Image 2 (OpenAI image generation) via the Codex CLI image tool on the
project owner's ChatGPT plan. Absolute paths into the generation run directory in each
`provenance.json` were made relative to it for publication; nothing else was changed.

To rebuild and byte-compare the shipped frames, from a CodePet checkout:

    mkdir -p art-src && tar xf codepet-art-sources-v0.2.tar -C art-src
    python3 tools/petart/extract.py --check

Licence: distributed under the MIT licence in `LICENSE` (the same terms as
`qml/assets/pets/LICENSE`). The images are AI-generated; copyright and distribution
suitability have not been legally reviewed. See `docs/licenses.md` in the repository.
"""


def run_prefix(run_roots):
    """Regex for an absolute path (`/...` or `~/...`) up to and including a run directory.

    Only the run directory's name has to match, so a path recorded on another machine or
    under another home directory is stripped too. None when no run root was given."""
    names = sorted({Path(r).expanduser().name for r in run_roots} - {""}, key=len, reverse=True)
    if not names:
        return None
    alternatives = "|".join(re.escape(n) for n in names)
    return re.compile(r"(?<![\w.~/-])~?/(?:[^/\s\"']+/)*?(?:" + alternatives + r")(?:/|(?=[\s\"']|$))")


def leak_regex(env=None):
    env = os.environ if env is None else env
    extra = [t for t in re.split(r"[,\s]+", env.get(LEAK_TERMS_ENV, "")) if t]
    return re.compile("|".join(list(LEAK_PATTERNS) + [re.escape(t) for t in extra]), re.I)


def clean(value, prefix):
    if isinstance(value, dict):
        return {k: clean(v, prefix) for k, v in value.items() if k != "wrapper"}
    if isinstance(value, list):
        return [clean(v, prefix) for v in value]
    if isinstance(value, str) and prefix is not None:
        return prefix.sub("", value)
    return value


def clean_provenance(raw, prefix):
    doc = json.loads(raw)
    out = clean(doc, prefix)
    if "tool" in out:
        out["tool"] = TOOL
    return (json.dumps(out, indent=1, ensure_ascii=False) + "\n").encode()


def default_mtime(src):
    """SOURCE_DATE_EPOCH, else the newest source file's mtime: stable while the sheets are."""
    if os.environ.get("SOURCE_DATE_EPOCH", "").isdigit():
        return int(os.environ["SOURCE_DATE_EPOCH"])
    return max(int(p.stat().st_mtime) for p in src.rglob("*") if p.is_file())


def members(src, prefix):
    """(archive name, bytes or None for a directory) in sorted order."""
    out = [(TOP, None)]
    for path in sorted(src.rglob("*")):
        if path.is_symlink():
            raise SystemExit(f"refusing a symlink in the sources: {path}")
        rel = path.relative_to(src).as_posix()
        if ".reject-" in path.name:
            continue
        name = f"{TOP}/{rel}"
        if path.is_dir():
            out.append((name, None))
        elif path.name == "provenance.json":
            out.append((name, clean_provenance(path.read_bytes(), prefix)))
        else:
            out.append((name, path.read_bytes()))
    out.append((f"{TOP}/LICENSE", (REPO / "qml/assets/pets/LICENSE").read_bytes()))
    out.append((f"{TOP}/README.md", README.encode()))
    return sorted(out, key=lambda m: m[0])


def verify(entries, leaks):
    data = dict(entries)
    problems = []
    for name, body in entries:
        if body is not None and name.endswith(TEXT_SUFFIXES):
            for line_no, line in enumerate(body.decode(errors="replace").splitlines(), 1):
                if leaks.search(line):
                    problems.append(f"{name}:{line_no}: {line.strip()[:120]}")
    species = 0
    for name, body in entries:
        if not name.endswith("/provenance.json"):
            continue
        species += 1
        folder = name.rsplit("/", 1)[0]
        for sheet in json.loads(body).get("sheets", []):
            blob = data.get(f"{folder}/{sheet.get('file')}")
            if blob is None:
                problems.append(f"{folder}: missing {sheet.get('file')}")
            elif hashlib.sha256(blob).hexdigest() != sheet.get("sha256"):
                problems.append(f"{folder}/{sheet.get('file')}: sha256 differs from provenance.json")
    if species == 0:
        problems.append("no provenance.json found")
    return species, problems


def write_tar(entries, out, mtime):
    with tarfile.open(out, "w", format=tarfile.GNU_FORMAT) as tar:
        for name, body in entries:
            info = tarfile.TarInfo(name)
            info.uid = info.gid = 0
            info.uname = info.gname = "root"
            info.mtime = mtime
            if body is None:
                info.type = tarfile.DIRTYPE
                info.mode = 0o755
                tar.addfile(info)
            else:
                info.mode = 0o644
                info.size = len(body)
                tar.addfile(info, io.BytesIO(body))


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--src", type=Path, required=True, help="source sheet root (read only)")
    ap.add_argument("--out", type=Path, required=True, help="output directory")
    ap.add_argument("--run-root", action="append", default=[], metavar="DIR",
                    help="generation run directory whose absolute prefix is stripped from "
                         "provenance paths (repeatable; only its name has to match)")
    ap.add_argument("--mtime", type=int, default=None,
                    help="member mtime (default SOURCE_DATE_EPOCH, else the newest source mtime)")
    args = ap.parse_args()
    src = args.src.expanduser().resolve()
    if not src.is_dir():
        raise SystemExit(f"no source directory: {src}")
    entries = members(src, run_prefix(args.run_root))
    species, problems = verify(entries, leak_regex())
    if problems:
        print("\n".join(problems), file=sys.stderr)
        raise SystemExit(f"{len(problems)} problem(s); nothing written")
    args.out.mkdir(parents=True, exist_ok=True)
    out = args.out / NAME
    write_tar(entries, out, default_mtime(src) if args.mtime is None else args.mtime)
    digest = hashlib.sha256(out.read_bytes()).hexdigest()
    print(f"{digest}  {NAME}")
    print(f"wrote {out} ({out.stat().st_size} bytes, {species} species, {len(entries)} entries)",
          file=sys.stderr)


if __name__ == "__main__":
    main()
