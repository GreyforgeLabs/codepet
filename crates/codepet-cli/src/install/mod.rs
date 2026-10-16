//! Client integrations (spec §5.1-§5.7): where each supported coding client keeps its
//! config, what CodePet installs there, how it is detected (file-only), connected,
//! disconnected and refreshed.
//!
//! Rules shared by every client:
//! - each client gets exactly one CodePet-owned artifact, marked `managed by codepet`
//!   (Codex: CodePet's groups inside the shared `hooks.json`);
//! - CodePet never writes `~/.claude/settings.json` or `~/.hermes/config.yaml`; the
//!   clients' own CLIs do, checked before and after (settings key check);
//! - client CLIs run only inside a clients job ([`runner`]);
//! - files in scanned dirs are written through temporary names that never end in a loaded
//!   extension, plugin dirs are built in `<data_dir>/staging` and renamed in;
//! - every stored command uses the literal shim path (`commands.json`).
pub mod claude;
pub mod codex;
pub mod detect;
pub mod grok;
pub mod hermes;
pub mod jsonish;
pub mod opencode;
pub mod runner;
pub mod tomlish;
pub mod yamlish;

use crate::hook::shim;
use codepet_core::agent::ClientId;
use codepet_storage::StatePaths;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// The marker every CodePet-owned client file carries.
pub const MARKER: &str = "managed by codepet";
/// The placeholder for the literal shim path in the shipped templates (`integrations/`).
pub const SHIM_PLACEHOLDER: &str = "@CODEPET_SHIM@";

/// One file of a CodePet-owned tree: (relative path, bytes, mode).
pub type TreeFile = (&'static str, Vec<u8>, u32);

/// Why a Connect or Disconnect failed (`protocol.json` `job_results.reason`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    NotInstalled,
    TooOld,
    Blocked,
    SettingsChanged,
}
impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotInstalled => "not_installed",
            Self::TooOld => "too_old",
            Self::Blocked => "blocked",
            Self::SettingsChanged => "settings_changed",
        }
    }
}

/// A failed client step: the reason, a short plain detail for the journal and doctor (never
/// file content), and whether CodePet's artifact is in place afterwards.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failed {
    pub reason: Reason,
    pub detail: String,
    pub artifact_installed: bool,
}
impl Failed {
    pub fn new(reason: Reason, detail: impl Into<String>) -> Self {
        Self {
            reason,
            detail: detail.into(),
            artifact_installed: false,
        }
    }
    pub fn installed(mut self) -> Self {
        self.artifact_installed = true;
        self
    }
}
impl From<io::Error> for Failed {
    fn from(e: io::Error) -> Self {
        Failed::new(Reason::Blocked, format!("file error: {}", e.kind()))
    }
}

/// A successful Connect: `needs_step` for Codex until its trust is seen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Connected {
    /// CodePet's artifact is in place (false only for Codex with hooks turned off).
    pub connected: bool,
    pub needs_step: Option<&'static str>,
}

/// What the artifact files say (file-only status).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Artifact {
    /// Nothing of CodePet's is installed.
    Missing,
    /// Installed with the current bytes.
    Current,
    /// CodePet's artifact is there with other bytes (an older release, or a moved shim).
    Stale,
}

/// The process environment the integrations resolve their paths from. Tests build one
/// from a throwaway HOME; the clients job builds it from its own environment.
#[derive(Clone, Debug)]
pub struct Env {
    pub home: PathBuf,
    /// `PATH` as given (passed to client CLIs) and its absolute entries.
    pub path_var: OsString,
    pub path_dirs: Vec<PathBuf>,
    /// `$CLAUDE_CONFIG_DIR` or `~/.claude`.
    pub claude_dir: PathBuf,
    /// `$CODEX_HOME` or `~/.codex`.
    pub codex_dir: PathBuf,
    /// `$XDG_CONFIG_HOME/opencode`.
    pub opencode_dir: PathBuf,
    /// `$HERMES_HOME` or `~/.hermes`.
    pub hermes_dir: PathBuf,
    /// `$GROK_HOME` or `~/.grok`.
    pub grok_dir: PathBuf,
    /// `$MISE_DATA_DIR` or `$XDG_DATA_HOME/mise`.
    pub mise_data: PathBuf,
    /// Config-dir overrides set in the environment, passed on to client CLIs.
    pub overrides: Vec<(String, OsString)>,
}

fn abs(v: Option<OsString>) -> Option<PathBuf> {
    v.filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

impl Env {
    /// Resolves from an environment lookup; `None` without an absolute `HOME`.
    pub fn resolve(get: impl Fn(&str) -> Option<OsString>) -> Option<Self> {
        let home = abs(get("HOME"))?;
        let path_var =
            get("PATH").unwrap_or_else(|| OsString::from("/usr/local/bin:/usr/bin:/bin"));
        let path_dirs = std::env::split_paths(&path_var)
            .filter(|p| p.is_absolute())
            .collect();
        let xdg_config_home = abs(get("XDG_CONFIG_HOME")).unwrap_or_else(|| home.join(".config"));
        let xdg_data_home = abs(get("XDG_DATA_HOME")).unwrap_or_else(|| home.join(".local/share"));
        let mut overrides = Vec::new();
        let mut over = |name: &str, default: PathBuf| match abs(get(name)) {
            Some(p) => {
                overrides.push((name.to_owned(), p.clone().into_os_string()));
                p
            }
            None => default,
        };
        let claude_dir = over("CLAUDE_CONFIG_DIR", home.join(".claude"));
        let codex_dir = over("CODEX_HOME", home.join(".codex"));
        let hermes_dir = over("HERMES_HOME", home.join(".hermes"));
        let grok_dir = over("GROK_HOME", home.join(".grok"));
        let mise_data = over("MISE_DATA_DIR", xdg_data_home.join("mise"));
        // OpenCode's config dir and mise's installs (the Claude and OpenCode wrappers).
        for name in ["XDG_CONFIG_HOME", "XDG_DATA_HOME"] {
            if let Some(p) = abs(get(name)) {
                overrides.push((name.to_owned(), p.into_os_string()));
            }
        }
        Some(Self {
            opencode_dir: xdg_config_home.join("opencode"),
            home,
            path_var,
            path_dirs,
            claude_dir,
            codex_dir,
            hermes_dir,
            grok_dir,
            mise_data,
            overrides,
        })
    }
    pub fn from_process() -> Option<Self> {
        Self::resolve(|k| std::env::var_os(k))
    }
    /// The client's config dir (detection requires it to exist).
    pub fn config_dir(&self, client: ClientId) -> &Path {
        match client {
            ClientId::ClaudeCode => &self.claude_dir,
            ClientId::Codex => &self.codex_dir,
            ClientId::OpenCode => &self.opencode_dir,
            ClientId::Hermes => &self.hermes_dir,
            ClientId::GrokCli => &self.grok_dir,
        }
    }
}

/// Everything a Connect, Disconnect or refresh needs.
#[derive(Clone, Debug)]
pub struct Ctx {
    pub env: Env,
    pub paths: StatePaths,
    /// The literal shim path every artifact runs.
    pub shim: PathBuf,
    /// The `codepet` binary the shim calls (`B`).
    pub bin: PathBuf,
    /// `<data_dir>/integrations` and `<data_dir>/staging`.
    pub integrations: PathBuf,
    pub staging: PathBuf,
}

impl Ctx {
    /// `None` when the data dir is unknown (no `HOME` and no `XDG_DATA_HOME`).
    pub fn new(env: Env, paths: StatePaths, bin: PathBuf) -> Option<Self> {
        Some(Self {
            shim: paths.shim_path()?,
            integrations: paths.integrations_dir()?,
            staging: paths.staging_dir()?,
            env,
            paths,
            bin,
        })
    }
    /// The shim path as it appears in stored commands; `Blocked` when it would need
    /// quoting in a shell or escaping in JSON (`commands.json` `stored_command.literal`).
    pub fn shim_literal(&self) -> Result<&str, Failed> {
        shell_safe(&self.shim)
            .ok_or_else(|| Failed::new(Reason::Blocked, "the shim path needs quoting"))
    }
    /// A shipped template with the literal shim path filled in.
    pub fn render(&self, template: &str) -> Result<String, Failed> {
        Ok(template.replace(SHIM_PLACEHOLDER, self.shim_literal()?))
    }
    /// `<shim> <id> >/dev/null 2>&1 || true` (shell clients).
    pub fn shell_command(&self, client: ClientId) -> Result<String, Failed> {
        Ok(format!(
            "{} {} >/dev/null 2>&1 || true",
            self.shim_literal()?,
            client.as_str()
        ))
    }
}

/// A path usable verbatim in a shell command and a JSON string.
pub fn shell_safe(p: &Path) -> Option<&str> {
    let s = p.to_str()?;
    let ok = p.is_absolute()
        && s.len() <= shim::MAX_PATH_BYTES
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._/+:@,=-".contains(&b));
    ok.then_some(s)
}

/// Creates a directory (and parents) with `mode` where missing; never follows a symlink
/// at the leaf.
pub fn ensure_dir(dir: &Path, mode: u32) -> io::Result<()> {
    match fs::symlink_metadata(dir) {
        Ok(m) if m.is_dir() => Ok(()),
        Ok(_) => Err(io::Error::other("not a directory")),
        Err(e) if e.kind() == ErrorKind::NotFound => {
            fs::DirBuilder::new().recursive(true).mode(mode).create(dir)
        }
        Err(e) => Err(e),
    }
}
/// Makes `dir` a private (0700) directory of this user, creating it if missing; an
/// existing one with another owner, group/other bits or a symlink in its place is refused.
pub fn ensure_private_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    match fs::symlink_metadata(dir) {
        Err(e) if e.kind() == ErrorKind::NotFound => {
            fs::DirBuilder::new().mode(0o700).create(dir)?;
        }
        Err(e) => return Err(e),
        Ok(_) => {}
    }
    let m = fs::symlink_metadata(dir)?;
    if !m.is_dir() || m.uid() != rustix::process::geteuid().as_raw() || m.mode() & 0o077 != 0 {
        return Err(io::Error::other("not a private directory of this user"));
    }
    Ok(())
}

/// Writes `bytes` to `path` through `<dir>/<tmp_name>` (exclusive create, fsync, rename),
/// with `mode`. Rewrites nothing when the bytes are already equal. Returns whether it wrote.
pub fn write_file(path: &Path, tmp_name: &str, bytes: &[u8], mode: u32) -> io::Result<bool> {
    if let Ok(m) = fs::symlink_metadata(path) {
        if m.is_file() && fs::read(path).is_ok_and(|old| old == bytes) {
            if m.permissions().mode() & 0o777 != mode {
                fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
            }
            return Ok(false);
        }
        if m.is_dir() {
            return Err(io::Error::other("a directory is in the way"));
        }
    }
    let dir = path.parent().ok_or_else(|| io::Error::other("no parent"))?;
    let tmp = dir.join(tmp_name);
    let _ = fs::remove_file(&tmp);
    let result = (|| {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.set_permissions(fs::Permissions::from_mode(mode))?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map(|()| true)
}

/// Removes a file if present (a missing file is fine).
pub fn remove_file(path: &Path) -> io::Result<bool> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

/// Removes CodePet's stale temporary names (`.codepet*.partial*`) from a scanned dir.
pub fn sweep_temps(dir: &Path) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let name = e.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with(".codepet") && name.contains(".partial") {
            let p = e.path();
            if fs::symlink_metadata(&p).is_ok_and(|m| m.is_dir()) {
                let _ = fs::remove_dir_all(&p);
            } else {
                let _ = fs::remove_file(&p);
            }
        }
    }
}

/// Installs a directory tree built in `staging` at `target` (same filesystem): a new
/// target by one rename, an existing one by an atomic exchange (the old tree is then
/// removed). `files` are (relative path, bytes, mode). Returns whether anything changed.
pub fn install_dir(
    staging: &Path,
    target: &Path,
    files: &[TreeFile],
) -> io::Result<bool> {
    if dir_matches(target, files) {
        return Ok(false);
    }
    ensure_dir(staging, 0o700)?;
    let name = target
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| io::Error::other("no dir name"))?;
    let stage = staging.join(format!(".{name}.partial-{}", std::process::id()));
    let _ = fs::remove_dir_all(&stage);
    fs::DirBuilder::new().mode(0o755).create(&stage)?;
    let built = (|| {
        for (rel, bytes, mode) in files {
            let p = stage.join(rel);
            if let Some(parent) = p.parent() {
                fs::DirBuilder::new()
                    .recursive(true)
                    .mode(0o755)
                    .create(parent)?;
            }
            let mut f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&p)?;
            f.write_all(bytes)?;
            f.set_permissions(fs::Permissions::from_mode(*mode))?;
            f.sync_all()?;
        }
        Ok::<(), io::Error>(())
    })();
    if let Err(e) = built {
        let _ = fs::remove_dir_all(&stage);
        return Err(e);
    }
    if let Some(parent) = target.parent() {
        ensure_dir(parent, 0o755)?;
    }
    let swapped = match fs::symlink_metadata(target) {
        Err(e) if e.kind() == ErrorKind::NotFound => fs::rename(&stage, target).map(|()| false),
        Ok(m) if m.is_dir() => rustix::fs::renameat_with(
            rustix::fs::CWD,
            &stage,
            rustix::fs::CWD,
            target,
            rustix::fs::RenameFlags::EXCHANGE,
        )
        .map(|()| true)
        .map_err(io::Error::from),
        Ok(_) => Err(io::Error::other("a file is in the way")),
        Err(e) => Err(e),
    };
    match swapped {
        Ok(true) => {
            // `stage` now holds the old tree.
            let _ = fs::remove_dir_all(&stage);
            Ok(true)
        }
        Ok(false) => Ok(true),
        Err(e) => {
            let _ = fs::remove_dir_all(&stage);
            Err(e)
        }
    }
}

/// The tree at `target` holds exactly `files` (no other regular files).
fn dir_matches(target: &Path, files: &[TreeFile]) -> bool {
    if !fs::symlink_metadata(target).is_ok_and(|m| m.is_dir()) {
        return false;
    }
    for (rel, bytes, mode) in files {
        let p = target.join(rel);
        match fs::symlink_metadata(&p) {
            Ok(m) if m.is_file() && m.permissions().mode() & 0o777 == *mode => {}
            _ => return false,
        }
        if fs::read(&p).map(|b| b != *bytes).unwrap_or(true) {
            return false;
        }
    }
    count_files(target) == files.len()
}

fn count_files(dir: &Path) -> usize {
    let Ok(rd) = fs::read_dir(dir) else {
        return 0;
    };
    rd.flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => count_files(&e.path()),
            Ok(_) => 1,
            Err(_) => 0,
        })
        .sum()
}

/// Installs or refreshes the shim (temp + fsync + rename, 0755). Its bytes depend only on
/// the state dir and the binary path. Returns whether it wrote.
pub fn install_shim(ctx: &Ctx) -> Result<bool, Failed> {
    let body = shim::render(&ctx.paths.state_dir, &ctx.bin)
        .map_err(|_| Failed::new(Reason::Blocked, "a path cannot be written into the shim"))?;
    ctx.shim_literal()?;
    let dir = ctx
        .shim
        .parent()
        .ok_or_else(|| Failed::new(Reason::Blocked, "no shim dir"))?;
    ensure_dir(dir, 0o755)?;
    if fs::read_to_string(&ctx.shim).is_ok_and(|old| old == body)
        && fs::metadata(&ctx.shim).is_ok_and(|m| m.permissions().mode() & 0o777 == 0o755)
    {
        return Ok(false);
    }
    shim::write_atomically(&ctx.shim, &body)?;
    Ok(true)
}

/// What the installed shim holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShimState {
    Missing,
    Tombstone,
    /// A CodePet shim with these `S` and `B` paths.
    Shim {
        state_dir: String,
        bin: String,
        current: bool,
    },
    /// Something without CodePet's marker.
    Foreign,
}

/// Reads the installed shim (doctor, refresh).
pub fn shim_state(shim_path: &Path) -> ShimState {
    let Ok(text) = fs::read_to_string(shim_path) else {
        return if fs::symlink_metadata(shim_path).is_ok() {
            ShimState::Foreign
        } else {
            ShimState::Missing
        };
    };
    if text == shim::TOMBSTONE {
        return ShimState::Tombstone;
    }
    if !text.contains("# managed by codepet") {
        return ShimState::Foreign;
    }
    let grab = |prefix: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(prefix))
            .and_then(|r| r.strip_suffix('\''))
            .map(str::to_owned)
    };
    match (grab("S='"), grab("B='")) {
        (Some(s), Some(b)) => {
            let current = shim::render(Path::new(&s), Path::new(&b)).is_ok_and(|r| r == text);
            ShimState::Shim {
                state_dir: s,
                bin: b,
                current,
            }
        }
        _ => ShimState::Foreign,
    }
}

/// Every shim-path command form a client artifact may hold for `client` with this shim.
pub fn is_our_command(cmd: &str, client: ClientId) -> bool {
    let Some((head, rest)) = cmd.split_once(' ') else {
        return false;
    };
    let id = client.as_str();
    head.ends_with("/codepet-hook") && (rest == id || rest.starts_with(&format!("{id} ")))
}

/// One client module's operations.
pub fn status(ctx: &Ctx, client: ClientId) -> Artifact {
    match client {
        ClientId::ClaudeCode => claude::status(ctx),
        ClientId::Codex => codex::status(ctx).artifact,
        ClientId::OpenCode => opencode::status(ctx),
        ClientId::Hermes => hermes::status(ctx),
        ClientId::GrokCli => grok::status(ctx),
    }
}

/// Refreshes a connected client's wholly CodePet-owned files whose marker is present and
/// whose bytes differ (installer step 5). Never runs a client CLI, never touches a shared
/// file (Codex). Returns whether it wrote.
pub fn refresh(ctx: &Ctx, client: ClientId) -> io::Result<bool> {
    match client {
        ClientId::ClaudeCode => claude::refresh(ctx),
        ClientId::Codex => Ok(false),
        ClientId::OpenCode => opencode::refresh(ctx),
        ClientId::Hermes => hermes::refresh(ctx),
        ClientId::GrokCli => grok::refresh(ctx),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The context at `commands.json` `reference_paths`.
    pub(crate) fn reference_ctx() -> Ctx {
        let env = Env::resolve(|k| match k {
            "HOME" => Some(OsString::from("/home/u")),
            "PATH" => Some(OsString::from("/usr/bin")),
            _ => None,
        })
        .unwrap();
        let paths =
            StatePaths::resolve(|k| (k == "HOME").then(|| OsString::from("/home/u"))).unwrap();
        Ctx::new(env, paths, PathBuf::from("/home/u/.local/bin/codepet")).unwrap()
    }

    #[test]
    fn rendered_client_files_match_the_frozen_contract_bytes() {
        let reference: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../docs/v0.3/contracts/commands.json"
        ))
        .unwrap();
        let ctx = reference_ctx();
        assert_eq!(
            ctx.shim.to_str(),
            reference["reference_paths"]["shim"].as_str()
        );
        let sha = |t: &str| codepet_storage::sha256_hex(t.as_bytes());
        let claude = ctx.render(claude::HOOKS_JSON).unwrap();
        assert_eq!(
            sha(&claude),
            reference["clients"]["claude-code"]["file_sha256_at_reference_paths"]
        );
        let grok = ctx.render(grok::TEMPLATE).unwrap();
        assert_eq!(
            sha(&grok),
            reference["clients"]["grok-cli"]["file_sha256_at_reference_paths"]
        );
        for c in [ClientId::ClaudeCode, ClientId::Codex, ClientId::GrokCli] {
            assert_eq!(
                ctx.shell_command(c).unwrap(),
                reference["clients"][c.as_str()]["command"]
                    .as_str()
                    .unwrap()
            );
        }
        // The shim at the reference paths.
        let body = shim::render(&ctx.paths.state_dir, &ctx.bin).unwrap();
        assert_eq!(sha(&body), reference["shim"]["reference_sha256"]);
        assert_eq!(sha(shim::TOMBSTONE), reference["shim"]["tombstone_sha256"]);
        // The argv clients' plugins carry the literal shim path, nothing else changes.
        assert!(ctx
            .render(opencode::TEMPLATE)
            .unwrap()
            .contains("const SHIM = \"/home/u/.local/share/codepet/bin/codepet-hook\";"));
        assert!(ctx
            .render(hermes::INIT_PY)
            .unwrap()
            .contains("_SHIM = \"/home/u/.local/share/codepet/bin/codepet-hook\""));
    }

    #[test]
    fn shell_safe_paths_only() {
        assert_eq!(
            shell_safe(Path::new("/home/u/.local/share/codepet/bin/codepet-hook")),
            Some("/home/u/.local/share/codepet/bin/codepet-hook")
        );
        for bad in [
            "rel/x", "/a b", "/a'b", "/a\"b", "/a$b", "/a\\b", "/a;b", "/é",
        ] {
            assert_eq!(shell_safe(Path::new(bad)), None, "{bad}");
        }
    }

    #[test]
    fn recognises_our_commands_only() {
        let c = ClientId::Codex;
        assert!(is_our_command(
            "/h/.local/share/codepet/bin/codepet-hook codex >/dev/null 2>&1 || true",
            c
        ));
        assert!(is_our_command("/old/codepet-hook codex", c));
        assert!(!is_our_command("/h/codepet-hook codexx", c));
        assert!(!is_our_command("/h/codepet-hook claude-code", c));
        assert!(!is_our_command("slopbrake-hook stop", c));
        assert!(!is_our_command("/h/not-codepet-hook codex", c));
    }

    #[test]
    fn install_dir_swaps_atomically_and_is_idempotent() {
        let t = tempfile::tempdir().unwrap();
        let staging = t.path().join("staging");
        let target = t.path().join("scan/codepet");
        let files = vec![
            ("a.txt", b"one".to_vec(), 0o644),
            ("d/b.txt", b"two".to_vec(), 0o644),
        ];
        assert!(install_dir(&staging, &target, &files).unwrap());
        assert!(!install_dir(&staging, &target, &files).unwrap());
        let files2 = vec![("a.txt", b"three".to_vec(), 0o644)];
        assert!(install_dir(&staging, &target, &files2).unwrap());
        assert_eq!(fs::read(target.join("a.txt")).unwrap(), b"three");
        assert!(!target.join("d").exists());
        assert_eq!(fs::read_dir(&staging).unwrap().count(), 0, "no stage left");
    }

    #[test]
    fn write_file_rewrites_only_on_change() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("codepet.json");
        assert!(write_file(&p, ".codepet.json.partial", b"x", 0o644).unwrap());
        assert!(!write_file(&p, ".codepet.json.partial", b"x", 0o644).unwrap());
        assert!(write_file(&p, ".codepet.json.partial", b"y", 0o644).unwrap());
        assert_eq!(fs::read_dir(t.path()).unwrap().count(), 1);
        fs::write(t.path().join(".codepet.json.partial"), "stale").unwrap();
        sweep_temps(t.path());
        assert_eq!(fs::read_dir(t.path()).unwrap().count(), 1);
    }
}
