#!/usr/bin/env python3
"""Markdown links for release artifacts.

    release_docs.py notes --version V --out FILE
        The CHANGELOG.md section `## V ...` as release notes, with every relative link
        rewritten to https://github.com/GreyforgeLabs/codepet/blob/vV/<path> (a relative
        link on a GitHub release page resolves under /releases/tag/ and goes nowhere).

    release_docs.py stage-links --version V --stage DIR
        In a staged release tree, rewrite each relative link whose target is not in the
        tree to the same tag-pinned GitHub URL, then fail if any relative link in any
        Markdown file still does not resolve.

Python 3 standard library only.
"""
import argparse
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
BLOB = "https://github.com/GreyforgeLabs/codepet/blob/v{version}/{path}"
LINK = re.compile(r"(!?\[[^\]\n]*\]\()([^)\s]+)(\))")


def is_relative(target):
    return not re.match(r"^(?:[a-z][a-z0-9+.-]*:|#|/)", target, re.I)


def changelog_section(version):
    text = (REPO / "CHANGELOG.md").read_text()
    m = re.search(rf"(?m)^## {re.escape(version)}\b[^\n]*\n", text)
    if not m:
        raise SystemExit(f"CHANGELOG.md has no '## {version}' section")
    rest = text[m.end():]
    nxt = re.search(r"(?m)^## ", rest)
    return rest[:nxt.start()] if nxt else rest


def notes(version, out):
    def absolute(m):
        target = m.group(2)
        if not is_relative(target):
            return m.group(0)
        path, _, anchor = target.partition("#")
        if not (REPO / path).exists():
            raise SystemExit(f"CHANGELOG.md links a missing file: {target}")
        url = BLOB.format(version=version, path=path) + (f"#{anchor}" if anchor else "")
        return m.group(1) + url + m.group(3)
    body = LINK.sub(absolute, changelog_section(version)).strip() + "\n"
    left = [m.group(2) for m in LINK.finditer(body) if is_relative(m.group(2))]
    if left:
        raise SystemExit(f"relative links left in the notes: {left}")
    Path(out).write_text(body)
    print(f"wrote {out}")


def stage_links(version, stage):
    stage = Path(stage).resolve()
    rewritten, broken = 0, []
    for md in sorted(stage.rglob("*.md")):
        text = md.read_text()

        def fix(m):
            nonlocal rewritten
            target = m.group(2)
            if not is_relative(target):
                return m.group(0)
            path, _, anchor = target.partition("#")
            resolved = (md.parent / path).resolve()
            if resolved.exists():
                return m.group(0)
            try:
                rel = resolved.relative_to(stage).as_posix()
            except ValueError:
                broken.append(f"{md.relative_to(stage)}: {target} (outside the tree)")
                return m.group(0)
            if not (REPO / rel).exists():
                broken.append(f"{md.relative_to(stage)}: {target} (not in the repository either)")
                return m.group(0)
            rewritten += 1
            return m.group(1) + BLOB.format(version=version, path=rel) + (f"#{anchor}" if anchor else "") + m.group(3)

        new = LINK.sub(fix, text)
        if new != text:
            md.write_text(new)
    for md in sorted(stage.rglob("*.md")):
        for m in LINK.finditer(md.read_text()):
            target = m.group(2)
            if is_relative(target) and not (md.parent / target.partition("#")[0]).exists():
                broken.append(f"{md.relative_to(stage)}: {target}")
    if broken:
        raise SystemExit("unresolved relative links:\n  " + "\n  ".join(sorted(set(broken))))
    print(f"links: {rewritten} rewritten to the v{version} tag; every relative link resolves")


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    n = sub.add_parser("notes")
    n.add_argument("--version", required=True)
    n.add_argument("--out", required=True)
    s = sub.add_parser("stage-links")
    s.add_argument("--version", required=True)
    s.add_argument("--stage", required=True)
    args = ap.parse_args()
    if args.cmd == "notes":
        notes(args.version, args.out)
    else:
        stage_links(args.version, args.stage)
    return 0


if __name__ == "__main__":
    sys.exit(main())
