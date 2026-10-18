//! A read-only reader for the block-style YAML that Hermes writes to `~/.hermes/config.yaml`
//! (`yaml.safe_dump` output, plus hand edits): block mappings and sequences, flow
//! collections, plain, quoted and block scalars, comments. Used only by the settings key
//! check of a Hermes Connect/Disconnect (spec §5.2 step 4): CodePet never writes that file,
//! Hermes's own CLI does, and CodePet compares the named sections before and after.
//!
//! Anything this reader does not understand (anchors, aliases, tags, complex keys) is a
//! parse error, and the caller fails closed (no CLI runs before a readable file; a file
//! that became unreadable afterwards is `settings_changed`).

/// One YAML node. Scalars are normalized so that a re-dump compares equal: plain `null`,
/// `~` and empty are [`Y::Null`], plain booleans are [`Y::Bool`], everything else is text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Y {
    Null,
    Bool(bool),
    Str(String),
    Seq(Vec<Y>),
    Map(Vec<(String, Y)>),
}

impl Y {
    pub fn get(&self, key: &str) -> Option<&Y> {
        match self {
            Y::Map(kv) => kv.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    /// The scalar strings of a sequence (as a sorted set); `None` if not a sequence.
    pub fn str_set(&self) -> Option<Vec<String>> {
        match self {
            Y::Seq(items) => {
                let mut out: Vec<String> = items
                    .iter()
                    .map(|i| match i {
                        Y::Str(s) => Some(s.clone()),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                out.sort();
                out.dedup();
                Some(out)
            }
            Y::Null => Some(Vec::new()),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct YamlError;

// YAML mappings are unordered; reject duplicate keys before canonicalizing them.
fn mapping(mut pairs: Vec<(String, Y)>) -> Result<Y, YamlError> {
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    if pairs.windows(2).any(|p| p[0].0 == p[1].0) {
        return Err(YamlError);
    }
    Ok(Y::Map(pairs))
}

const MAX_DEPTH: usize = 64;

#[derive(Clone, Debug)]
struct Line {
    indent: usize,
    text: String,
}

/// Cuts a comment (`#` at the start or after whitespace, outside quotes).
fn strip_comment(s: &str) -> &str {
    let (mut single, mut double, mut prev_space) = (false, false, true);
    let mut escaped = false;
    for (i, c) in s.char_indices() {
        if double {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                double = false;
            }
        } else if single {
            if c == '\'' {
                single = false;
            }
        } else {
            match c {
                '"' => double = true,
                '\'' => single = true,
                '#' if prev_space => return &s[..i],
                _ => {}
            }
        }
        prev_space = c == ' ' || c == '\t';
    }
    s
}

/// Parses a YAML document into a tree; an empty document is [`Y::Null`].
pub fn parse(text: &str) -> Result<Y, YamlError> {
    let mut lines = Vec::new();
    for raw in text.lines() {
        if raw.contains('\t') && raw.trim_start().starts_with('\t') {
            return Err(YamlError);
        }
        let indent = raw.len() - raw.trim_start_matches(' ').len();
        let body = raw[indent..].trim_end();
        lines.push(Line {
            indent,
            text: body.to_owned(),
        });
    }
    let mut p = P { lines, i: 0 };
    p.skip_blank();
    if p.i >= p.lines.len() {
        return Ok(Y::Null);
    }
    let indent = p.lines[p.i].indent;
    let v = p.node(indent, 0)?;
    p.skip_blank();
    if p.i < p.lines.len() {
        return Err(YamlError);
    }
    Ok(v)
}

struct P {
    lines: Vec<Line>,
    i: usize,
}

/// Finds a mapping colon (`: ` or a trailing `:`) outside quotes and flow brackets.
fn key_colon(s: &str) -> Option<usize> {
    let (mut single, mut double, mut depth) = (false, false, 0i32);
    let mut escaped = false;
    let b = s.as_bytes();
    for (i, c) in s.char_indices() {
        if double {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                double = false;
            }
            continue;
        }
        if single {
            if c == '\'' {
                single = false;
            }
            continue;
        }
        match c {
            '"' if i == 0 => double = true,
            '\'' if i == 0 => single = true,
            '[' | '{' => depth += 1,
            ']' | '}' => depth -= 1,
            ':' if depth == 0 && (i + 1 == b.len() || b[i + 1] == b' ') => return Some(i),
            _ => {}
        }
    }
    None
}

fn is_seq_item(t: &str) -> bool {
    t == "-" || t.starts_with("- ")
}

fn unquote_key(k: &str) -> Result<String, YamlError> {
    let k = k.trim();
    if k.starts_with('"') || k.starts_with('\'') {
        match scalar(k)? {
            Y::Str(s) => Ok(s),
            _ => Err(YamlError),
        }
    } else if k.is_empty() || k.starts_with(['?', '&', '*', '!', '[', '{']) {
        Err(YamlError)
    } else {
        Ok(k.to_owned())
    }
}

/// One single-line scalar.
fn scalar(t: &str) -> Result<Y, YamlError> {
    let t = t.trim();
    if t.starts_with(['&', '*', '!', '|', '>', '%', '@', '`']) {
        return Err(YamlError);
    }
    if let Some(rest) = t.strip_prefix('\'') {
        let body = rest.strip_suffix('\'').ok_or(YamlError)?;
        return Ok(Y::Str(body.replace("''", "'")));
    }
    if let Some(rest) = t.strip_prefix('"') {
        let body = rest.strip_suffix('"').ok_or(YamlError)?;
        let mut out = String::new();
        let mut chars = body.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            match chars.next().ok_or(YamlError)? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                '0' => out.push('\0'),
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '/' => out.push('/'),
                ' ' => out.push(' '),
                'x' | 'u' | 'U' => {
                    return Err(YamlError);
                }
                _ => return Err(YamlError),
            }
        }
        return Ok(Y::Str(out));
    }
    Ok(match t {
        "" | "~" | "null" | "Null" | "NULL" => Y::Null,
        "true" | "True" | "TRUE" => Y::Bool(true),
        "false" | "False" | "FALSE" => Y::Bool(false),
        _ => Y::Str(t.to_owned()),
    })
}

/// A flow collection or scalar (`[a, b]`, `{k: v}`), possibly nested.
fn flow(s: &str) -> Result<Y, YamlError> {
    let mut f = Flow {
        b: s.trim().chars().collect(),
        i: 0,
    };
    let v = f.value(0)?;
    f.ws();
    if f.i != f.b.len() {
        return Err(YamlError);
    }
    Ok(v)
}

struct Flow {
    b: Vec<char>,
    i: usize,
}

impl Flow {
    fn ws(&mut self) {
        while self.i < self.b.len() && self.b[self.i].is_whitespace() {
            self.i += 1;
        }
    }
    fn token(&mut self) -> String {
        let start = self.i;
        if matches!(self.b.get(self.i), Some('"') | Some('\'')) {
            let q = self.b[self.i];
            self.i += 1;
            while self.i < self.b.len() {
                if self.b[self.i] == '\\' && q == '"' {
                    self.i += 2;
                    continue;
                }
                if self.b[self.i] == q {
                    if q == '\'' && self.b.get(self.i + 1) == Some(&'\'') {
                        self.i += 2;
                        continue;
                    }
                    self.i += 1;
                    break;
                }
                self.i += 1;
            }
        } else {
            while self.i < self.b.len() && !matches!(self.b[self.i], ',' | ']' | '}') {
                if self.b[self.i] == ':' && self.b.get(self.i + 1).is_none_or(|c| c.is_whitespace())
                {
                    break;
                }
                self.i += 1;
            }
        }
        self.b[start..self.i.min(self.b.len())].iter().collect()
    }
    fn value(&mut self, depth: usize) -> Result<Y, YamlError> {
        if depth > MAX_DEPTH {
            return Err(YamlError);
        }
        self.ws();
        match self.b.get(self.i) {
            Some('[') => {
                self.i += 1;
                let mut items = Vec::new();
                loop {
                    self.ws();
                    if self.b.get(self.i) == Some(&']') {
                        self.i += 1;
                        return Ok(Y::Seq(items));
                    }
                    items.push(self.value(depth + 1)?);
                    self.ws();
                    match self.b.get(self.i) {
                        Some(',') => self.i += 1,
                        Some(']') => {}
                        _ => return Err(YamlError),
                    }
                }
            }
            Some('{') => {
                self.i += 1;
                let mut kv = Vec::new();
                loop {
                    self.ws();
                    if self.b.get(self.i) == Some(&'}') {
                        self.i += 1;
                        return mapping(kv);
                    }
                    let k = unquote_key(&self.token())?;
                    self.ws();
                    if self.b.get(self.i) != Some(&':') {
                        return Err(YamlError);
                    }
                    self.i += 1;
                    let v = self.value(depth + 1)?;
                    kv.push((k, v));
                    self.ws();
                    match self.b.get(self.i) {
                        Some(',') => self.i += 1,
                        Some('}') => {}
                        _ => return Err(YamlError),
                    }
                }
            }
            _ => {
                let t = self.token();
                scalar(&t)
            }
        }
    }
}

impl P {
    fn skip_blank(&mut self) {
        while self.i < self.lines.len() {
            let t = strip_comment(&self.lines[self.i].text)
                .trim_end()
                .to_owned();
            if t.is_empty() || t == "---" || t == "..." {
                self.i += 1;
            } else {
                break;
            }
        }
    }
    fn cur(&mut self) -> Option<(usize, String)> {
        self.skip_blank();
        let l = self.lines.get(self.i)?;
        Some((l.indent, strip_comment(&l.text).trim_end().to_owned()))
    }
    /// A node whose first line is at `indent`.
    fn node(&mut self, indent: usize, depth: usize) -> Result<Y, YamlError> {
        if depth > MAX_DEPTH {
            return Err(YamlError);
        }
        let Some((ind, t)) = self.cur() else {
            return Ok(Y::Null);
        };
        if ind != indent {
            return Err(YamlError);
        }
        if is_seq_item(&t) {
            self.seq(indent, depth)
        } else if key_colon(&t).is_some() && !t.starts_with(['[', '{']) {
            self.map(indent, depth)
        } else {
            self.i += 1;
            self.inline_value(&t, indent, depth)
        }
    }
    /// The value text after `key:` or `- `, with continuation lines deeper than `indent`.
    fn inline_value(&mut self, t: &str, indent: usize, depth: usize) -> Result<Y, YamlError> {
        let t = t.trim();
        if t.starts_with('|') || t.starts_with('>') {
            // Block scalar: every following line deeper than `indent` (blank lines too).
            let mut body = Vec::new();
            while let Some(l) = self.lines.get(self.i) {
                if !l.text.is_empty() && l.indent <= indent {
                    break;
                }
                body.push(l.text.clone());
                self.i += 1;
            }
            return Ok(Y::Str(body.join("\n")));
        }
        if t.starts_with('[') || t.starts_with('{') {
            let mut text = t.to_owned();
            while flow(&text).is_err() {
                let Some((ind, more)) = self.cur() else {
                    return Err(YamlError);
                };
                if ind <= indent {
                    return Err(YamlError);
                }
                text.push(' ');
                text.push_str(more.trim());
                self.i += 1;
                if text.len() > 1 << 20 {
                    return Err(YamlError);
                }
            }
            return flow(&text);
        }
        // A plain scalar may continue on deeper lines.
        let mut text = t.to_owned();
        if !(t.starts_with('"') || t.starts_with('\'')) {
            while let Some((ind, more)) = self.cur() {
                if ind <= indent || is_seq_item(&more) || key_colon(&more).is_some() {
                    break;
                }
                text.push(' ');
                text.push_str(more.trim());
                self.i += 1;
            }
        } else if (t.starts_with('"') && !(t.len() > 1 && t.ends_with('"')))
            || (t.starts_with('\'') && !(t.len() > 1 && t.ends_with('\'')))
        {
            // A quoted scalar folded over lines.
            while let Some((_, more)) = self.cur() {
                text.push(' ');
                text.push_str(more.trim());
                self.i += 1;
                let closed = if t.starts_with('"') {
                    text.ends_with('"') && !text.ends_with("\\\"")
                } else {
                    text.ends_with('\'')
                };
                if closed {
                    break;
                }
            }
        }
        let _ = depth;
        scalar(&text)
    }
    fn map(&mut self, indent: usize, depth: usize) -> Result<Y, YamlError> {
        let mut kv: Vec<(String, Y)> = Vec::new();
        while let Some((ind, t)) = self.cur() {
            if ind < indent {
                break;
            }
            if ind > indent || is_seq_item(&t) {
                return Err(YamlError);
            }
            let colon = key_colon(&t).ok_or(YamlError)?;
            let key = unquote_key(&t[..colon])?;
            let rest = t[colon + 1..].trim().to_owned();
            self.i += 1;
            let value = if rest.is_empty() {
                match self.cur() {
                    Some((ind2, _)) if ind2 > indent => self.node(ind2, depth + 1)?,
                    // A sequence may sit at the key's own indent.
                    Some((ind2, t2)) if ind2 == indent && is_seq_item(&t2) => {
                        self.seq(indent, depth + 1)?
                    }
                    _ => Y::Null,
                }
            } else {
                self.inline_value(&rest, indent, depth + 1)?
            };
            kv.push((key, value));
        }
        mapping(kv)
    }
    fn seq(&mut self, indent: usize, depth: usize) -> Result<Y, YamlError> {
        let mut items = Vec::new();
        while let Some((ind, t)) = self.cur() {
            if ind < indent || (ind == indent && !is_seq_item(&t)) {
                break;
            }
            if ind > indent {
                return Err(YamlError);
            }
            let rest = t[1..].trim_start();
            if rest.is_empty() {
                self.i += 1;
                items.push(match self.cur() {
                    Some((ind2, _)) if ind2 > indent => self.node(ind2, depth + 1)?,
                    _ => Y::Null,
                });
                continue;
            }
            // `- key: v` or `- - x`: re-read the rest as a line at its own column.
            let offset = t.len() - rest.len();
            let col = indent + offset;
            let nested = is_seq_item(rest)
                || (key_colon(rest).is_some() && !rest.starts_with(['[', '{', '"', '\'']));
            if nested {
                self.lines[self.i] = Line {
                    indent: col,
                    text: rest.to_owned(),
                };
                items.push(self.node(col, depth + 1)?);
            } else {
                self.i += 1;
                items.push(self.inline_value(rest, indent, depth + 1)?);
            }
        }
        Ok(Y::Seq(items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> Y {
        Y::Str(v.to_owned())
    }

    #[test]
    fn reads_hermes_shaped_config() {
        let text = "\
# top comment
model:
  default: gpt-x   # trailing
  provider: 'open router'
plugins:
  enabled:
  - herdr-agent-state
  - codepet
  disabled: []
  entries:
    x: {a: 1, b: [p, q]}
hooks:
  pre_tool_call:
    - command: \"echo hi # not a comment\"
      matcher: Bash
      timeout: 5
hooks_auto_accept: false
note: |
  line one
  line two
empty:
quoted: \"a\\nb\"
";
        let y = parse(text).unwrap();
        let plugins = y.get("plugins").unwrap();
        assert_eq!(
            plugins.get("enabled").unwrap().str_set().unwrap(),
            vec!["codepet".to_owned(), "herdr-agent-state".to_owned()]
        );
        assert_eq!(plugins.get("disabled").unwrap(), &Y::Seq(vec![]));
        let x = plugins.get("entries").unwrap().get("x").unwrap();
        assert_eq!(x.get("b").unwrap(), &Y::Seq(vec![s("p"), s("q")]));
        let hooks = y.get("hooks").unwrap().get("pre_tool_call").unwrap();
        let Y::Seq(items) = hooks else { panic!() };
        assert_eq!(
            items[0].get("command").unwrap(),
            &s("echo hi # not a comment")
        );
        assert_eq!(items[0].get("timeout").unwrap(), &s("5"));
        assert_eq!(y.get("hooks_auto_accept").unwrap(), &Y::Bool(false));
        assert_eq!(y.get("note").unwrap(), &s("line one\nline two"));
        assert_eq!(y.get("empty").unwrap(), &Y::Null);
        assert_eq!(y.get("quoted").unwrap(), &s("a\nb"));
        assert_eq!(
            y.get("model").unwrap().get("provider").unwrap(),
            &s("open router")
        );
    }

    #[test]
    fn a_redump_compares_equal() {
        // The same content as PyYAML's safe_dump would write it (sorted, other indent).
        let a = "plugins:\n  enabled: [b, a]\n  scan_on_install: true\nx: 'y'\n";
        let b = "plugins:\n    enabled:\n    - b\n    - a\n    scan_on_install: true\nx: y\n";
        assert_eq!(parse(a).unwrap(), parse(b).unwrap());
    }

    #[test]
    fn plugin_name_lists_refuse_unknown_item_types() {
        assert_eq!(parse("[codepet, true]").unwrap().str_set(), None);
        assert_eq!(parse("[codepet, {name: other}]").unwrap().str_set(), None);
    }

    #[test]
    fn mapping_order_is_semantic_but_sequence_order_is_preserved() {
        assert_eq!(
            parse("hooks: {z: [{second: b, first: a}], a: []}\npermissions: {z: no, a: yes}\n")
                .unwrap(),
            parse("permissions: {a: yes, z: no}\nhooks: {a: [], z: [{first: a, second: b}]}\n")
                .unwrap()
        );
        assert_ne!(parse("a: [x, y]").unwrap(), parse("a: [y, x]").unwrap());
        for duplicate in [
            "a: x\na: y\n",
            "a: {b: x, b: y}",
            "a: [{b: x, b: y}]",
            "a:\n  b: x\n  'b': y\n",
        ] {
            assert_eq!(parse(duplicate), Err(YamlError), "{duplicate}");
        }
    }

    /// Manual probe (ignored): `CODEPET_YAML_PROBE=<file> cargo test yaml_probe -- --ignored`
    /// parses a real config in memory and prints only whether it parsed and how many
    /// top-level keys it has (never any content).
    #[test]
    #[ignore]
    fn yaml_probe() {
        let Some(p) = std::env::var_os("CODEPET_YAML_PROBE") else {
            return;
        };
        let text = std::fs::read_to_string(p).unwrap();
        match parse(&text) {
            Ok(Y::Map(kv)) => println!("yaml_probe: parsed, {} top-level keys", kv.len()),
            Ok(_) => println!("yaml_probe: parsed, not a mapping"),
            Err(_) => println!("yaml_probe: NOT parsed"),
        }
    }

    #[test]
    fn refuses_what_it_cannot_read() {
        for bad in [
            "a: &x 1\nb: *x\n",
            "? complex\n: key\n",
            "a: !tag v\n",
            "a: 1\n  b: 2\n",
            "a: [1, 2\n",
        ] {
            assert_eq!(parse(bad), Err(YamlError), "{bad:?}");
        }
        assert_eq!(parse("").unwrap(), Y::Null);
        assert_eq!(parse("# only\n---\n").unwrap(), Y::Null);
    }
}
