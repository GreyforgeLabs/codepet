//! The fixed v0.2 companion roster and pet-name normalization. Pure; no I/O.

use crate::redact_display_text;

/// Canonical species ids in roster order. Storage expands its `CHECK (species IN (...))`
/// clauses from this list, and the art registry uses the same order.
pub const SPECIES_IDS: [&str; 15] = [
    "cat",
    "dog",
    "capybara",
    "penguin",
    "robot",
    "iguana",
    "spider",
    "snake",
    "goldfish",
    "rabbit",
    "turtle",
    "frog",
    "owl",
    "red_panda",
    "axolotl",
];

/// Default companion names, index-aligned with [`SPECIES_IDS`].
pub const DEFAULT_NAMES: [&str; 15] = [
    "Ginger", "Biscuit", "Yuzu", "Pip", "Bolt", "Sol", "Lace", "Ribbon", "Bubbles", "Clover",
    "Atlas", "Hopper", "Quill", "Maple", "Lotus",
];

/// Longest accepted name, counted in Unicode scalar values.
pub const MAX_NAME_CHARS: usize = 24;
/// Longest accepted name in UTF-8 bytes (matches the `lives.name` CHECK).
pub const MAX_NAME_BYTES: usize = 96;

pub fn is_species(id: &str) -> bool {
    SPECIES_IDS.contains(&id)
}

pub fn default_name(id: &str) -> Option<&'static str> {
    SPECIES_IDS
        .iter()
        .position(|s| *s == id)
        .map(|i| DEFAULT_NAMES[i])
}

/// Trim; strip control and bidi characters (reuse privacy::redact_display_text);
/// 1..=24 chars; at most 96 bytes.
///
/// Whitespace runs (including tabs and newlines kept by the redactor) collapse to a
/// single space. A name the redactor would mask as a credential is refused rather
/// than stored as `[REDACTED]`.
pub fn normalize_pet_name(raw: &str) -> Result<String, &'static str> {
    let cleaned = redact_display_text(raw);
    if cleaned.contains("[REDACTED") && !raw.contains("[REDACTED") {
        return Err("name looks like a credential");
    }
    let mut out = String::with_capacity(cleaned.len());
    let mut pending_space = false;
    for c in cleaned.chars() {
        if c.is_whitespace() {
            pending_space = !out.is_empty();
        } else if c.is_control() {
            // The redactor keeps tab and newline; anything else here is dropped.
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            out.push(c);
        }
    }
    let chars = out.chars().count();
    if chars == 0 {
        return Err("name is empty");
    }
    if chars > MAX_NAME_CHARS {
        return Err("name is longer than 24 characters");
    }
    if out.len() > MAX_NAME_BYTES {
        return Err("name is longer than 96 bytes");
    }
    Ok(out)
}
