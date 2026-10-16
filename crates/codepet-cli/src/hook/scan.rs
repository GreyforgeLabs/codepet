//! The hook's top-level-only JSON tokenizer (spec §5.8). It keeps only allow-listed depth-1
//! keys (`codepet_core::agent::ALLOWED_KEYS`) and skips every other value, nested values
//! included, without buffering or recursing, so a prompt that contains
//! `"session_id":"x"` can never spoof an id and deep nesting cannot overflow a stack.
//! Captured strings are decoded and kept only up to `MAX_ID_FIELD_BYTES`.
use codepet_core::agent::{Field, Payload, ALLOWED_KEYS, MAX_ID_FIELD_BYTES};

/// Keys longer than this are never allow-listed; they are skipped like any value.
const MAX_KEY_BYTES: usize = 32;

/// How the scan ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    /// The top-level object closed (only whitespace may follow).
    Complete,
    /// The bytes ran out inside the object.
    Truncated,
}

/// Not a JSON object (or malformed where the tokenizer had to look).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Invalid;

struct Cursor<'a> {
    b: &'a [u8],
    i: usize,
}

/// Running out of input is distinct from bad input.
enum Stop {
    Eof,
    Bad,
}
type Step<T> = Result<T, Stop>;

impl Cursor<'_> {
    fn peek(&self) -> Step<u8> {
        self.b.get(self.i).copied().ok_or(Stop::Eof)
    }
    fn ws(&mut self) {
        while matches!(self.b.get(self.i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }
    fn eat(&mut self, c: u8) -> Step<()> {
        if self.peek()? == c {
            self.i += 1;
            Ok(())
        } else {
            Err(Stop::Bad)
        }
    }
    fn literal(&mut self, word: &[u8]) -> Step<()> {
        for &c in word {
            self.eat(c)?;
        }
        Ok(())
    }

    /// Skips a string body after its opening quote.
    fn skip_string(&mut self) -> Step<()> {
        loop {
            match self.peek()? {
                b'"' => {
                    self.i += 1;
                    return Ok(());
                }
                b'\\' => {
                    self.i += 1;
                    if !matches!(
                        self.peek()?,
                        b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' | b'u'
                    ) {
                        return Err(Stop::Bad);
                    }
                    self.i += 1;
                }
                _ => self.i += 1,
            }
        }
    }

    /// Reads a string body after its opening quote, decoding it when it is at most `cap`
    /// bytes; `Ok(None)` when longer, not UTF-8, or holding a control character.
    fn string(&mut self, cap: usize) -> Step<Option<String>> {
        let mut out: Vec<u8> = Vec::new();
        let mut keep = true;
        loop {
            let c = self.peek()?;
            self.i += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let e = self.peek()?;
                    self.i += 1;
                    let ch: Option<char> = match e {
                        b'"' => Some('"'),
                        b'\\' => Some('\\'),
                        b'/' => Some('/'),
                        b'b' => Some('\u{8}'),
                        b'f' => Some('\u{c}'),
                        b'n' => Some('\n'),
                        b'r' => Some('\r'),
                        b't' => Some('\t'),
                        b'u' => {
                            let hi = self.hex4()?;
                            if (0xD800..0xDC00).contains(&hi) {
                                // A surrogate pair: `\uD83D\uDE00`.
                                if self.peek()? == b'\\' {
                                    self.i += 1;
                                    self.eat(b'u')?;
                                    let lo = self.hex4()?;
                                    if (0xDC00..0xE000).contains(&lo) {
                                        char::from_u32(
                                            0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00),
                                        )
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else {
                                char::from_u32(hi)
                            }
                        }
                        _ => return Err(Stop::Bad),
                    };
                    match ch {
                        Some(ch) if keep => {
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        Some(_) => {}
                        None => keep = false,
                    }
                }
                c if c < 0x20 => keep = false,
                c => {
                    if keep {
                        out.push(c);
                    }
                }
            }
            if out.len() > cap {
                keep = false;
                out.clear();
            }
        }
        if !keep {
            return Ok(None);
        }
        Ok(String::from_utf8(out).ok())
    }

    fn hex4(&mut self) -> Step<u32> {
        let mut v = 0u32;
        for _ in 0..4 {
            let c = self.peek()?;
            self.i += 1;
            let d = (c as char).to_digit(16).ok_or(Stop::Bad)?;
            v = v * 16 + d;
        }
        Ok(v)
    }

    /// A number token; `Some` when it is an integer that fits `i64`.
    fn number(&mut self) -> Step<Option<i64>> {
        let start = self.i;
        let mut integer = true;
        while let Some(&c) = self.b.get(self.i) {
            match c {
                b'0'..=b'9' | b'-' | b'+' => {}
                b'.' | b'e' | b'E' => integer = false,
                _ => break,
            }
            self.i += 1;
        }
        if self.i == self.b.len() {
            // A number at the very end may continue in the bytes not read.
            return Err(Stop::Eof);
        }
        let text = std::str::from_utf8(&self.b[start..self.i]).map_err(|_| Stop::Bad)?;
        if text.is_empty() || text == "-" {
            return Err(Stop::Bad);
        }
        Ok(if integer { text.parse().ok() } else { None })
    }

    /// Skips any value without recursion: containers are only counted (O(1) memory), so
    /// the grammar inside skipped containers is checked loosely (separators, balance).
    fn skip_value(&mut self) -> Step<()> {
        let mut depth = 0usize;
        loop {
            // A value is expected (a key inside a skipped object counts as one).
            self.ws();
            match self.peek()? {
                b'{' | b'[' => {
                    self.i += 1;
                    depth += 1;
                    self.ws();
                    if !matches!(self.peek()?, b'}' | b']') {
                        continue;
                    }
                    self.i += 1;
                    depth -= 1;
                }
                b'"' => {
                    self.i += 1;
                    self.skip_string()?;
                }
                b't' => self.literal(b"true")?,
                b'f' => self.literal(b"false")?,
                b'n' => self.literal(b"null")?,
                b'-' | b'0'..=b'9' => {
                    self.number()?;
                }
                _ => return Err(Stop::Bad),
            }
            // After a value: close containers until a separator asks for the next value.
            loop {
                if depth == 0 {
                    return Ok(());
                }
                self.ws();
                match self.peek()? {
                    b',' | b':' => {
                        self.i += 1;
                        break;
                    }
                    b'}' | b']' => {
                        self.i += 1;
                        depth -= 1;
                    }
                    _ => return Err(Stop::Bad),
                }
            }
        }
    }

    /// A captured allow-listed value.
    fn field(&mut self) -> Step<Field> {
        self.ws();
        Ok(match self.peek()? {
            b'"' => {
                self.i += 1;
                match self.string(MAX_ID_FIELD_BYTES)? {
                    Some(s) => Field::Str(s),
                    None => Field::Other,
                }
            }
            b't' => {
                self.literal(b"true")?;
                Field::Bool(true)
            }
            b'f' => {
                self.literal(b"false")?;
                Field::Bool(false)
            }
            b'n' => {
                self.literal(b"null")?;
                Field::Null
            }
            b'-' | b'0'..=b'9' => match self.number()? {
                Some(n) => Field::Int(n),
                None => Field::Other,
            },
            _ => {
                self.skip_value()?;
                Field::Other
            }
        })
    }

    fn object(&mut self, p: &mut Payload) -> Step<()> {
        self.ws();
        self.eat(b'{')?;
        self.ws();
        if self.peek()? == b'}' {
            self.i += 1;
            return Ok(());
        }
        loop {
            self.ws();
            self.eat(b'"')?;
            let key = self.string(MAX_KEY_BYTES)?;
            self.ws();
            self.eat(b':')?;
            match key.as_deref().filter(|k| ALLOWED_KEYS.contains(k)) {
                Some(k) => {
                    let v = self.field()?;
                    p.set(k, v);
                }
                None => self.skip_value()?,
            }
            self.ws();
            match self.peek()? {
                b',' => self.i += 1,
                b'}' => {
                    self.i += 1;
                    return Ok(());
                }
                _ => return Err(Stop::Bad),
            }
        }
    }
}

/// Scans the bytes the hook read. `capped` says the read stopped at the size cap (more
/// input may follow), which is the only case in which a cut-off object is `Truncated`
/// rather than invalid.
pub fn scan(bytes: &[u8], capped: bool) -> Result<(Payload, End), Invalid> {
    let mut p = Payload::default();
    let mut c = Cursor { b: bytes, i: 0 };
    match c.object(&mut p) {
        Ok(()) => {
            c.ws();
            if c.i == bytes.len() {
                Ok((p, End::Complete))
            } else {
                Err(Invalid)
            }
        }
        Err(Stop::Eof) if capped => Ok((p, End::Truncated)),
        Err(_) => Err(Invalid),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(s: &str) -> Payload {
        let (p, end) = scan(s.as_bytes(), false).unwrap();
        assert_eq!(end, End::Complete);
        p
    }

    #[test]
    fn keeps_only_top_level_allow_listed_keys() {
        let p = ok(
            r#"{"hook_event_name":"PreToolUse","session_id":"abc","tool_input":
            {"command":"ls","session_id":"spoof","deep":[1,{"session_id":"x"}]},
            "prompt":"say \"session_id\":\"x\"","tool_use_id":"toolu_1"}"#,
        );
        assert_eq!(p.get("session_id"), Some(&Field::Str("abc".into())));
        assert_eq!(p.get("tool_use_id"), Some(&Field::Str("toolu_1".into())));
        assert_eq!(p.get("prompt"), None);
        assert_eq!(p.get("tool_input"), None);
    }

    #[test]
    fn values_have_their_json_types() {
        let p = ok(
            r#"{"codepet_v":1,"tools":-3,"interactive":false,"event":null,
            "approval_key":1.5,"turn_id":{"a":1},"error":[1,2]}"#,
        );
        assert_eq!(p.get("codepet_v"), Some(&Field::Int(1)));
        assert_eq!(p.get("tools"), Some(&Field::Int(-3)));
        assert_eq!(p.get("interactive"), Some(&Field::Bool(false)));
        assert_eq!(p.get("event"), Some(&Field::Null));
        assert_eq!(p.get("approval_key"), Some(&Field::Other));
        assert_eq!(p.get("turn_id"), Some(&Field::Other));
        assert_eq!(p.get("error"), Some(&Field::Other));
        let p = ok(r#"{"codepet_v":1e0,"tools":99999999999999999999}"#);
        assert_eq!(p.get("codepet_v"), Some(&Field::Other));
        assert_eq!(p.get("tools"), Some(&Field::Other));
    }

    #[test]
    fn strings_decode_escapes_and_respect_the_cap() {
        let p = ok(r#"{"session_id":"a\u0062c\/d","hook\u005fevent_name":"Stop"}"#);
        assert_eq!(p.get("session_id"), Some(&Field::Str("abc/d".into())));
        assert_eq!(p.get("hook_event_name"), Some(&Field::Str("Stop".into())));
        let long = format!(r#"{{"session_id":"{}"}}"#, "a".repeat(129));
        assert_eq!(ok(&long).get("session_id"), Some(&Field::Other));
        let edge = format!(r#"{{"session_id":"{}"}}"#, "a".repeat(128));
        assert!(matches!(ok(&edge).get("session_id"), Some(Field::Str(_))));
        let p = ok(r#"{"session_id":"\ud83d\ude00","turn_id":"a\u0001"}"#);
        assert_eq!(p.get("session_id"), Some(&Field::Str("\u{1F600}".into())));
        // An escaped control character is valid JSON (and later fails the id check).
        assert_eq!(p.get("turn_id"), Some(&Field::Str("a\u{1}".into())));
        // Last occurrence wins, as in a JSON parser.
        assert_eq!(
            ok(r#"{"session_id":"a","session_id":"b"}"#).get("session_id"),
            Some(&Field::Str("b".into()))
        );
    }

    #[test]
    fn malformed_and_truncated_inputs() {
        for bad in [
            "",
            "[]",
            "\"x\"",
            "{",
            "{\"a\"}",
            "{\"a\":}",
            "{\"a\":1,}",
            "{\"a\":1} x",
            "{\"a\":tru}",
            "{\"a\":\"\\q\"}",
            "{\"a\":[1 2]}",
        ] {
            assert!(scan(bad.as_bytes(), false).is_err(), "{bad:?}");
        }
        assert!(scan(b"{\"a\":\xff\xfe}", false).is_err());
        // Non-UTF-8 inside a skipped string is skipped, not decoded.
        assert!(scan(b"{\"prompt\":\"\xff\xfe\",\"session_id\":\"s\"}", false).is_ok());
        let cut =
            br#"{"hook_event_name":"PreToolUse","session_id":"s","tool_input":{"content":"aaaa"#;
        let (p, end) = scan(cut, true).unwrap();
        assert_eq!(end, End::Truncated);
        assert_eq!(p.get("session_id"), Some(&Field::Str("s".into())));
        assert!(
            scan(cut, false).is_err(),
            "a cut at EOF is malformed, not truncated"
        );
    }

    #[test]
    fn deep_nesting_needs_no_stack() {
        let depth = 1_000_000;
        let mut s = String::from(r#"{"x":"#);
        s.push_str(&"[".repeat(depth));
        s.push_str(&"]".repeat(depth));
        s.push_str(r#","session_id":"s"}"#);
        let p = ok(&s);
        assert_eq!(p.get("session_id"), Some(&Field::Str("s".into())));
        let mut o = String::from(r#"{"x":"#);
        o.push_str(&r#"{"a":"#.repeat(1000));
        o.push('1');
        o.push_str(&"}".repeat(1000));
        o.push_str(r#","session_id":"t"}"#);
        assert_eq!(ok(&o).get("session_id"), Some(&Field::Str("t".into())));
    }

    #[test]
    fn agrees_with_serde_on_valid_objects() {
        let samples = [
            r#"{"a":{"b":[1,2,{"c":"}"}],"d":"]"},"session_id":"x1","tools":7}"#,
            r#"{"e":[[],[[]],{}],"session_id":"y","interactive":true}"#,
            r#"{"z":-0.5e+10,"session_id":"q","codepet_v":1}"#,
        ];
        for s in samples {
            let v: serde_json::Value = serde_json::from_str(s).unwrap();
            let p = ok(s);
            assert_eq!(
                p.get("session_id"),
                Some(&Field::Str(v["session_id"].as_str().unwrap().into())),
                "{s}"
            );
        }
    }
}
