//! File-only client detection (spec §5.2 `clients.status`): the binary is found with
//! `stat` on `PATH` (plus each client's usual install dirs, because the bar's `PATH` may be
//! shorter than a login shell's), and its version is read from install paths or package
//! metadata. Nothing here executes a client or makes a network call.
//!
//! Version sources (documented for `doctor` and `docs/v0.3`):
//! - Claude Code and OpenCode installed through mise: the `mise` wrapper's tool, resolved
//!   through `<mise data>/installs/<tool>/latest` (else the highest installed version);
//! - Codex (`~/.codex/packages/standalone/releases/<v>-<target>/bin/codex`) and Grok
//!   (`~/.grok/downloads/grok-<v>-<target>`): the version in the binary's resolved path;
//! - Hermes: `version` in the `pyproject.toml` of the checkout its wrapper runs (never
//!   `hermes --version`, which makes a network update check);
//! - otherwise any `x.y.z` in the resolved path, else unknown (the Connect job may then
//!   ask `<binary> --version` under the client CLI rules, except Hermes).
use super::Env;
use codepet_core::agent::ClientId;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// The command name of each client.
pub fn command_name(client: ClientId) -> &'static str {
    match client {
        ClientId::ClaudeCode => "claude",
        ClientId::Codex => "codex",
        ClientId::OpenCode => "opencode",
        ClientId::Hermes => "hermes",
        ClientId::GrokCli => "grok",
    }
}

/// The oldest version each integration was tested against (SPEC §5.3-§5.7).
pub fn tested_version(client: ClientId) -> (u64, u64, u64) {
    match client {
        ClientId::ClaudeCode => (2, 1, 294),
        ClientId::Codex => (0, 162, 0),
        ClientId::OpenCode => (1, 19, 0),
        ClientId::Hermes => (0, 21, 3),
        ClientId::GrokCli => (1, 0, 52),
    }
}

/// What detection found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detected {
    pub binary: Option<PathBuf>,
    pub version: Option<String>,
    pub config_dir: PathBuf,
    pub config_exists: bool,
}
impl Detected {
    /// The binary and the config dir exist (`client_hooks.installed`).
    pub fn installed(&self) -> bool {
        self.binary.is_some() && self.config_exists
    }
}

/// `x.y.z` (a leading `v` and any suffix after the patch number are ignored).
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim().trim_start_matches('v');
    let mut parts = s.splitn(3, '.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch_text = parts.next()?;
    let digits: String = patch_text
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    let patch = digits.parse().ok()?;
    Some((major, minor, patch))
}

/// The last `x.y.z` found in `s` (each part 1-6 digits), as text.
pub fn find_version(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut best = None;
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() && (i == 0 || !b[i - 1].is_ascii_digit() && b[i - 1] != b'.') {
            let start = i;
            let mut dots = 0;
            let mut j = i;
            let mut run = 0;
            while j < b.len() && (b[j].is_ascii_digit() || (b[j] == b'.' && dots < 2)) {
                if b[j] == b'.' {
                    if run == 0 || j + 1 >= b.len() || !b[j + 1].is_ascii_digit() {
                        break;
                    }
                    dots += 1;
                    run = 0;
                } else {
                    run += 1;
                    if run > 6 {
                        break;
                    }
                }
                j += 1;
            }
            if dots == 2 && run > 0 {
                best = Some(s[start..j].to_owned());
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    best
}

fn executable(p: &Path) -> bool {
    fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// The usual install dirs of a client, searched after `PATH`.
fn fallback_dirs(env: &Env, client: ClientId) -> Vec<PathBuf> {
    let mut dirs = vec![
        env.home.join(".local/bin"),
        env.mise_data.join("shims"),
        env.home.join(".bun/bin"),
        env.home.join(".npm-global/bin"),
        env.home.join(".cargo/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
    ];
    match client {
        ClientId::GrokCli => dirs.insert(0, env.grok_dir.join("bin")),
        ClientId::ClaudeCode => dirs.insert(0, env.claude_dir.join("local")),
        ClientId::OpenCode => dirs.insert(0, env.home.join(".opencode/bin")),
        _ => {}
    }
    dirs
}

/// The client's binary on `PATH` (then its usual dirs), by `stat` only.
pub fn find_binary(env: &Env, client: ClientId) -> Option<PathBuf> {
    let name = command_name(client);
    env.path_dirs
        .iter()
        .cloned()
        .chain(fallback_dirs(env, client))
        .map(|d| d.join(name))
        .find(|p| executable(p))
}

/// A short text wrapper script's content (at most 8 KiB, starting with `#!`).
fn wrapper_text(p: &Path) -> Option<String> {
    let m = fs::metadata(p).ok()?;
    if m.len() > 8 * 1024 {
        return None;
    }
    let text = fs::read_to_string(p).ok()?;
    text.starts_with("#!").then_some(text)
}

/// The version mise would run for `tool`: `installs/<tool>/latest`, else the highest.
fn mise_version(env: &Env, tool: &str) -> Option<String> {
    let dir = env.mise_data.join("installs").join(tool);
    if let Ok(target) = fs::read_link(dir.join("latest")) {
        if let Some(v) = target.to_str().and_then(find_version) {
            return Some(v);
        }
    }
    let mut best: Option<((u64, u64, u64), String)> = None;
    for e in fs::read_dir(&dir).ok()?.flatten() {
        let name = e.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if let Some(v) = parse_version(name).filter(|_| name.split('.').count() == 3) {
            if best.as_ref().is_none_or(|(b, _)| v > *b) {
                best = Some((v, name.to_owned()));
            }
        }
    }
    best.map(|(_, s)| s)
}

/// `version = "x.y.z"` in a `pyproject.toml`.
fn pyproject_version(p: &Path) -> Option<String> {
    let m = fs::metadata(p).ok()?;
    if m.len() > 256 * 1024 {
        return None;
    }
    let text = fs::read_to_string(p).ok()?;
    let mut in_project = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_project = t == "[project]" || t == "[tool.poetry]";
            continue;
        }
        if in_project {
            if let Some(rest) = t.strip_prefix("version") {
                let v = rest
                    .trim_start()
                    .strip_prefix('=')?
                    .trim()
                    .trim_matches(['"', '\'']);
                return parse_version(v).map(|_| v.to_owned());
            }
        }
    }
    None
}

/// Absolute paths quoted or written in a wrapper script.
fn script_paths(text: &str) -> Vec<PathBuf> {
    text.split(|c: char| c.is_whitespace() || c == '"' || c == '\'')
        .filter(|t| t.starts_with('/') && t.len() < 4096)
        .map(PathBuf::from)
        .collect()
}

/// `$HOME/.grok/bin/grok`, `${HOME}/...` or `~/...` inside a wrapper, plus absolute paths.
fn script_targets(text: &str, home: &Path) -> Vec<PathBuf> {
    let mut out = script_paths(text);
    for raw in text.split(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '`') {
        let Some(rest) = raw
            .strip_prefix("$HOME/")
            .or_else(|| raw.strip_prefix("${HOME}/"))
            .or_else(|| raw.strip_prefix("~/"))
        else {
            continue;
        };
        if rest.is_empty() || rest.contains("..") || rest.starts_with('/') {
            continue;
        }
        let path = home.join(rest);
        if !out.iter().any(|p| p == &path) {
            out.push(path);
        }
    }
    out
}

/// The version of the binary at `bin` (file-only).
pub fn version_of(env: &Env, client: ClientId, bin: &Path) -> Option<String> {
    if client == ClientId::Hermes {
        if let Some(text) = wrapper_text(bin) {
            for p in script_targets(&text, &env.home) {
                for dir in p.ancestors().skip(1).take(4) {
                    if let Some(v) = pyproject_version(&dir.join("pyproject.toml")) {
                        return Some(v);
                    }
                }
            }
        }
        if let Some(v) = pyproject_version(&env.hermes_dir.join("hermes-agent/pyproject.toml")) {
            return Some(v);
        }
    }
    if let Some(text) = wrapper_text(bin) {
        let tool = command_name(client);
        if text.contains("mise") {
            if let Some(v) = mise_version(env, tool) {
                return Some(v);
            }
        }
        for p in script_targets(&text, &env.home) {
            if p.file_name().and_then(|n| n.to_str()) == Some(tool) {
                if let Some(v) = fs::canonicalize(&p)
                    .ok()
                    .and_then(|c| c.to_str().and_then(find_version))
                {
                    return Some(v);
                }
            }
        }
    }
    let real = fs::canonicalize(bin).ok()?;
    // Only the part below the home, so a version-like home path never matches.
    let rel = real.strip_prefix(&env.home).unwrap_or(&real);
    find_version(rel.to_str()?)
}

/// Detects one client (file-only).
pub fn detect(env: &Env, client: ClientId) -> Detected {
    let binary = find_binary(env, client);
    let version = binary.as_deref().and_then(|b| version_of(env, client, b));
    let config_dir = env.config_dir(client).to_path_buf();
    let config_exists = fs::metadata(&config_dir).is_ok_and(|m| m.is_dir());
    Detected {
        binary,
        version,
        config_dir,
        config_exists,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::os::unix::fs::symlink;

    fn env_for(home: &Path) -> Env {
        let h = home.to_path_buf();
        Env::resolve(move |k| match k {
            "HOME" => Some(h.clone().into_os_string()),
            "PATH" => Some(OsString::from(format!("{}/bin", h.display()))),
            _ => None,
        })
        .unwrap()
    }

    fn exe(p: &Path, body: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
        fs::set_permissions(p, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn versions_from_real_install_layouts() {
        let t = tempfile::tempdir().unwrap();
        let h = t.path();
        let env = env_for(h);
        // mise-wrapped claude: installs/claude/latest -> ./2.1.294
        exe(&h.join(".local/bin/claude"), "#!/bin/bash\nmise use -g --quiet \"claude\" || exit 1\nexec mise x \"claude\" -- \"claude\" \"$@\"\n");
        let inst = h.join(".local/share/mise/installs/claude");
        exe(&inst.join("2.1.289/claude"), "x");
        exe(&inst.join("2.1.294/claude"), "x");
        symlink("./2.1.294", inst.join("latest")).unwrap();
        // codex standalone symlink on PATH
        let codex = h.join(
            ".codex/packages/standalone/releases/0.163.0-x86_64-unknown-linux-musl/bin/codex",
        );
        exe(&codex, "elf");
        fs::create_dir_all(h.join("bin")).unwrap();
        symlink(&codex, h.join("bin/codex")).unwrap();
        // grok in ~/.grok/bin (not on PATH)
        exe(&h.join(".grok/downloads/grok-1.0.52-linux-x86_64"), "elf");
        fs::create_dir_all(h.join(".grok/bin")).unwrap();
        symlink(
            "../downloads/grok-1.0.52-linux-x86_64",
            h.join(".grok/bin/grok"),
        )
        .unwrap();
        // hermes wrapper -> checkout pyproject
        let agent = h.join(".hermes/hermes-agent");
        exe(&agent.join("venv/bin/python"), "x");
        fs::write(agent.join("pyproject.toml"), "[build-system]\nrequires=[]\n[project]\nname = \"hermes-agent\"\nversion = \"0.21.3\"\n").unwrap();
        exe(
            &h.join(".local/bin/hermes"),
            &format!(
                "#!/usr/bin/env bash\nexec \"{0}/venv/bin/python\" \"{0}/hermes\" \"$@\"\n",
                agent.display()
            ),
        );

        let c = detect(&env, ClientId::ClaudeCode);
        assert_eq!(c.version.as_deref(), Some("2.1.294"));
        assert!(c.binary.is_some() && !c.config_exists && !c.installed());
        let x = detect(&env, ClientId::Codex);
        assert_eq!(x.version.as_deref(), Some("0.163.0"));
        assert!(x.installed(), "~/.codex exists");
        let g = detect(&env, ClientId::GrokCli);
        assert_eq!(g.binary, Some(h.join(".grok/bin/grok")));
        assert_eq!(g.version.as_deref(), Some("1.0.52"));
        let m = detect(&env, ClientId::Hermes);
        assert_eq!(m.version.as_deref(), Some("0.21.3"));
        let o = detect(&env, ClientId::OpenCode);
        assert_eq!(o.binary, None);
        assert!(!o.installed());
    }

    #[test]
    fn version_helpers() {
        assert_eq!(
            find_version("/a/2.1/3.4.5-x86/1.2"),
            Some("3.4.5".to_owned())
        );
        assert_eq!(
            find_version("grok-1.0.52-linux-x86_64"),
            Some("1.0.52".to_owned())
        );
        assert_eq!(find_version("/usr/bin/x"), None);
        assert_eq!(parse_version("v0.163.0-alpha"), Some((0, 163, 0)));
        assert_eq!(parse_version("1.2"), None);
        assert!(parse_version("0.161.9").unwrap() < tested_version(ClientId::Codex));
    }

    #[test]
    fn grok_dispatch_wrapper_reports_the_real_version() {
        let t = tempfile::tempdir().unwrap();
        let h = t.path();
        exe(&h.join(".grok/downloads/grok-1.0.62-linux-x86_64"), "elf");
        fs::create_dir_all(h.join(".grok/bin")).unwrap();
        symlink(
            "../downloads/grok-1.0.62-linux-x86_64",
            h.join(".grok/bin/grok"),
        )
        .unwrap();
        exe(
            &h.join(".local/bin/grok"),
            "#!/bin/bash\nif [[ ${1-} == bot ]]; then\n  exec \"$HOME/.local/bin/grok-bot\" \"$@\"\nfi\nexec \"$HOME/.grok/bin/grok\" \"$@\"\n",
        );
        let env = Env::resolve(|k| match k {
            "HOME" => Some(h.to_path_buf().into_os_string()),
            "PATH" => Some(OsString::from(format!("{}/.local/bin", h.display()))),
            _ => None,
        })
        .unwrap();
        let g = detect(&env, ClientId::GrokCli);
        assert_eq!(g.binary, Some(h.join(".local/bin/grok")));
        assert_eq!(g.version.as_deref(), Some("1.0.62"));
        assert!(g.installed());
    }
}
