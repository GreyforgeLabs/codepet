//! A small order-preserving JSON tree for the one shared client file CodePet edits,
//! Codex's `hooks.json` (spec §5.7). Codex trust keys are positional, so foreign groups
//! must keep their place and their keys their order; `serde_json`'s default map sorts keys
//! and enabling `preserve_order` would change every other JSON map in the workspace.
//!
//! Numbers keep their source lexeme; strings are decoded and re-encoded exactly as
//! `serde_json` encodes them, and [`to_pretty`] prints like `serde_json::to_string_pretty`.
//! So a file CodePet wrote itself round-trips byte for byte, and a foreign file keeps its
//! meaning (whitespace may change, as `commands.json` `serialization` allows).

/// One JSON value; objects keep their key order (duplicates are kept as found).
#[derive(Clone, Debug, PartialEq)]
pub enum J {
    Null,
    Bool(bool),
    /// The number's source lexeme (already validated as a JSON number).
    Num(String),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

impl J {
    pub fn get(&self, key: &str) -> Option<&J> {
        match self {
            J::Obj(kv) => kv.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn get_mut(&mut self, key: &str) -> Option<&mut J> {
        match self {
            J::Obj(kv) => kv.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            J::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_arr(&self) -> Option<&Vec<J>> {
        match self {
            J::Arr(a) => Some(a),
            _ => None,
        }
    }
    /// `serde_json::Value` of this tree (keys sorted, numbers parsed), for hashing and
    /// comparison. A number serde cannot represent becomes its lexeme as a string.
    pub fn to_value(&self) -> serde_json::Value {
        use serde_json::Value;
        match self {
            J::Null => Value::Null,
            J::Bool(b) => Value::Bool(*b),
            J::Num(n) => serde_json::from_str(n).unwrap_or_else(|_| Value::String(n.clone())),
            J::Str(s) => Value::String(s.clone()),
            J::Arr(a) => Value::Array(a.iter().map(J::to_value).collect()),
            J::Obj(kv) => {
                Value::Object(kv.iter().map(|(k, v)| (k.clone(), v.to_value())).collect())
            }
        }
    }
}

/// A parse failure (never shows file content).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseError;

/// Nesting deeper than this is refused (no unbounded recursion on a hostile file).
const MAX_DEPTH: usize = 64;

/// Parses one JSON document (surrounding whitespace allowed).
pub fn parse(text: &str) -> Result<J, ParseError> {
    let mut p = Parser {
        b: text.as_bytes(),
        i: 0,
    };
    p.ws();
    let v = p.value(0)?;
    p.ws();
    if p.i != p.b.len() {
        return Err(ParseError);
    }
    Ok(v)
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }
    fn eat(&mut self, c: u8) -> bool {
        if self.b.get(self.i) == Some(&c) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn lit(&mut self, word: &str) -> Result<(), ParseError> {
        if self.b[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(())
        } else {
            Err(ParseError)
        }
    }
    fn value(&mut self, depth: usize) -> Result<J, ParseError> {
        if depth > MAX_DEPTH {
            return Err(ParseError);
        }
        match self.b.get(self.i).copied() {
            Some(b'{') => {
                self.i += 1;
                let mut kv = Vec::new();
                self.ws();
                if self.eat(b'}') {
                    return Ok(J::Obj(kv));
                }
                loop {
                    self.ws();
                    let k = self.string()?;
                    self.ws();
                    if !self.eat(b':') {
                        return Err(ParseError);
                    }
                    self.ws();
                    let v = self.value(depth + 1)?;
                    kv.push((k, v));
                    self.ws();
                    if self.eat(b',') {
                        continue;
                    }
                    if self.eat(b'}') {
                        return Ok(J::Obj(kv));
                    }
                    return Err(ParseError);
                }
            }
            Some(b'[') => {
                self.i += 1;
                let mut a = Vec::new();
                self.ws();
                if self.eat(b']') {
                    return Ok(J::Arr(a));
                }
                loop {
                    self.ws();
                    a.push(self.value(depth + 1)?);
                    self.ws();
                    if self.eat(b',') {
                        continue;
                    }
                    if self.eat(b']') {
                        return Ok(J::Arr(a));
                    }
                    return Err(ParseError);
                }
            }
            Some(b'"') => Ok(J::Str(self.string()?)),
            Some(b't') => self.lit("true").map(|()| J::Bool(true)),
            Some(b'f') => self.lit("false").map(|()| J::Bool(false)),
            Some(b'n') => self.lit("null").map(|()| J::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(ParseError),
        }
    }
    fn digits(&mut self) -> usize {
        let start = self.i;
        while self.i < self.b.len() && self.b[self.i].is_ascii_digit() {
            self.i += 1;
        }
        self.i - start
    }
    fn number(&mut self) -> Result<J, ParseError> {
        let start = self.i;
        self.eat(b'-');
        if self.eat(b'0') {
        } else if self.digits() == 0 {
            return Err(ParseError);
        }
        if self.eat(b'.') && self.digits() == 0 {
            return Err(ParseError);
        }
        if self.eat(b'e') || self.eat(b'E') {
            let _ = self.eat(b'+') || self.eat(b'-');
            if self.digits() == 0 {
                return Err(ParseError);
            }
        }
        let lexeme = std::str::from_utf8(&self.b[start..self.i]).map_err(|_| ParseError)?;
        Ok(J::Num(lexeme.to_owned()))
    }
    fn hex4(&mut self) -> Result<u32, ParseError> {
        let s = self.b.get(self.i..self.i + 4).ok_or(ParseError)?;
        let s = std::str::from_utf8(s).map_err(|_| ParseError)?;
        let v = u32::from_str_radix(s, 16).map_err(|_| ParseError)?;
        self.i += 4;
        Ok(v)
    }
    fn string(&mut self) -> Result<String, ParseError> {
        if !self.eat(b'"') {
            return Err(ParseError);
        }
        let mut out: Vec<u8> = Vec::new();
        loop {
            let c = *self.b.get(self.i).ok_or(ParseError)?;
            self.i += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let e = *self.b.get(self.i).ok_or(ParseError)?;
                    self.i += 1;
                    let ch = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let hi = self.hex4()?;
                            let cp = if (0xD800..0xDC00).contains(&hi) {
                                if !(self.eat(b'\\') && self.eat(b'u')) {
                                    return Err(ParseError);
                                }
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return Err(ParseError);
                                }
                                0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                            } else {
                                hi
                            };
                            char::from_u32(cp).ok_or(ParseError)?
                        }
                        _ => return Err(ParseError),
                    };
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                }
                0..=0x1f => return Err(ParseError),
                _ => out.push(c),
            }
        }
        String::from_utf8(out).map_err(|_| ParseError)
    }
}

/// Encodes a string exactly as `serde_json` does.
fn push_str(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn pretty(out: &mut String, v: &J, indent: usize) {
    let pad = |out: &mut String, n: usize| out.extend(std::iter::repeat_n("  ", n));
    match v {
        J::Null => out.push_str("null"),
        J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        J::Num(n) => out.push_str(n),
        J::Str(s) => push_str(out, s),
        J::Arr(a) if a.is_empty() => out.push_str("[]"),
        J::Obj(kv) if kv.is_empty() => out.push_str("{}"),
        J::Arr(a) => {
            out.push_str("[\n");
            for (i, item) in a.iter().enumerate() {
                pad(out, indent + 1);
                pretty(out, item, indent + 1);
                out.push_str(if i + 1 < a.len() { ",\n" } else { "\n" });
            }
            pad(out, indent);
            out.push(']');
        }
        J::Obj(kv) => {
            out.push_str("{\n");
            for (i, (k, item)) in kv.iter().enumerate() {
                pad(out, indent + 1);
                push_str(out, k);
                out.push_str(": ");
                pretty(out, item, indent + 1);
                out.push_str(if i + 1 < kv.len() { ",\n" } else { "\n" });
            }
            pad(out, indent);
            out.push('}');
        }
    }
}

/// Like `serde_json::to_string_pretty` (two-space indent), without a trailing newline.
pub fn to_pretty(v: &J) -> String {
    let mut out = String::new();
    pretty(&mut out, v, 0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_serde_pretty_output_byte_for_byte() {
        let text = r#"{"hooks":{"Stop":[{"matcher":"^Bash$","hooks":[{"type":"command","command":"a \"b\" \\ é\t","timeout":600,"async":false}]}],"Empty":[],"E":{}},"x":-1.5e3,"n":null}"#;
        let serde: serde_json::Value = serde_json::from_str(text).unwrap();
        let tree = parse(text).unwrap();
        assert_eq!(tree.to_value(), serde);
        // Key order is the file's, not sorted.
        let J::Obj(top) = &tree else { panic!() };
        assert_eq!(top[0].0, "hooks");
        let pretty = to_pretty(&tree);
        assert_eq!(parse(&pretty).unwrap(), tree);
        assert_eq!(to_pretty(&parse(&pretty).unwrap()), pretty);
        let stop = &tree
            .get("hooks")
            .unwrap()
            .get("Stop")
            .unwrap()
            .as_arr()
            .unwrap()[0];
        let J::Obj(group) = stop else { panic!() };
        assert_eq!(group[0].0, "matcher", "order preserved");
    }

    #[test]
    fn matches_serde_pretty_for_ordered_input() {
        // serde_json sorts keys; with keys already sorted both printers agree.
        let text = r#"{"a":[1,2,{"b":"\u0001\u001f/"}],"c":{"d":true}}"#;
        let serde: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(
            to_pretty(&parse(text).unwrap()),
            serde_json::to_string_pretty(&serde).unwrap()
        );
    }

    #[test]
    fn refuses_bad_documents() {
        for bad in [
            "",
            "{",
            "[1,]",
            "{\"a\":}",
            "{\"a\" 1}",
            "01",
            "1.",
            "-",
            "\"\\x\"",
            "nul",
            "{} {}",
            "\"a\nb\"",
            "\"\\ud800\"",
        ] {
            assert_eq!(parse(bad), Err(ParseError), "{bad:?}");
        }
        let deep = "[".repeat(100) + &"]".repeat(100);
        assert_eq!(parse(&deep), Err(ParseError));
        assert!(parse("\"\\ud83d\\ude00\"").is_ok());
    }
}
