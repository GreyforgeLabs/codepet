//! Conservative best-effort display/persistence filtering, not a secret detector.
//! Call before hashing newly captured text so review identity covers the visible bytes.

/// Removes terminal/bidirectional controls and masks common credentials. A suspicious
/// credential assignment masks the remainder of that line deliberately: preserving an
/// exact command is less important than accidentally retaining a quoted secret.
/// Unknown secrets cannot be recognized; never supply credentials in task text.
pub fn redact_display_text(input: &str) -> String {
    let mut clean = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            match chars.next() {
                Some('[') => {
                    for next in chars.by_ref() {
                        if ('\u{40}'..='\u{7e}').contains(&next) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    while let Some(next) = chars.next() {
                        if next == '\u{7}' {
                            break;
                        }
                        if next == '\u{1b}' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else if (!c.is_control() || matches!(c, '\n' | '\t'))
            && !matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}')
        {
            clean.push(c);
        }
    }
    let mut result = String::with_capacity(clean.len());
    let mut private_key = false;
    for line in clean.split_inclusive('\n') {
        let lower = line.to_ascii_lowercase();
        if lower.contains("-----begin ") && lower.contains("private key-----") {
            private_key = true;
            result.push_str("[REDACTED PRIVATE KEY]\n");
            continue;
        }
        if private_key {
            if lower.contains("-----end ") && lower.contains("private key-----") {
                private_key = false;
            }
            continue;
        }
        let markers = [
            "authorization:",
            "bearer ",
            "ghp_",
            "github_pat_",
            "gho_",
            "sk-",
            "akia",
        ];
        let keys = [
            "api_key",
            "api-key",
            "apikey",
            "access_token",
            "access-token",
            "password",
            "passwd",
            "secret",
            "token",
        ];
        let direct = markers
            .iter()
            .flat_map(|m| lower.match_indices(m))
            .filter_map(|(at, _)| boundary(&lower, at).then_some(at));
        let assigned = keys
            .iter()
            .flat_map(|m| lower.match_indices(m))
            .filter_map(|(at, key)| {
                let after = lower[at + key.len()..].trim_start_matches(['\"', '\'', ' ', '\t']);
                let flag = at >= 2
                    && &lower.as_bytes()[at - 2..at] == b"--"
                    && lower[at + key.len()..].starts_with([' ', '\t', '=']);
                (boundary(&lower, at) && (flag || after.starts_with(['=', ':']))).then_some(at)
            });
        let cutoff = direct.chain(assigned).min();
        if let Some(at) = cutoff {
            result.push_str(&line[..at]);
            result.push_str("[REDACTED]");
            if line.ends_with('\n') {
                result.push('\n');
            }
        } else {
            result.push_str(line);
        }
    }
    result
}

fn boundary(text: &str, at: usize) -> bool {
    at == 0 || !text.as_bytes()[at - 1].is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn masks_common_credentials_and_multiline_keys() {
        let text = "cmd --api-key=very-secret argument\nAuthorization: Bearer secret\nUTF-8 猫 ghp_hidden\n-----BEGIN RSA PRIVATE KEY-----\nsecret bytes\n-----END RSA PRIVATE KEY-----\nvisible";
        let safe = redact_display_text(text);
        assert!(!safe.contains("very-secret"));
        assert!(!safe.contains("secret bytes"));
        assert!(!safe.contains("ghp_hidden"));
        assert!(safe.contains("猫 [REDACTED]"));
        assert!(safe.ends_with("visible"));
    }
    #[test]
    fn strips_terminal_hyperlinks_controls_and_bidi() {
        assert_eq!(redact_display_text("\x1b[31mred\x1b[0m\n\x1b]8;;https://secret.invalid\x07label\x1b]8;;\x07\u{202e}\x08"), "red\nlabel");
    }
    #[test]
    fn preserves_plain_text_and_is_idempotent() {
        let text = "Explain the password reset workflow. Read src/main.rs task-state\n\tTests failed: 3 assertions. 猫";
        assert_eq!(redact_display_text(text), text);
        for secret in [
            "API_KEY=123",
            "{\"password\": \"value\"}",
            "cmd --password hidden",
            "token = hidden",
            "猫password=hidden",
        ] {
            let safe = redact_display_text(secret);
            assert!(safe.contains("[REDACTED]"));
            assert!(!safe.contains("hidden"));
        }
        let safe = redact_display_text("API_KEY=123");
        assert_eq!(redact_display_text(&safe), safe);
    }
}
