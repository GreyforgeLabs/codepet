#!/usr/bin/env python3
"""Calm-copy lint for CodePet's user-facing text (docs/lifecycle.md, Copy rules; ADR 0001 D7(e)).

Scans every string literal in the QML and JavaScript UI sources (qml/shared, qml/standalone,
qml/omarchy, qml/shell.qml and the Omarchy bar plugin, which stages qml/shared/LifeText.js
byte-for-byte) plus the copy-rules section of docs/lifecycle.md, and fails on the banned list:
"kill", "killed", "starv", "your fault", "abandon", "neglected you", "guilt", "punish", "streak",
"hurry" and "!!!" in prose. Comments and engineering prose (where "kill" legitimately describes
processes) are not scanned; identifier-like literals (object names, method names, paths) are
skipped. Terms match at a word start, so "Skills" is not "kill".

The banned list must equal LifeText.BANNED (checked here). Exemptions are explicit, per file
and per term, each with a reason.

v0.3 kid copy (SPEC-v0.3 §1.2 K1, K2, K8), over everything that reaches a kid-facing screen:
- omarchy-plugin/strings.json, every key and every careIconSet variant (aquatic., robot.);
  Help (help.*) is the adult page and exempt, the Codex guide (codex.*) is exempt from K1/K2
  but not K8;
- string literals in the bar plugin's kid-facing QML (Help and the Codex guide exempt);
- the species manifest fields that reach the screen: displayName, defaultName and careWords
  (at most two plain words each); careLabels and careHints must never be read by the plugin;
- the Home status line: every face line, filled with a typical name and every app pair, fits
  one line of the 448 px card in a monospace font at Style.font.heading.
K1: no digit or %, except the literal /reload-plugins. K2: none of the jargon words, except the
verb phrases "turn on/off", "turned on/off" and "turn {name} into". K8: at most 8 words a line,
and at most 6 words for 90% of them.

Run: python3 qml/tests/copy_lint.py           (self-test, then scan; exit 1 on findings)
     python3 qml/tests/copy_lint.py --self-test
No dependencies beyond the Python standard library.
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]

BANNED = ["kill", "killed", "starv", "your fault", "abandon", "neglected you", "guilt", "punish",
          "streak", "hurry", "!!!"]

SCAN_GLOBS = [
    "qml/shared/*.qml", "qml/shared/*.js",
    "qml/standalone/*.qml", "qml/standalone/*.js",
    "qml/omarchy/*.qml", "qml/omarchy/*.js",
    "qml/shell.qml",
    "qml/work.qml",
    "omarchy-plugin/*.qml", "omarchy-plugin/*.js",
]
LIFECYCLE_DOC = "docs/lifecycle.md"

# (relative path, term) -> reason. Keep this list short and specific.
EXEMPT = {
    ("qml/shared/TaskReview.qml", "abandon"):
        "v0.1 task-recovery wording: abandoning an interrupted simulated task (task.discard, no reward); "
        "it never describes the companion",
}
# Only this file may fence its BANNED definition with copy-lint: off/on markers.
MARKER_FILE = "qml/shared/LifeText.js"

# Identifier-like: empty, or one word starting lowercase (camelCase allowed), or a dotted /
# underscored / slashed token such as an object name, method, state or path. No whitespace.
TOKEN_LIKE = re.compile(r"|[a-z][A-Za-z0-9_.\-/:#%@*?]*|[A-Za-z0-9_\-]*[._/:#][A-Za-z0-9_.\-/:#%@*?]*")
REGEX_PRECEDERS = set("(,=:[!&|?{};+-*%<>~^")


def banned_in(text: str) -> str:
    lower = text.lower()
    for term in BANNED:
        if term == "!!!":
            if term in lower:
                return term
            continue
        if re.search(r"(?:^|[^a-z])" + re.escape(term), lower):
            return term
    return ""


def string_literals(src: str):
    """Yields (line, text) for each string literal in QML/JS source, skipping comments and
    regular-expression literals. Template literals are returned with ${...} left in place."""
    i, n, line = 0, len(src), 1
    prev = ""  # last significant (non-space, non-comment) character
    prev_word = ""
    while i < n:
        c = src[i]
        if c == "\n":
            line += 1
            i += 1
            continue
        if c.isspace():
            i += 1
            continue
        if src.startswith("//", i):
            j = src.find("\n", i)
            i = n if j < 0 else j
            continue
        if src.startswith("/*", i):
            j = src.find("*/", i + 2)
            j = n if j < 0 else j + 2
            line += src.count("\n", i, j)
            i = j
            continue
        if c in "\"'`":
            start_line = line
            j = i + 1
            buf = []
            while j < n and src[j] != c:
                if src[j] == "\\" and j + 1 < n:
                    esc = src[j + 1]
                    buf.append({"n": "\n", "t": "\t"}.get(esc, esc))
                    if esc == "\n":
                        line += 1
                    j += 2
                    continue
                if src[j] == "\n":
                    line += 1
                buf.append(src[j])
                j += 1
            yield start_line, "".join(buf)
            i = j + 1
            prev, prev_word = c, ""
            continue
        if c == "/" and (prev == "" or prev in REGEX_PRECEDERS or prev_word in ("return", "typeof", "case")):
            j = i + 1
            in_class = False
            closed = False
            while j < n and src[j] != "\n":
                if src[j] == "\\":
                    j += 2
                    continue
                if src[j] == "[":
                    in_class = True
                elif src[j] == "]":
                    in_class = False
                elif src[j] == "/" and not in_class:
                    closed = True
                    break
                j += 1
            if closed:
                i = j + 1
                while i < n and src[i].isalpha():
                    i += 1
                prev, prev_word = "/", ""
                continue
            # Not a regular expression after all (no closing slash on this line): an operator.
        if c.isalnum() or c in "_$":
            j = i
            while j < n and (src[j].isalnum() or src[j] in "_$"):
                j += 1
            prev_word = src[i:j]
            prev = src[j - 1]
            i = j
            continue
        prev, prev_word = c, ""
        i += 1


def user_facing(text: str) -> bool:
    """Identifier-like literals (object names, methods, states, paths) are not prose."""
    return TOKEN_LIKE.fullmatch(text) is None


def strip_marker_region(rel: str, src: str) -> tuple[str, list[str]]:
    """Blanks the copy-lint: off/on region (keeping line numbers). Returns (source, problems)."""
    problems = []
    off = "// copy-lint: off"
    on = "// copy-lint: on"
    if off not in src:
        return src, problems
    if rel != MARKER_FILE:
        problems.append(f"{rel}: copy-lint markers are only allowed in {MARKER_FILE}")
        return src, problems
    a = src.index(off)
    b = src.find(on, a)
    if b < 0:
        problems.append(f"{rel}: copy-lint: off without copy-lint: on")
        return src, problems
    region = src[a:b]
    body = [ln for ln in region.splitlines()[1:] if ln.strip()]
    if len(body) != 1 or not body[0].startswith("var BANNED = ["):
        problems.append(f"{rel}: the copy-lint: off region may only hold the BANNED list")
    blank = "".join("\n" if ch == "\n" else " " for ch in region)
    return src[:a] + blank + src[b:], problems


def lifetext_banned(src: str) -> list[str] | None:
    m = re.search(r"^var BANNED = \[(.*)\]\s*$", src, re.M)
    if not m:
        return None
    return re.findall(r'"([^"]*)"', m.group(1))


def scan_source(rel: str, src: str) -> list[str]:
    findings = []
    src, problems = strip_marker_region(rel, src)
    findings.extend(problems)
    lines = src.splitlines()
    for line, text in string_literals(src):
        if line - 1 < len(lines) and re.match(r"\s*(\.?import)\b", lines[line - 1]):
            continue
        if not user_facing(text):
            continue
        term = banned_in(text)
        if term and (rel, term) not in EXEMPT:
            findings.append(f"{rel}:{line}: banned '{term}' in user-facing text: {text[:120]!r}")
    return findings


def copy_section(md: str) -> str:
    """The body of the first heading whose title mentions copy (until the next heading of the
    same or a higher level), with inline code spans removed."""
    lines = md.splitlines()
    out, level = [], None
    for ln in lines:
        m = re.match(r"^(#+)\s+(.*)$", ln)
        if m:
            if level is not None and len(m.group(1)) <= level:
                break
            if level is None and re.search(r"\bcopy\b", m.group(2), re.I):
                level = len(m.group(1))
                continue
        if level is not None:
            out.append(ln)
    return "\n".join(out)


def scan_doc(rel: str, md: str) -> list[str]:
    findings = []
    section = copy_section(md)
    if not section.strip():
        return [f"{rel}: no copy-rules section found (a heading mentioning 'copy')"]
    for n, ln in enumerate(section.splitlines(), 1):
        text = re.sub(r"`[^`]*`", "", ln)
        # A line naming three or more banned terms is the banned list itself, not copy.
        if sum(1 for t in BANNED if banned_in_term(text, t)) >= 3:
            continue
        term = banned_in(text)
        if term:
            findings.append(f"{rel} (copy rules, line {n}): banned '{term}': {ln.strip()[:120]!r}")
    return findings


def banned_in_term(text: str, term: str) -> bool:
    lower = text.lower()
    if term == "!!!":
        return term in lower
    return re.search(r"(?:^|[^a-z])" + re.escape(term), lower) is not None


def scan(repo: Path) -> tuple[list[str], list[str]]:
    findings, notes = [], []
    lifetext = repo / MARKER_FILE
    if lifetext.exists():
        listed = lifetext_banned(lifetext.read_text(encoding="utf-8"))
        if listed != BANNED:
            findings.append(f"{MARKER_FILE}: BANNED {listed} differs from copy_lint.BANNED {BANNED}")
    else:
        findings.append(f"{MARKER_FILE} is missing")
    files = sorted({p for g in SCAN_GLOBS for p in repo.glob(g) if p.is_file()})
    for path in files:
        rel = path.relative_to(repo).as_posix()
        findings.extend(scan_source(rel, path.read_text(encoding="utf-8")))
    doc = repo / LIFECYCLE_DOC
    if doc.exists():
        findings.extend(scan_doc(LIFECYCLE_DOC, doc.read_text(encoding="utf-8")))
    else:
        notes.append(f"{LIFECYCLE_DOC} not present yet; its copy-rules section was not scanned")
    notes.append(f"scanned {len(files)} UI source files")
    return findings, notes


def self_test() -> None:
    def lits(src):
        return [t for _, t in string_literals(src)]

    assert lits('var a = "x" // "kill"\n/* "abandon" */ var b = \'y\'') == ["x", "y"]
    assert lits('var r = /["\']/g; var s = "after"') == ["after"], lits('var r = /["\']/g; var s = "after"')
    assert lits('x = a / b; y = "z" / 2') == ["z"]
    assert lits('t: "say \\"hi\\""') == ['say "hi"']
    assert lits('return /a"b/.test(s) ? "yes" : "no"') == ["yes", "no"]
    assert [ln for ln, _ in string_literals('a\n\n"x"\n/*\n\n*/"y"')] == [3, 6]
    assert [ln for ln, _ in string_literals('(a\n/ b)\n"x"')] == [3], "a division at a line start keeps line numbers"
    assert banned_in("Skills · simulation progress") == ""
    assert banned_in("Keep your streak!") == "streak"
    assert banned_in("STARVING") == "starv"
    assert banned_in("Wow!!!") == "!!!"
    assert banned_in("Ginger !!") == ""
    assert not user_facing("taskReview.abandon") and not user_facing("abandoned")
    assert user_facing("Abandon (no reward)") and user_facing("kill it now") and user_facing("Hurry")
    f = scan_source("qml/shared/X.qml", 'Label { text: "Don\'t hurry"; objectName: "x.kill" }')
    assert len(f) == 1 and "hurry" in f[0], f
    assert scan_source("qml/shared/TaskReview.qml", 'text: "Abandon (no reward)"') == []
    assert scan_source("qml/shared/Panel.qml", 'text: "Abandon (no reward)"') != []
    assert scan_source("qml/shared/X.qml", 'import "kill me.js" as K\n') == []
    marked = '// copy-lint: off\nvar BANNED = ["kill"]\n// copy-lint: on\nvar t = "fine"\n'
    assert scan_source(MARKER_FILE, marked) == []
    assert scan_source("qml/shared/Other.js", marked) != []
    sneaky = '// copy-lint: off\nvar BANNED = ["kill"]\nvar t = "kill"\n// copy-lint: on\n'
    assert scan_source(MARKER_FILE, sneaky) != []
    doc = "# Lifecycle\nKill the process.\n## Copy rules\nNever say: kill, starve, guilt, streak.\nDon't hurry them.\n## Other\nkill -TERM\n"
    out = scan_doc("docs/lifecycle.md", doc)
    assert len(out) == 1 and "hurry" in out[0], out
    assert scan_doc("docs/lifecycle.md", "# Nothing here\n") != []


# --------------------------------------------------------------------------- v0.3 kid copy

PLUGIN_STRINGS = "omarchy-plugin/strings.json"
PETS_DIR = "qml/assets/pets"
JARGON = ["session", "hook", "turn", "provider", "xp", "level", "percent", "simulation", "preflight",
          "token", "armed", "attended", "presence", "daemon", "config", "rpc", "plugin", "marketplace",
          "client", "agent", "trust"]
K2_VERB_PHRASES = [r"\bturn(ed)? (on|off)\b", r"\bturn \{name\} into", r"\bturn \S+ into"]
K1_EXEMPT_LITERALS = ["/reload-plugins"]
# Plugin QML files whose literals are not kid copy: the adult pages, and the transport and
# presence recorder, whose literals are error messages for logs (the popup shows only
# strings.json err.generic for any failure).
ADULT_QML = {"HelpScreen.qml", "UpdatesScreen.qml", "UpdateController.qml", "CodexGuide.qml", "BarRpc.qml", "PresenceRecorder.qml"}
APP_NAMES = ["Claude Code", "Codex", "OpenCode", "Hermes", "Grok"]
# Status line fit (SPEC-v0.3 §3.2): inner card width at base 16, monospace advance 0.6 em,
# Style.font.heading = round(16 x 1.333).
CARD_INNER_PX = 448
MONO_ADVANCE = 0.6
HEADING_PX = round(16 * 1.333)
STATUS_KEYS = re.compile(r"^(?:(?:aquatic|robot)\.)?(?:mood\.|act\.|grew$|off\.line$|picker\.welcome$|treats\.full$)")


def words(text: str) -> int:
    return len([w for w in re.split(r"\s+", text.strip()) if re.search(r"[A-Za-z{]", w)])


def kid_problems(text: str, k1k2: bool = True, statistic: bool = False) -> list[str]:
    """K1/K2/K8 problems of one kid-facing line ([] when it passes)."""
    out = []
    scan = text
    for lit in K1_EXEMPT_LITERALS:
        scan = scan.replace(lit, " ")
    if k1k2:
        if not statistic and re.search(r"[0-9%]", re.sub(r"\{[a-z0-9]+\}", "", scan)):
            out.append("K1 digit or %")
        low = scan.lower()
        for phrase in K2_VERB_PHRASES:
            low = re.sub(phrase, " ", low)
        low = re.sub(r"\{[a-z0-9]+\}", "", low)
        for term in JARGON:
            if statistic and term in {"xp", "level", "percent"}:
                continue
            if re.search(r"\b" + term, low):
                out.append(f"K2 {term}")
    if words(text) > 8:
        out.append(f"K8 {words(text)} words")
    return out


def status_width_px(text: str, font_px: int) -> float:
    return len(text) * font_px * MONO_ADVANCE


def lint_strings(table: dict) -> tuple[list[str], list[str]]:
    findings, notes = [], []
    kid_counts = []
    for key, text in table.items():
        if key.startswith("help.") or key.startswith("aquatic.help.") or key.startswith("robot.help."):
            continue
        adult_guide = (key.startswith("codex.") or key.startswith("updates."))
        for p in kid_problems(text, k1k2=not adult_guide, statistic=key.startswith("stats.") or key == "work.energy"):
            findings.append(f"{PLUGIN_STRINGS}: {key}: {p}: {text!r}")
        if not adult_guide:
            kid_counts.append(words(text))
        if STATUS_KEYS.search(key):
            for fill in status_fills(text):
                if status_width_px(fill, HEADING_PX) > CARD_INNER_PX:
                    findings.append(f"{PLUGIN_STRINGS}: {key}: status line does not fit at heading size: {fill!r}")
    short = sum(1 for n in kid_counts if n <= 6)
    if kid_counts and short / len(kid_counts) < 0.9:
        findings.append(f"{PLUGIN_STRINGS}: K8 only {short}/{len(kid_counts)} kid lines have <= 6 words")
    return findings, notes


def status_fills(template: str) -> list[str]:
    """The template filled with a typical six-letter name and every app (pair)."""
    out = []
    apps = APP_NAMES if "{app}" in template else [""]
    for a in apps:
        apps2 = [b for b in APP_NAMES if b != a] if "{app2}" in template else [""]
        for b in apps2:
            out.append(template.replace("{name}", "Ginger").replace("{app}", a).replace("{app2}", b)
                       .replace("{word}", "Fresh water"))
    return out


def lint_plugin_qml(repo: Path) -> list[str]:
    findings = []
    for path in sorted((repo / "omarchy-plugin").glob("*.qml")):
        if path.name in ADULT_QML:
            continue
        rel = path.relative_to(repo).as_posix()
        src = path.read_text(encoding="utf-8")
        lines = src.splitlines()
        for line, text in string_literals(src):
            if line - 1 < len(lines) and re.match(r"\s*(\.?import)\b", lines[line - 1]):
                continue
            if not user_facing(text) or not re.search(r"[A-Za-z]", text):
                continue
            for p in kid_problems(text):
                findings.append(f"{rel}:{line}: {p}: {text!r}")
    for path in sorted((repo / "omarchy-plugin").glob("*.js")) + sorted((repo / "omarchy-plugin").glob("*.qml")):
        src = path.read_text(encoding="utf-8")
        for field in ("careLabels", "careHints"):
            if field in src:
                findings.append(f"{path.relative_to(repo).as_posix()}: reads {field}, which never reaches a kid-facing screen")
    return findings


def lint_manifests(repo: Path) -> list[str]:
    findings = []
    for path in sorted((repo / PETS_DIR).glob("*/manifest.json")):
        rel = path.relative_to(repo).as_posix()
        m = json.loads(path.read_text(encoding="utf-8"))
        for field in ("displayName", "defaultName"):
            value = m.get(field)
            if not isinstance(value, str) or not value:
                findings.append(f"{rel}: {field} missing")
                continue
            for p in kid_problems(value):
                findings.append(f"{rel}: {field}: {p}: {value!r}")
        for need, word in (m.get("careWords") or {}).items():
            if need not in ("food", "water", "play", "wash"):
                findings.append(f"{rel}: careWords has unknown need {need!r}")
            if not isinstance(word, str) or not word or words(word) > 2:
                findings.append(f"{rel}: careWords.{need} must be one or two plain words: {word!r}")
                continue
            for p in kid_problems(word):
                findings.append(f"{rel}: careWords.{need}: {p}: {word!r}")
    return findings


def kid_scan(repo: Path) -> tuple[list[str], list[str]]:
    findings, notes = [], []
    strings = repo / PLUGIN_STRINGS
    if strings.is_file():
        f, n = lint_strings(json.loads(strings.read_text(encoding="utf-8")))
        findings += f
        notes += n
    else:
        findings.append(f"{PLUGIN_STRINGS} is missing")
    findings += lint_plugin_qml(repo)
    findings += lint_manifests(repo)
    return findings, notes


def kid_self_test() -> None:
    assert kid_problems("Ginger is happy!") == []
    assert kid_problems("Turn on") == [] and kid_problems("! Turned off in Codex") == []
    assert kid_problems("Turn {name} into…") == []
    assert kid_problems("Or type /reload-plugins.") == []
    assert kid_problems("Level 2") == ["K1 digit or %", "K2 level"]
    assert kid_problems("Level 2", statistic=True) == []
    assert kid_problems("72%", statistic=True) == []
    assert kid_problems("provider 42", statistic=True) == ["K2 provider"]
    assert kid_problems("your turn") == ["K2 turn"]
    assert kid_problems("50% done", k1k2=False) == []
    assert kid_problems("a b c d e f g h i") == ["K8 9 words"]
    assert words("{name} is helping {app} and {app2}.") == 6
    fills = status_fills("{name} is helping {app} and {app2}.")
    assert len(fills) == 20 and "Ginger is helping Claude Code and OpenCode." in fills
    f, n = lint_strings({"mood.happy": "{name} is happy!", "help.x": "Level 50% session", "codex.step3": "Type /hooks."})
    assert f == [] and n == [], (f, n)
    f, _ = lint_strings({"tile.food": "Feed the xp", "codex.step1": "one two three four five six seven eight nine"})
    assert any("K2 xp" in x for x in f) and any("K8 9" in x for x in f), f
    f, _ = lint_strings({"act.x": "{name} " + "w" * 60})
    assert any("at heading size" in x for x in f), f


def main(argv: list[str]) -> int:
    self_test()
    kid_self_test()
    if "--self-test" in argv:
        print("copy_lint self-test: ok")
        return 0
    findings, notes = scan(REPO)
    kid_findings, kid_notes = kid_scan(REPO)
    findings += kid_findings
    notes += sorted(set(kid_notes))
    for n in notes:
        print("note: " + n)
    for f in findings:
        print("FAIL " + f)
    print(f"copy_lint: {len(findings)} finding(s)")
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
