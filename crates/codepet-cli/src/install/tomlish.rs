//! A read-only reader for the only `~/.codex/config.toml` keys CodePet may look at (spec
//! §5.7, invariant O1): `[features] hooks` / `codex_hooks` and the `[hooks.state."<key>"]`
//! trust entries (`trusted_hash`, `enabled`). Everything else is skipped unread; the file is
//! never logged, copied or written. A full TOML parser is not needed for that and would be
//! a new dependency.

/// One `[hooks.state."<key>"]` entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrustEntry {
    pub key: String,
    pub trusted_hash: Option<String>,
    pub enabled: Option<bool>,
}

/// What CodePet reads from `config.toml`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CodexConfig {
    /// `[features] hooks = false` (or the alias `codex_hooks = false`).
    pub hooks_off: bool,
    pub trust: Vec<TrustEntry>,
}

impl CodexConfig {
    pub fn trust_for(&self, key: &str) -> Option<&TrustEntry> {
        self.trust.iter().rev().find(|t| t.key == key)
    }
}

/// Splits a dotted key (`a."b.c".d`) into segments; `None` if it is not a plain key.
fn key_segments(s: &str) -> Option<Vec<String>> {
    let b = s.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    loop {
        while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
            i += 1;
        }
        let (seg, next) = match b.get(i)? {
            b'"' => basic_string(s, i)?,
            b'\'' => literal_string(s, i)?,
            _ => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'-')
                {
                    i += 1;
                }
                if i == start {
                    return None;
                }
                (s[start..i].to_owned(), i)
            }
        };
        out.push(seg);
        i = next;
        while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
            i += 1;
        }
        match b.get(i) {
            None => return Some(out),
            Some(b'.') => i += 1,
            Some(_) => return None,
        }
    }
}

/// A basic string starting at `s[i] == '"'`: (decoded, index after the closing quote).
fn basic_string(s: &str, i: usize) -> Option<(String, usize)> {
    let mut out = String::new();
    let mut chars = s[i + 1..].char_indices();
    while let Some((j, c)) = chars.next() {
        match c {
            '"' => return Some((out, i + 1 + j + 1)),
            '\\' => {
                let (_, e) = chars.next()?;
                match e {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    'u' | 'U' => {
                        let n = if e == 'u' { 4 } else { 8 };
                        let mut hex = String::new();
                        for _ in 0..n {
                            hex.push(chars.next()?.1);
                        }
                        out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                    }
                    _ => return None,
                }
            }
            '\n' => return None,
            c => out.push(c),
        }
    }
    None
}

fn literal_string(s: &str, i: usize) -> Option<(String, usize)> {
    let rest = &s[i + 1..];
    let end = rest.find('\'')?;
    let body = &rest[..end];
    if body.contains('\n') {
        return None;
    }
    Some((body.to_owned(), i + 1 + end + 1))
}

/// Cuts a `#` comment that is outside any string.
fn strip_comment(line: &str) -> &str {
    let mut in_basic = false;
    let mut in_literal = false;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        if in_basic {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_basic = false;
            }
            continue;
        }
        if in_literal {
            if c == '\'' {
                in_literal = false;
            }
            continue;
        }
        match c {
            '"' => in_basic = true,
            '\'' => in_literal = true,
            '#' => return &line[..i],
            _ => {}
        }
    }
    line
}

/// A scalar this reader understands.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Scalar {
    Bool(bool),
    Str(String),
    Other,
}

fn scalar(v: &str) -> Scalar {
    let v = v.trim();
    match v {
        "true" => Scalar::Bool(true),
        "false" => Scalar::Bool(false),
        _ if v.starts_with('"') && !v.starts_with("\"\"\"") => match basic_string(v, 0) {
            Some((s, end)) if v[end..].trim().is_empty() => Scalar::Str(s),
            _ => Scalar::Other,
        },
        _ if v.starts_with('\'') && !v.starts_with("'''") => match literal_string(v, 0) {
            Some((s, end)) if v[end..].trim().is_empty() => Scalar::Str(s),
            _ => Scalar::Other,
        },
        _ => Scalar::Other,
    }
}

/// Splits `key = value` at the first `=` outside a quoted key.
fn split_kv(line: &str) -> Option<(Vec<String>, &str)> {
    let mut in_basic = false;
    let mut in_literal = false;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        if in_basic {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_basic = false;
            }
            continue;
        }
        if in_literal {
            if c == '\'' {
                in_literal = false;
            }
            continue;
        }
        match c {
            '"' => in_basic = true,
            '\'' => in_literal = true,
            '=' => return Some((key_segments(line[..i].trim())?, &line[i + 1..])),
            _ => {}
        }
    }
    None
}

/// Parses `{ a = 1, b = "x" }` (one level, no nested tables) into key/scalar pairs.
fn inline_table(v: &str) -> Option<Vec<(Vec<String>, Scalar)>> {
    let v = v.trim();
    let body = v.strip_prefix('{')?.strip_suffix('}')?;
    let mut out = Vec::new();
    // Split at commas outside strings.
    let mut parts = Vec::new();
    let (mut start, mut in_basic, mut in_literal, mut escaped) = (0, false, false, false);
    for (i, c) in body.char_indices() {
        if in_basic {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_basic = false;
            }
            continue;
        }
        if in_literal {
            if c == '\'' {
                in_literal = false;
            }
            continue;
        }
        match c {
            '"' => in_basic = true,
            '\'' => in_literal = true,
            ',' => {
                parts.push(&body[start..i]);
                start = i + 1;
            }
            '{' | '[' => return None,
            _ => {}
        }
    }
    parts.push(&body[start..]);
    for p in parts {
        if p.trim().is_empty() {
            continue;
        }
        let (k, val) = split_kv(p)?;
        out.push((k, scalar(val)));
    }
    Some(out)
}

/// Reads the CodePet-relevant keys. Malformed lines are skipped; a value spanning lines
/// (multi-line strings or arrays) is skipped whole, so its content is never read as keys.
pub fn read(text: &str) -> CodexConfig {
    let mut cfg = CodexConfig::default();
    let mut table: Vec<String> = Vec::new();
    let mut lines = text.lines();
    let set = |cfg: &mut CodexConfig, path: &[String], value: &Scalar| {
        let p: Vec<&str> = path.iter().map(String::as_str).collect();
        match (p.as_slice(), value) {
            (["features", "hooks" | "codex_hooks"], Scalar::Bool(b)) => cfg.hooks_off = !b,
            (["hooks", "state", key, field], v) => {
                let entry = match cfg.trust.iter_mut().rev().find(|t| t.key == *key) {
                    Some(e) => e,
                    None => {
                        cfg.trust.push(TrustEntry {
                            key: (*key).to_owned(),
                            ..TrustEntry::default()
                        });
                        cfg.trust.last_mut().expect("just pushed")
                    }
                };
                match (*field, v) {
                    ("trusted_hash", Scalar::Str(s)) => entry.trusted_hash = Some(s.clone()),
                    ("enabled", Scalar::Bool(b)) => entry.enabled = Some(*b),
                    _ => {}
                }
            }
            _ => {}
        }
    };
    while let Some(raw) = lines.next() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("[[") {
            table = vec!["\u{0}array".to_owned()];
            continue;
        }
        if let Some(inner) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            table = key_segments(inner).unwrap_or_else(|| vec!["\u{0}bad".to_owned()]);
            if table.len() == 4 && table[0] == "hooks" && table[1] == "state" {
                // `[hooks.state."<key>"."x"]`: not an entry.
            }
            continue;
        }
        let Some((key, value)) = split_kv(line) else {
            continue;
        };
        let v = value.trim();
        // Multi-line values: skip their continuation lines unread.
        let open = v.starts_with("\"\"\"") || v.starts_with("'''");
        if open {
            let fence = &v[..3];
            if !v[3..].contains(fence) {
                for more in lines.by_ref() {
                    if more.contains(fence) {
                        break;
                    }
                }
            }
            continue;
        }
        if v.starts_with('[') {
            let mut depth: i32 = v.matches('[').count() as i32 - v.matches(']').count() as i32;
            while depth > 0 {
                match lines.next() {
                    Some(more) => {
                        let m = strip_comment(more);
                        depth += m.matches('[').count() as i32 - m.matches(']').count() as i32;
                    }
                    None => break,
                }
            }
            continue;
        }
        let mut path = table.clone();
        path.extend(key);
        if v.starts_with('{') {
            if let Some(pairs) = inline_table(v) {
                for (k, s) in pairs {
                    let mut full = path.clone();
                    full.extend(k);
                    set(&mut cfg, &full, &s);
                }
            }
            continue;
        }
        set(&mut cfg, &path, &scalar(v));
    }
    cfg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_features_and_trust_entries_in_every_style() {
        let text = r#"
model = "x" # comment with [hooks.state."fake"]
notify = ["a", "b",
  "c = d"]
[features]
hooks = false
[hooks.state."/h/.codex/hooks.json:stop:0:0"]
trusted_hash = "sha256:aa" # trailing
enabled = true
[hooks.state.'/h/.codex/hooks.json:stop:1:0']
trusted_hash = 'sha256:bb'
enabled = false
[hooks.state]
"/h/.codex/hooks.json:session_end:0:0" = { trusted_hash = "sha256:cc", enabled = true }
[profiles.x]
hooks = true
desc = """
[features]
hooks = true
"""
"#;
        let c = read(text);
        assert!(c.hooks_off);
        assert_eq!(c.trust.len(), 3);
        let a = c.trust_for("/h/.codex/hooks.json:stop:0:0").unwrap();
        assert_eq!(a.trusted_hash.as_deref(), Some("sha256:aa"));
        assert_eq!(a.enabled, Some(true));
        let b = c.trust_for("/h/.codex/hooks.json:stop:1:0").unwrap();
        assert_eq!(
            (b.trusted_hash.as_deref(), b.enabled),
            (Some("sha256:bb"), Some(false))
        );
        let s = c.trust_for("/h/.codex/hooks.json:session_end:0:0").unwrap();
        assert_eq!(s.trusted_hash.as_deref(), Some("sha256:cc"));
        assert!(c.trust_for("fake").is_none());
    }

    #[test]
    fn alias_and_defaults() {
        assert!(!read("").hooks_off);
        assert!(read("[features]\ncodex_hooks = false\n").hooks_off);
        assert!(read("features.hooks = false\n").hooks_off);
        assert!(!read("[features]\nhooks = true\n").hooks_off);
        assert!(read("features = { hooks = false }\n").hooks_off);
    }
}
