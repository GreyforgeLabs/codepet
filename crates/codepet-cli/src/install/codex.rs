//! Codex CLI (spec §5.7): CodePet's six groups in the shared `~/.codex/hooks.json`, the
//! only shared-file edit of v0.3. Codex trust keys are positional
//! (`<path>:<snake_event>:<group>:<handler>`), so CodePet never inserts, reorders or
//! removes a group that a foreign group follows:
//! - Connect: an exactly equal CodePet group is kept; an older or tombstoned CodePet group
//!   is replaced in place; otherwise the group is appended;
//! - Disconnect: a CodePet group is removed only while no foreign group follows it in its
//!   array; otherwise it stays as a tombstone (the `disconnected` flag makes the shim exit),
//!   and a later pass removes it once it is last;
//! - every write keeps `hooks.json.codepet-orig` (first touch) and `hooks.json.codepet-last`
//!   (the bytes before this write) beside the file, compares and swaps against concurrent
//!   writers (3 tries, then `blocked`), keeps the file's mode, re-reads it, and checks that
//!   every foreign `[hooks.state]` position still holds the same handler (else it restores
//!   `codepet-last` and fails `blocked`).
//!
//! `config.toml` is read only for `[features] hooks`/`codex_hooks` and `[hooks.state]`; it
//! is never written, logged or copied. CodePet never writes `trusted_hash`
//! (§11 decision 4): the user trusts the lines in Codex's `/hooks`.
use super::jsonish::{self, J};
use super::tomlish::{self, CodexConfig};
use super::{Artifact, Connected, Ctx, Failed, Reason};
use codepet_core::agent::ClientId;
use codepet_storage::{STEP_CODEX_HOOKS_OFF, STEP_CODEX_TRUST};
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub const TEMPLATE: &str = include_str!("../../../../integrations/codex/groups.json.in");
const MAX_FILE: u64 = 4 * 1024 * 1024;
const CAS_TRIES: u32 = 3;

pub fn hooks_path(ctx: &Ctx) -> PathBuf {
    ctx.env.codex_dir.join("hooks.json")
}
fn config_path(ctx: &Ctx) -> PathBuf {
    ctx.env.codex_dir.join("config.toml")
}

/// `UserPromptSubmit` -> `user_prompt_submit`.
pub fn snake(event: &str) -> String {
    let mut out = String::new();
    for (i, c) in event.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// CodePet's (event, group) pairs, in `commands.json` order.
pub fn groups(ctx: &Ctx) -> Result<Vec<(String, J)>, Failed> {
    let text = ctx.render(TEMPLATE)?;
    let tree = jsonish::parse(&text).map_err(|_| Failed::new(Reason::Blocked, "bad template"))?;
    let mut out = Vec::new();
    for item in tree.as_arr().into_iter().flatten() {
        let event = item
            .get("event")
            .and_then(J::as_str)
            .unwrap_or_default()
            .to_owned();
        let group = item.get("group").cloned().unwrap_or(J::Null);
        out.push((event, group));
    }
    Ok(out)
}

/// A group every handler of which runs a CodePet Codex command (current or older).
pub fn is_codepet_group(g: &J) -> bool {
    g.get("hooks").and_then(J::as_arr).is_some_and(|hs| {
        !hs.is_empty()
            && hs.iter().all(|h| {
                h.get("command")
                    .and_then(J::as_str)
                    .is_some_and(|c| super::is_our_command(c, ClientId::Codex))
            })
    })
}

/// Codex's trust fingerprint of one handler (`config/src/fingerprint.rs`): sha256 of the
/// compact sorted-key JSON `{event_name, matcher?, hooks:[{async, command, timeout, type}]}`
/// with `async` false and `timeout` 600 when omitted, and `timeout` clamped to 1-3 s for
/// `session_end` and `interrupt`.
pub fn handler_hash(snake_event: &str, matcher: Option<&J>, handler: &J) -> String {
    let mut timeout = match handler.get("timeout") {
        Some(J::Num(n)) => n.parse::<f64>().map(|t| t as i64).unwrap_or(600),
        _ => 600,
    };
    if matches!(snake_event, "session_end" | "interrupt") {
        timeout = timeout.clamp(1, 3);
    }
    let mut obj = serde_json::Map::new();
    obj.insert("event_name".into(), snake_event.into());
    if let Some(m) = matcher.filter(|m| **m != J::Null) {
        obj.insert("matcher".into(), m.to_value());
    }
    let h = serde_json::json!({
        "async": matches!(handler.get("async"), Some(J::Bool(true))),
        "command": handler.get("command").map(J::to_value).unwrap_or(serde_json::Value::Null),
        "timeout": timeout,
        "type": handler.get("type").map(J::to_value).unwrap_or(serde_json::Value::Null),
    });
    obj.insert("hooks".into(), serde_json::Value::Array(vec![h]));
    let compact = serde_json::Value::Object(obj).to_string();
    format!("sha256:{}", codepet_storage::sha256_hex(compact.as_bytes()))
}

/// The fingerprint at `<snake_event>:<group>:<handler>` of a parsed `hooks.json`.
fn hash_at(tree: &J, snake_event: &str, g: usize, h: usize) -> Option<String> {
    let J::Obj(events) = tree.get("hooks")? else {
        return None;
    };
    let (_, arr) = events.iter().find(|(k, _)| snake(k) == snake_event)?;
    let group = arr.as_arr()?.get(g)?;
    let handler = group.get("hooks")?.as_arr()?.get(h)?;
    Some(handler_hash(snake_event, group.get("matcher"), handler))
}

/// The trust-key prefixes Codex may use for this `hooks.json` (literal and canonical).
fn key_prefixes(path: &Path) -> Vec<String> {
    let mut out = vec![path.to_string_lossy().into_owned()];
    if let Ok(c) = fs::canonicalize(path) {
        let c = c.to_string_lossy().into_owned();
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

/// Parses `<prefix>:<snake>:<g>:<h>` for one of `prefixes`.
fn parse_key<'a>(key: &'a str, prefixes: &[String]) -> Option<(&'a str, usize, usize)> {
    for p in prefixes {
        if let Some(rest) = key
            .strip_prefix(p.as_str())
            .and_then(|r| r.strip_prefix(':'))
        {
            let mut it = rest.rsplitn(3, ':');
            let h = it.next()?.parse().ok()?;
            let g = it.next()?.parse().ok()?;
            let ev = it.next()?;
            return Some((ev, g, h));
        }
    }
    None
}

fn read_config(ctx: &Ctx) -> CodexConfig {
    match fs::metadata(config_path(ctx)) {
        Ok(m) if m.len() <= MAX_FILE => fs::read_to_string(config_path(ctx))
            .map(|t| tomlish::read(&t))
            .unwrap_or_default(),
        _ => CodexConfig::default(),
    }
}

/// What the files say about Codex (file-only).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodexStatus {
    pub artifact: Artifact,
    pub hooks_off: bool,
    /// CodePet lines trusted with the expected hash, out of [`CodexStatus::lines`].
    pub trusted: usize,
    /// CodePet lines present (exact groups).
    pub lines: usize,
    /// CodePet lines turned off in `/hooks` (`enabled = false`).
    pub disabled: usize,
    /// CodePet groups followed by a foreign group (a Disconnect would tombstone them).
    pub followed: usize,
    /// The `needs_step` the files prove: `Some(None)` clears, `Some(Some(_))` sets, `None`
    /// keeps the stored value.
    pub needs_step: Option<Option<&'static str>>,
}

fn read_tree(path: &Path) -> io::Result<Option<(Vec<u8>, J)>> {
    match fs::metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
        Ok(m) if m.len() > MAX_FILE => return Err(io::Error::other("hooks.json is too large")),
        Ok(_) => {}
    }
    let bytes = fs::read(path)?;
    let text =
        std::str::from_utf8(&bytes).map_err(|_| io::Error::other("hooks.json is not UTF-8"))?;
    let tree = jsonish::parse(text).map_err(|_| io::Error::other("hooks.json is not JSON"))?;
    Ok(Some((bytes, tree)))
}

fn event_array<'a>(tree: &'a J, event: &str) -> Option<&'a Vec<J>> {
    tree.get("hooks")?.get(event)?.as_arr()
}

pub fn status(ctx: &Ctx) -> CodexStatus {
    let cfg = read_config(ctx);
    let mut st = CodexStatus {
        artifact: Artifact::Missing,
        hooks_off: cfg.hooks_off,
        trusted: 0,
        lines: 0,
        disabled: 0,
        followed: 0,
        needs_step: None,
    };
    let path = hooks_path(ctx);
    let tree = read_tree(&path).ok().flatten().map(|(_, t)| t);
    let wanted = groups(ctx).unwrap_or_default();
    let prefixes = key_prefixes(&path);
    let mut any_ours = false;
    if let Some(tree) = &tree {
        if let Some(J::Obj(events)) = tree.get("hooks") {
            for (ev, arr) in events {
                if let Some(arr) = arr.as_arr() {
                    let last_foreign = arr.iter().rposition(|g| !is_codepet_group(g));
                    for (i, g) in arr.iter().enumerate() {
                        if is_codepet_group(g) {
                            any_ours = true;
                            if last_foreign.is_some_and(|f| f > i) {
                                st.followed += 1;
                            }
                        }
                    }
                }
                let _ = ev;
            }
        }
        for (event, group) in &wanted {
            let Some(arr) = event_array(tree, event) else {
                continue;
            };
            let Some(i) = arr.iter().position(|g| g == group) else {
                continue;
            };
            st.lines += 1;
            let sn = snake(event);
            let expected = hash_at(tree, &sn, i, 0);
            let entry = prefixes
                .iter()
                .find_map(|p| cfg.trust_for(&format!("{p}:{sn}:{i}:0")));
            if let Some(e) = entry {
                if e.enabled == Some(false) {
                    st.disabled += 1;
                } else if e.trusted_hash.is_some() && e.trusted_hash == expected {
                    st.trusted += 1;
                }
            }
        }
    }
    st.artifact = if !wanted.is_empty() && st.lines == wanted.len() {
        Artifact::Current
    } else if any_ours {
        Artifact::Stale
    } else {
        Artifact::Missing
    };
    st.needs_step = if st.hooks_off || st.disabled > 0 {
        Some(Some(STEP_CODEX_HOOKS_OFF))
    } else if (st.artifact == Artifact::Current && st.trusted == st.lines)
        || st.artifact == Artifact::Missing
    {
        Some(None)
    } else {
        None
    };
    st
}

/// Positions (snake event, group, handler, fingerprint) of every `[hooks.state]` entry for
/// this file that sits on a foreign handler of `tree`.
fn foreign_positions(
    tree: &J,
    cfg: &CodexConfig,
    prefixes: &[String],
) -> Vec<(String, usize, usize, String)> {
    let mut out = Vec::new();
    for t in &cfg.trust {
        let Some((ev, g, h)) = parse_key(&t.key, prefixes) else {
            continue;
        };
        let group = tree
            .get("hooks")
            .and_then(|hs| match hs {
                J::Obj(events) => events.iter().find(|(k, _)| snake(k) == ev).map(|(_, v)| v),
                _ => None,
            })
            .and_then(J::as_arr)
            .and_then(|a| a.get(g));
        if group.is_some_and(|g| !is_codepet_group(g)) {
            if let Some(hash) = hash_at(tree, ev, g, h) {
                out.push((ev.to_owned(), g, h, hash));
            }
        }
    }
    out
}

/// What a merge did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MergeReport {
    pub wrote: bool,
    /// CodePet groups left in place because a foreign group follows them.
    pub tombstoned: usize,
}

/// Connect: appends or refreshes CodePet's groups.
fn add_groups(tree: &mut J, wanted: &[(String, J)]) -> Result<bool, Failed> {
    let bad = || Failed::new(Reason::Blocked, "hooks.json has an unexpected shape");
    let J::Obj(top) = tree else {
        return Err(bad());
    };
    if !top.iter().any(|(k, _)| k == "hooks") {
        top.push(("hooks".into(), J::Obj(Vec::new())));
    }
    let hooks = tree.get_mut("hooks").ok_or_else(bad)?;
    let J::Obj(events) = hooks else {
        return Err(bad());
    };
    let mut changed = false;
    for (event, group) in wanted {
        if !events.iter().any(|(k, _)| k == event) {
            events.push((event.clone(), J::Arr(Vec::new())));
        }
        let (_, arr) = events
            .iter_mut()
            .find(|(k, _)| k == event)
            .expect("just ensured");
        let J::Arr(arr) = arr else {
            return Err(bad());
        };
        if arr.iter().any(|g| g == group) {
            continue;
        }
        if let Some(i) = arr.iter().position(is_codepet_group) {
            arr[i] = group.clone();
        } else {
            arr.push(group.clone());
        }
        changed = true;
    }
    Ok(changed)
}

/// Disconnect: removes trailing CodePet groups; counts the ones a foreign group follows.
fn remove_groups(tree: &mut J) -> Result<(bool, usize), Failed> {
    let Some(J::Obj(events)) = tree.get_mut("hooks") else {
        return Ok((false, 0));
    };
    let mut changed = false;
    let mut left = 0;
    let mut emptied = Vec::new();
    for (k, arr) in events.iter_mut() {
        let J::Arr(arr) = arr else {
            continue;
        };
        let had = arr.len();
        let had_ours = arr.iter().any(is_codepet_group);
        while arr.last().is_some_and(is_codepet_group) {
            arr.pop();
        }
        changed |= arr.len() != had;
        left += arr.iter().filter(|g| is_codepet_group(g)).count();
        if arr.is_empty() && had_ours {
            emptied.push(k.clone());
        }
    }
    events.retain(|(k, _)| !emptied.contains(k));
    Ok((changed, left))
}

/// Read, edit, compare-and-swap write, verify, foreign-trust check. `between` runs after
/// each read and before the swap (tests use it to race a concurrent writer).
fn edit(
    ctx: &Ctx,
    mut change: impl FnMut(&mut J) -> Result<bool, Failed>,
    mut between: impl FnMut(u32),
) -> Result<bool, Failed> {
    let path = hooks_path(ctx);
    let cfg = read_config(ctx);
    let prefixes = key_prefixes(&path);
    for attempt in 0..CAS_TRIES {
        let read = read_tree(&path).map_err(|e| Failed::new(Reason::Blocked, e.to_string()))?;
        let (old_bytes, mut tree) = match read {
            Some((b, t)) => (Some(b), t),
            None => (None, J::Obj(vec![("hooks".into(), J::Obj(Vec::new()))])),
        };
        let before_tree = tree.clone();
        if !change(&mut tree)? {
            return Ok(false);
        }
        let text = jsonish::to_pretty(&tree) + "\n";
        between(attempt);
        // Compare and swap: the file must still hold what was read.
        let now = match fs::read(&path) {
            Ok(b) => Some(b),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        if now != old_bytes {
            continue;
        }
        let mode = fs::metadata(&path)
            .map(|m| m.permissions().mode() & 0o7777)
            .unwrap_or(0o644);
        if let Some(old) = &old_bytes {
            let orig = path.with_file_name("hooks.json.codepet-orig");
            if fs::symlink_metadata(&orig).is_err() {
                super::write_file(&orig, ".hooks.json.codepet-orig.tmp", old, 0o600)?;
            }
            super::write_file(
                &path.with_file_name("hooks.json.codepet-last"),
                ".hooks.json.codepet-last.tmp",
                old,
                0o600,
            )?;
        }
        super::ensure_dir(&ctx.env.codex_dir, 0o700)?;
        let tmp = path.with_file_name(format!(".hooks.json.codepet-{}", std::process::id()));
        let _ = fs::remove_file(&tmp);
        {
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&tmp)?;
            f.write_all(text.as_bytes())?;
            f.set_permissions(fs::Permissions::from_mode(mode))?;
            f.sync_all()?;
        }
        if let Err(e) = fs::rename(&tmp, &path) {
            let _ = fs::remove_file(&tmp);
            return Err(e.into());
        }
        // Re-read and verify, then the foreign-trust check.
        let after = read_tree(&path).ok().flatten().map(|(_, t)| t);
        let foreign_ok =
            foreign_positions(&before_tree, &cfg, &prefixes)
                .iter()
                .all(|(ev, g, h, hash)| {
                    after
                        .as_ref()
                        .and_then(|t| hash_at(t, ev, *g, *h))
                        .is_some_and(|now| now == *hash)
                });
        if after.as_ref() != Some(&tree) || !foreign_ok {
            match &old_bytes {
                Some(old) => {
                    let _ = super::write_file(&path, ".hooks.json.codepet-restore", old, mode);
                }
                None => {
                    let _ = fs::remove_file(&path);
                }
            }
            return Err(Failed::new(
                Reason::Blocked,
                "a foreign Codex hook would have moved; restored",
            ));
        }
        return Ok(true);
    }
    Err(Failed::new(
        Reason::Blocked,
        "hooks.json kept changing while CodePet wrote it",
    ))
}

fn connect_with(ctx: &Ctx, between: impl FnMut(u32)) -> Result<Connected, Failed> {
    if read_config(ctx).hooks_off {
        return Ok(Connected {
            connected: false,
            needs_step: Some(STEP_CODEX_HOOKS_OFF),
        });
    }
    let wanted = groups(ctx)?;
    edit(ctx, |t| add_groups(t, &wanted), between)?;
    let st = status(ctx);
    if st.artifact != Artifact::Current {
        return Err(
            Failed::new(Reason::Blocked, "CodePet's Codex lines are not in place").installed(),
        );
    }
    let needs_step = match st.needs_step {
        Some(Some(step)) => Some(step),
        Some(None) => None,
        None => Some(STEP_CODEX_TRUST),
    };
    Ok(Connected {
        connected: true,
        needs_step,
    })
}

pub fn connect(ctx: &Ctx) -> Result<Connected, Failed> {
    connect_with(ctx, |_| {})
}

fn disconnect_with(ctx: &Ctx, between: impl FnMut(u32)) -> Result<MergeReport, Failed> {
    let mut left = 0;
    let wrote = edit(
        ctx,
        |t| {
            let (changed, l) = remove_groups(t)?;
            left = l;
            Ok(changed)
        },
        between,
    )?;
    Ok(MergeReport {
        wrote,
        tombstoned: left,
    })
}

pub fn disconnect(ctx: &Ctx) -> Result<MergeReport, Failed> {
    disconnect_with(ctx, |_| {})
}

#[cfg(test)]
mod tests {
    use super::*;
    use codepet_storage::StatePaths;
    use std::ffi::OsString;

    const REF: &str = include_str!("../../../../docs/v0.3/contracts/commands.json");

    fn ctx_at(home: &Path) -> Ctx {
        let h = home.to_path_buf();
        let env = super::super::Env::resolve(move |k| match k {
            "HOME" => Some(h.clone().into_os_string()),
            "PATH" => Some(OsString::from("/usr/bin")),
            _ => None,
        })
        .unwrap();
        let h = home.to_path_buf();
        let paths = StatePaths::resolve(move |k| (k == "HOME").then(|| h.clone().into_os_string()))
            .unwrap();
        Ctx::new(env, paths, home.join(".local/bin/codepet")).unwrap()
    }

    #[test]
    fn the_six_frozen_trust_hashes() {
        let reference: serde_json::Value = serde_json::from_str(REF).unwrap();
        let ctx = ctx_at(Path::new("/home/u"));
        let ours = groups(&ctx).unwrap();
        let frozen = reference["clients"]["codex"]["groups"].as_array().unwrap();
        assert_eq!(ours.len(), 6);
        for ((event, group), want) in ours.iter().zip(frozen) {
            assert_eq!(event, want["event"].as_str().unwrap());
            assert_eq!(snake(event), want["snake_event"].as_str().unwrap());
            assert_eq!(group.to_value(), want["group"]);
            let h = &group.get("hooks").unwrap().as_arr().unwrap()[0];
            assert_eq!(
                handler_hash(&snake(event), group.get("matcher"), h),
                want["trusted_hash"].as_str().unwrap(),
                "{event}"
            );
        }
        // SessionEnd never carries async; PermissionRequest and PostToolUse do.
        let has_async = |e: &str| {
            ours.iter()
                .find(|(ev, _)| ev == e)
                .unwrap()
                .1
                .get("hooks")
                .unwrap()
                .as_arr()
                .unwrap()[0]
                .get("async")
                .is_some()
        };
        assert!(
            !has_async("SessionEnd") && has_async("PermissionRequest") && has_async("PostToolUse")
        );
    }

    #[test]
    fn hash_formula_matches_a_known_foreign_entry() {
        // herdr's SessionStart entry, verified by the Codex research (§2 Trust).
        let h = jsonish::parse(r#"{"type":"command","command":"bash '/home/gfarch/.codex/herdr-agent-state.sh' session","timeout":10}"#).unwrap();
        let hash = handler_hash("session_start", None, &h);
        assert!(
            hash.starts_with("sha256:7dd71528") && hash.ends_with("dfd73a"),
            "{hash}"
        );
    }

    fn write_hooks(ctx: &Ctx, text: &str) {
        fs::create_dir_all(&ctx.env.codex_dir).unwrap();
        fs::write(hooks_path(ctx), text).unwrap();
    }

    const FOREIGN: &str = r#"{
  "hooks": {
    "SessionStart": [{"matcher": "startup", "hooks": [{"type": "command", "command": "herdr session", "timeout": 10}]}],
    "PreToolUse": [{"matcher": "^Bash$", "hooks": [{"type": "command", "command": "slopbrake-hook pre-tool-use"}]}],
    "PostToolUse": [{"hooks": [{"type": "command", "command": "slopbrake-hook post-tool-use"}]}],
    "Stop": [{"hooks": [{"type": "command", "command": "slopbrake-hook stop", "timeout": 600}]}]
  },
  "zz_unknown": {"keep": [1, 2.50, "x"]}
}"#;

    #[test]
    fn connect_appends_at_the_end_idempotently_keeping_order_mode_and_backups() {
        let t = tempfile::tempdir().unwrap();
        let ctx = ctx_at(t.path());
        write_hooks(&ctx, FOREIGN);
        fs::set_permissions(hooks_path(&ctx), fs::Permissions::from_mode(0o640)).unwrap();
        let c = connect(&ctx).unwrap();
        assert_eq!(
            c,
            Connected {
                connected: true,
                needs_step: Some(STEP_CODEX_TRUST)
            }
        );
        let text = fs::read_to_string(hooks_path(&ctx)).unwrap();
        let tree = jsonish::parse(&text).unwrap();
        // Foreign groups keep index 0; ours follow (PostToolUse 1, Stop 1, others 0).
        for (ev, idx) in [
            ("PostToolUse", 1),
            ("Stop", 1),
            ("UserPromptSubmit", 0),
            ("PermissionRequest", 0),
            ("Interrupt", 0),
            ("SessionEnd", 0),
        ] {
            let arr = event_array(&tree, ev).unwrap();
            assert!(is_codepet_group(&arr[idx]), "{ev}");
            assert_eq!(arr.len(), idx + 1, "{ev}");
        }
        assert!(!is_codepet_group(&event_array(&tree, "Stop").unwrap()[0]));
        // Key order kept: hooks first, zz_unknown second; foreign lexemes kept.
        let J::Obj(top) = &tree else { panic!() };
        assert_eq!(
            top.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
            ["hooks", "zz_unknown"]
        );
        assert!(text.contains("2.50"));
        assert_eq!(
            fs::metadata(hooks_path(&ctx)).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert_eq!(
            fs::read_to_string(ctx.env.codex_dir.join("hooks.json.codepet-orig")).unwrap(),
            FOREIGN
        );
        // Second run: byte-identical, no new backup content.
        let again = connect(&ctx).unwrap();
        assert_eq!(again.needs_step, Some(STEP_CODEX_TRUST));
        assert_eq!(fs::read_to_string(hooks_path(&ctx)).unwrap(), text);
        assert_eq!(
            fs::read_to_string(ctx.env.codex_dir.join("hooks.json.codepet-orig")).unwrap(),
            FOREIGN
        );
        // No temp files left beside it.
        let names: Vec<String> = fs::read_dir(&ctx.env.codex_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert!(names.iter().all(|n| !n.starts_with('.')), "{names:?}");
    }

    #[test]
    fn trust_status_reads_the_config_and_clears_the_step() {
        let t = tempfile::tempdir().unwrap();
        let ctx = ctx_at(t.path());
        write_hooks(&ctx, FOREIGN);
        connect(&ctx).unwrap();
        let tree = read_tree(&hooks_path(&ctx)).unwrap().unwrap().1;
        let p = hooks_path(&ctx).display().to_string();
        let mut toml = String::from("model = \"x\"\n[features]\nhooks = true\n");
        let mut n = 0;
        for (event, group) in groups(&ctx).unwrap() {
            let arr = event_array(&tree, &event).unwrap();
            let i = arr.iter().position(|g| *g == group).unwrap();
            let sn = snake(&event);
            toml += &format!(
                "[hooks.state.\"{p}:{sn}:{i}:0\"]\ntrusted_hash = \"{}\"\n",
                hash_at(&tree, &sn, i, 0).unwrap()
            );
            n += 1;
            if n == 5 {
                break;
            }
        }
        fs::write(config_path(&ctx), &toml).unwrap();
        let st = status(&ctx);
        assert_eq!((st.trusted, st.lines, st.needs_step), (5, 6, None));
        // The sixth line trusted: the step clears.
        let (event, group) = groups(&ctx).unwrap().pop().unwrap();
        let i = event_array(&tree, &event)
            .unwrap()
            .iter()
            .position(|g| *g == group)
            .unwrap();
        let sn = snake(&event);
        fs::write(
            config_path(&ctx),
            format!(
                "{toml}[hooks.state.\"{p}:{sn}:{i}:0\"]\ntrusted_hash = \"{}\"\n",
                hash_at(&tree, &sn, i, 0).unwrap()
            ),
        )
        .unwrap();
        let st = status(&ctx);
        assert_eq!((st.trusted, st.needs_step), (6, Some(None)));
        assert_eq!(connect(&ctx).unwrap().needs_step, None);
        // A line turned off in /hooks.
        fs::write(
            config_path(&ctx),
            format!("[hooks.state.\"{p}:{sn}:{i}:0\"]\nenabled = false\n"),
        )
        .unwrap();
        assert_eq!(status(&ctx).needs_step, Some(Some(STEP_CODEX_HOOKS_OFF)));
    }

    #[test]
    fn hooks_off_writes_nothing() {
        let t = tempfile::tempdir().unwrap();
        let ctx = ctx_at(t.path());
        write_hooks(&ctx, FOREIGN);
        fs::write(config_path(&ctx), "[features]\ncodex_hooks = false\n").unwrap();
        let c = connect(&ctx).unwrap();
        assert_eq!(
            c,
            Connected {
                connected: false,
                needs_step: Some(STEP_CODEX_HOOKS_OFF)
            }
        );
        assert_eq!(fs::read_to_string(hooks_path(&ctx)).unwrap(), FOREIGN);
        assert!(!ctx.env.codex_dir.join("hooks.json.codepet-orig").exists());
        assert_eq!(status(&ctx).needs_step, Some(Some(STEP_CODEX_HOOKS_OFF)));
    }

    #[test]
    fn compare_and_swap_rebuilds_from_a_concurrent_write() {
        let t = tempfile::tempdir().unwrap();
        let ctx = ctx_at(t.path());
        write_hooks(&ctx, FOREIGN);
        let path = hooks_path(&ctx);
        let raced = FOREIGN.replace(
            "\"Stop\": [",
            "\"Stop\": [{\"hooks\": [{\"type\": \"command\", \"command\": \"herdr stop\"}]}, ",
        );
        let c = connect_with(&ctx, |attempt| {
            if attempt == 0 {
                fs::write(&path, &raced).unwrap();
            }
        })
        .unwrap();
        assert!(c.connected);
        let tree = read_tree(&path).unwrap().unwrap().1;
        let stop = event_array(&tree, "Stop").unwrap();
        assert_eq!(
            stop.len(),
            3,
            "the racing group is kept and ours appended after it"
        );
        assert!(is_codepet_group(&stop[2]));
        assert_eq!(
            fs::read_to_string(ctx.env.codex_dir.join("hooks.json.codepet-last")).unwrap(),
            raced
        );
        // A writer that never stops: blocked after three tries, nothing written.
        let t2 = tempfile::tempdir().unwrap();
        let ctx2 = ctx_at(t2.path());
        write_hooks(&ctx2, FOREIGN);
        let p2 = hooks_path(&ctx2);
        let err = connect_with(&ctx2, |n| {
            fs::write(&p2, format!("{FOREIGN}\n{}", " ".repeat(n as usize + 1))).unwrap()
        })
        .unwrap_err();
        assert_eq!(err.reason, Reason::Blocked);
        assert!(!fs::read_to_string(&p2).unwrap().contains("codepet-hook"));
    }

    #[test]
    fn disconnect_tombstones_groups_a_foreign_group_follows() {
        let t = tempfile::tempdir().unwrap();
        let ctx = ctx_at(t.path());
        write_hooks(&ctx, FOREIGN);
        connect(&ctx).unwrap();
        // Something appends a foreign group after ours in Stop (index 2).
        let path = hooks_path(&ctx);
        let mut tree = read_tree(&path).unwrap().unwrap().1;
        if let Some(J::Obj(events)) = tree.get_mut("hooks") {
            let (_, J::Arr(stop)) = events.iter_mut().find(|(k, _)| k == "Stop").unwrap() else {
                panic!()
            };
            stop.push(
                jsonish::parse(r#"{"hooks":[{"type":"command","command":"later-hook"}]}"#).unwrap(),
            );
        }
        fs::write(&path, jsonish::to_pretty(&tree) + "\n").unwrap();
        let p = path.display().to_string();
        let later = hash_at(&tree, "stop", 2, 0).unwrap();
        fs::write(config_path(&ctx), format!("[hooks.state.\"{p}:stop:2:0\"]\ntrusted_hash = \"{later}\"\n[hooks.state.\"{p}:stop:0:0\"]\ntrusted_hash = \"{}\"\n", hash_at(&tree, "stop", 0, 0).unwrap())).unwrap();
        let r = disconnect(&ctx).unwrap();
        assert_eq!(
            r,
            MergeReport {
                wrote: true,
                tombstoned: 1
            }
        );
        let after = read_tree(&path).unwrap().unwrap().1;
        // The foreign trusted positions are unchanged.
        assert_eq!(hash_at(&after, "stop", 2, 0).unwrap(), later);
        assert!(
            is_codepet_group(&event_array(&after, "Stop").unwrap()[1]),
            "tombstone stays"
        );
        assert!(
            event_array(&after, "UserPromptSubmit").is_none(),
            "emptied arrays we made go"
        );
        assert_eq!(event_array(&after, "PostToolUse").unwrap().len(), 1);
        assert_eq!(status(&ctx).followed, 1);
        // The foreign group goes away later: the next pass removes the tombstone.
        let mut tree = after.clone();
        if let Some(J::Obj(events)) = tree.get_mut("hooks") {
            let (_, J::Arr(stop)) = events.iter_mut().find(|(k, _)| k == "Stop").unwrap() else {
                panic!()
            };
            stop.pop();
        }
        fs::write(&path, jsonish::to_pretty(&tree) + "\n").unwrap();
        fs::remove_file(config_path(&ctx)).unwrap();
        assert_eq!(
            disconnect(&ctx).unwrap(),
            MergeReport {
                wrote: true,
                tombstoned: 0
            }
        );
        let fin = read_tree(&path).unwrap().unwrap().1;
        assert_eq!(
            fin.to_value(),
            jsonish::parse(FOREIGN).unwrap().to_value(),
            "back to the foreign file"
        );
        assert_eq!(
            disconnect(&ctx).unwrap(),
            MergeReport {
                wrote: false,
                tombstoned: 0
            }
        );
    }

    #[test]
    fn foreign_trust_check_restores_on_a_moved_position() {
        let t = tempfile::tempdir().unwrap();
        let ctx = ctx_at(t.path());
        write_hooks(&ctx, FOREIGN);
        let path = hooks_path(&ctx);
        let tree = read_tree(&path).unwrap().unwrap().1;
        let p = path.display().to_string();
        fs::write(
            config_path(&ctx),
            format!(
                "[hooks.state.\"{p}:stop:0:0\"]\ntrusted_hash = \"{}\"\n",
                hash_at(&tree, "stop", 0, 0).unwrap()
            ),
        )
        .unwrap();
        // A broken edit that inserts before the foreign group must be refused and undone.
        let err = edit(
            &ctx,
            |t| {
                let Some(J::Obj(events)) = t.get_mut("hooks") else {
                    unreachable!()
                };
                let (_, J::Arr(stop)) = events.iter_mut().find(|(k, _)| k == "Stop").unwrap()
                else {
                    unreachable!()
                };
                stop.insert(
                    0,
                    jsonish::parse(
                        r#"{"hooks":[{"type":"command","command":"/x/codepet-hook codex"}]}"#,
                    )
                    .unwrap(),
                );
                Ok(true)
            },
            |_| {},
        )
        .unwrap_err();
        assert_eq!(err.reason, Reason::Blocked);
        assert_eq!(fs::read_to_string(&path).unwrap(), FOREIGN);
    }

    #[test]
    fn an_older_codepet_group_is_replaced_in_place() {
        let t = tempfile::tempdir().unwrap();
        let ctx = ctx_at(t.path());
        let old = FOREIGN.replace(
            "\"Stop\": [",
            &format!("\"Stop\": [{{\"hooks\": [{{\"type\": \"command\", \"command\": \"{}/.local/share/codepet/bin/codepet-hook codex\", \"timeout\": 5}}]}}, ", t.path().display()),
        );
        write_hooks(&ctx, &old);
        connect(&ctx).unwrap();
        let tree = read_tree(&hooks_path(&ctx)).unwrap().unwrap().1;
        let stop = event_array(&tree, "Stop").unwrap();
        assert_eq!(stop.len(), 2);
        assert!(is_codepet_group(&stop[0]) && !is_codepet_group(&stop[1]));
        let want = groups(&ctx)
            .unwrap()
            .into_iter()
            .find(|(e, _)| e == "Stop")
            .unwrap()
            .1;
        assert_eq!(stop[0], want);
    }

    #[test]
    fn an_unparsable_file_is_never_touched() {
        let t = tempfile::tempdir().unwrap();
        let ctx = ctx_at(t.path());
        write_hooks(&ctx, "{ not json");
        assert_eq!(connect(&ctx).unwrap_err().reason, Reason::Blocked);
        assert_eq!(fs::read_to_string(hooks_path(&ctx)).unwrap(), "{ not json");
    }
}
