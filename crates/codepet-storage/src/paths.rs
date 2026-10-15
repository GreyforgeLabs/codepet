//! State directory resolution and fail-closed ownership/mode checks.
use crate::{sys, Result, StorageError, DB_FILE};
use std::ffi::OsString;
use std::fs::{self, DirBuilder, Metadata, OpenOptions};
use std::io::ErrorKind;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

pub const PRESENCE_FILE: &str = "presence.json";
pub const BAR_STATE_FILE: &str = "bar-state.json";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatePaths {
    /// `$XDG_STATE_HOME/codepet` or `$HOME/.local/state/codepet`.
    pub state_dir: PathBuf,
    pub db_path: PathBuf,
    /// Absolute `$HOME`, if set; preflight refuses it as a project folder.
    pub home: Option<PathBuf>,
    /// The account's home from `/etc/passwd` (set by `from_env`), also refused. Preflight
    /// fails closed when neither home is known.
    pub account_home: Option<PathBuf>,
}
impl StatePaths {
    /// Pure resolution from an environment lookup; touches no files. Tests pass a closure
    /// instead of mutating the process environment.
    pub fn resolve(env: impl Fn(&str) -> Option<OsString>) -> Result<Self> {
        let unsafe_dir = |why: &str| StorageError::UnsafeStateDir(why.to_owned());
        let home = env("HOME")
            .filter(|h| !h.is_empty())
            .map(PathBuf::from)
            .filter(|h| plain_absolute(h));
        let base = match env("XDG_STATE_HOME").filter(|v| !v.is_empty()) {
            Some(xdg) => {
                let xdg = PathBuf::from(xdg);
                if !plain_absolute(&xdg) {
                    return Err(unsafe_dir("XDG_STATE_HOME is not a plain absolute path"));
                }
                xdg
            }
            None => home
                .as_ref()
                .ok_or_else(|| unsafe_dir("neither XDG_STATE_HOME nor an absolute HOME is set"))?
                .join(".local/state"),
        };
        let state_dir = base.join("codepet");
        Ok(Self {
            db_path: state_dir.join(DB_FILE),
            state_dir,
            home,
            account_home: None,
        })
    }
    /// `resolve` over the process environment, plus the account home of the effective UID.
    pub fn from_env() -> Result<Self> {
        let mut paths = Self::resolve(|k| std::env::var_os(k))?;
        paths.account_home = sys::current_uid()
            .ok()
            .and_then(|uid| fs::read_to_string("/etc/passwd").ok().map(|p| (uid, p)))
            .and_then(|(uid, passwd)| sys::passwd_home(&passwd, uid))
            .filter(|h| plain_absolute(h));
        Ok(paths)
    }
    /// The bar leader's attended-presence sidecar, `<state_dir>/presence.json`. Storage
    /// only reads it (spec §5.2).
    pub fn presence_path(&self) -> PathBuf {
        self.state_dir.join(PRESENCE_FILE)
    }
    /// The UIs' passive state file, `<state_dir>/bar-state.json`, written only by
    /// `Store::export_bar_state` (spec §10).
    pub fn bar_state_path(&self) -> PathBuf {
        self.state_dir.join(BAR_STATE_FILE)
    }
    /// Every known form of the user's home directory.
    pub fn homes(&self) -> impl Iterator<Item = &PathBuf> {
        self.home.iter().chain(&self.account_home)
    }
    /// Validates (or creates with 0700) the state dir and pre-creates the DB file 0600.
    /// Existing entries are never chmodded: wrong owner, group/other bits or symlinks fail.
    pub fn prepare(&self) -> Result<()> {
        let uid = sys::current_uid()?;
        let dir = &self.state_dir;
        match fs::symlink_metadata(dir) {
            Err(e) if e.kind() == ErrorKind::NotFound => {
                DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
            }
            Err(e) => return Err(e.into()),
            Ok(_) => {}
        }
        check_entry(dir, &fs::symlink_metadata(dir)?, uid, true)?;
        match fs::symlink_metadata(&self.db_path) {
            Err(e) if e.kind() == ErrorKind::NotFound => {
                // O_CREAT|O_EXCL never follows a (dangling) symlink.
                match OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&self.db_path)
                {
                    Ok(_) => {}
                    Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(e.into()),
                }
            }
            Err(e) => return Err(e.into()),
            Ok(_) => {}
        }
        check_entry(
            &self.db_path,
            &fs::symlink_metadata(&self.db_path)?,
            uid,
            false,
        )?;
        for suffix in ["-wal", "-shm"] {
            let mut side = self.db_path.clone().into_os_string();
            side.push(suffix);
            let side = PathBuf::from(side);
            match fs::symlink_metadata(&side) {
                Ok(m) => check_entry(&side, &m, uid, false)?,
                Err(e) if e.kind() == ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
}

fn plain_absolute(p: &Path) -> bool {
    p.is_absolute()
        && !p
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
}

fn check_entry(path: &Path, m: &Metadata, uid: u32, dir: bool) -> Result<()> {
    let fail = |why: &str| {
        Err(StorageError::UnsafeStateDir(format!(
            "{}: {why}",
            path.display()
        )))
    };
    if m.file_type().is_symlink() {
        return fail("is a symlink");
    }
    if dir && !m.is_dir() {
        return fail("is not a directory");
    }
    if !dir && !m.is_file() {
        return fail("is not a regular file");
    }
    if m.uid() != uid {
        return fail("is not owned by the current user");
    }
    if m.mode() & 0o077 != 0 {
        return fail("has group or other permission bits");
    }
    Ok(())
}
