//! State directory resolution and fail-closed ownership/mode checks.
use crate::{sys, Result, StorageError, DB_FILE};
use std::ffi::OsString;
use std::fs::{self, DirBuilder, Metadata, OpenOptions};
use std::io::ErrorKind;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

pub const PRESENCE_FILE: &str = "presence.json";
pub const BAR_STATE_FILE: &str = "bar-state.json";

// ---- v0.3 state-dir entries (spec §5.1, §5.2, §7.3). Created only by `prepare()`-ing
// commands (rpc, state, migrate) and the clients job; the hook and the fold never create
// anything. ----
/// Hook inbox directory (0700): one `<event_id>.ev` file per event.
pub const INBOX_DIR: &str = "inbox";
/// Marker the hook touches when the inbox holds more than 2048 entries.
pub const INBOX_OVERFLOW_FILE: &str = ".overflow";
/// Empty 0600 flag that mirrors `meta.enabled = 0`; the shim exits when it exists.
pub const OFF_FLAG: &str = "off";
/// Empty 0600 flag that mirrors `meta.count_helpers = 1`; read by the shim.
pub const HELPERS_FLAG: &str = "helpers";
/// `flock` file: at most one fold process; taken before the database (lock order).
pub const FOLD_LOCK_FILE: &str = "fold.lock";
/// Fold failure marker; its mtime is the earliest retry time.
pub const FOLD_BACKOFF_FILE: &str = "fold.backoff";
/// Connect/Disconnect/test job journals, `jobs/<job_id>.json`.
pub const JOBS_DIR: &str = "jobs";
/// Per-client flags, `clients/<id>.disconnected`.
pub const CLIENTS_DIR: &str = "clients";
/// Suffix of a client's disconnected flag.
pub const DISCONNECTED_SUFFIX: &str = ".disconnected";
/// The ready check's nonce (0600); only events carrying it count as test events.
pub const TEST_NONCE_FILE: &str = "test.nonce";

// ---- v0.3 data-dir entries (`$XDG_DATA_HOME/codepet`, spec §5.1). Release pruning never
// touches these. ----
/// The frozen POSIX shim every client artifact runs.
pub const SHIM_REL_PATH: &str = "bin/codepet-hook";
/// Shipped client integrations, e.g. the Claude Code local marketplace.
pub const INTEGRATIONS_DIR: &str = "integrations";
/// Same-filesystem staging area for plugin dirs that are renamed into scanned dirs.
pub const STAGING_DIR: &str = "staging";

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
    /// `$XDG_DATA_HOME/codepet` or `$HOME/.local/share/codepet` (shim, integrations,
    /// staging); `None` when neither is a plain absolute path. Never created here.
    pub data_dir: Option<PathBuf>,
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
        // An unusable XDG_DATA_HOME is ignored (XDG base-dir rules), unlike the state home.
        let data_dir = env("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .filter(|d| plain_absolute(d))
            .or_else(|| home.as_ref().map(|h| h.join(".local/share")))
            .map(|d| d.join("codepet"));
        Ok(Self {
            db_path: state_dir.join(DB_FILE),
            state_dir,
            home,
            account_home: None,
            data_dir,
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
    /// `<state_dir>/inbox`.
    pub fn inbox_dir(&self) -> PathBuf {
        self.state_dir.join(INBOX_DIR)
    }
    /// `<state_dir>/inbox/.overflow`.
    pub fn inbox_overflow_path(&self) -> PathBuf {
        self.inbox_dir().join(INBOX_OVERFLOW_FILE)
    }
    /// `<state_dir>/off`.
    pub fn off_flag_path(&self) -> PathBuf {
        self.state_dir.join(OFF_FLAG)
    }
    /// `<state_dir>/helpers`.
    pub fn helpers_flag_path(&self) -> PathBuf {
        self.state_dir.join(HELPERS_FLAG)
    }
    /// `<state_dir>/fold.lock`.
    pub fn fold_lock_path(&self) -> PathBuf {
        self.state_dir.join(FOLD_LOCK_FILE)
    }
    /// `<state_dir>/fold.backoff`.
    pub fn fold_backoff_path(&self) -> PathBuf {
        self.state_dir.join(FOLD_BACKOFF_FILE)
    }
    /// `<state_dir>/jobs`.
    pub fn jobs_dir(&self) -> PathBuf {
        self.state_dir.join(JOBS_DIR)
    }
    /// `<state_dir>/jobs/<job_id>.json`. The caller validates `job_id`.
    pub fn job_path(&self, job_id: &str) -> PathBuf {
        self.jobs_dir().join(format!("{job_id}.json"))
    }
    /// `<state_dir>/clients`.
    pub fn clients_dir(&self) -> PathBuf {
        self.state_dir.join(CLIENTS_DIR)
    }
    /// `<state_dir>/clients/<client>.disconnected`. The caller passes a supported id.
    pub fn disconnected_flag_path(&self, client: &str) -> PathBuf {
        self.clients_dir()
            .join(format!("{client}{DISCONNECTED_SUFFIX}"))
    }
    /// `<state_dir>/test.nonce`.
    pub fn test_nonce_path(&self) -> PathBuf {
        self.state_dir.join(TEST_NONCE_FILE)
    }
    /// `<data_dir>/bin/codepet-hook`.
    pub fn shim_path(&self) -> Option<PathBuf> {
        self.data_dir.as_ref().map(|d| d.join(SHIM_REL_PATH))
    }
    /// `<data_dir>/integrations`.
    pub fn integrations_dir(&self) -> Option<PathBuf> {
        self.data_dir.as_ref().map(|d| d.join(INTEGRATIONS_DIR))
    }
    /// `<data_dir>/staging`.
    pub fn staging_dir(&self) -> Option<PathBuf> {
        self.data_dir.as_ref().map(|d| d.join(STAGING_DIR))
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

#[cfg(test)]
mod v03_tests {
    use super::*;

    fn env_of(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
        move |k| {
            pairs
                .iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn v03_state_entries_live_in_the_state_dir() {
        let p = StatePaths::resolve(env_of(&[("HOME", "/h/u")])).unwrap();
        let s = Path::new("/h/u/.local/state/codepet");
        assert_eq!(p.inbox_dir(), s.join("inbox"));
        assert_eq!(p.inbox_overflow_path(), s.join("inbox/.overflow"));
        assert_eq!(p.off_flag_path(), s.join("off"));
        assert_eq!(p.helpers_flag_path(), s.join("helpers"));
        assert_eq!(p.fold_lock_path(), s.join("fold.lock"));
        assert_eq!(p.fold_backoff_path(), s.join("fold.backoff"));
        assert_eq!(p.jobs_dir(), s.join("jobs"));
        assert_eq!(p.job_path("j1"), s.join("jobs/j1.json"));
        assert_eq!(p.clients_dir(), s.join("clients"));
        assert_eq!(
            p.disconnected_flag_path("codex"),
            s.join("clients/codex.disconnected")
        );
        assert_eq!(p.test_nonce_path(), s.join("test.nonce"));
    }

    #[test]
    fn v03_data_dir_follows_xdg_data_home_or_home() {
        let p = StatePaths::resolve(env_of(&[("HOME", "/h/u")])).unwrap();
        let d = Path::new("/h/u/.local/share/codepet");
        assert_eq!(p.data_dir.as_deref(), Some(d));
        assert_eq!(p.shim_path(), Some(d.join("bin/codepet-hook")));
        assert_eq!(p.integrations_dir(), Some(d.join("integrations")));
        assert_eq!(p.staging_dir(), Some(d.join("staging")));
        let p =
            StatePaths::resolve(env_of(&[("HOME", "/h/u"), ("XDG_DATA_HOME", "/x/d")])).unwrap();
        assert_eq!(p.data_dir.as_deref(), Some(Path::new("/x/d/codepet")));
        // A relative or dotted XDG_DATA_HOME is ignored, never an error.
        for bad in ["rel/d", "/x/../d", ""] {
            let pairs: &'static [(&'static str, &'static str)] = match bad {
                "rel/d" => &[("HOME", "/h/u"), ("XDG_DATA_HOME", "rel/d")],
                "/x/../d" => &[("HOME", "/h/u"), ("XDG_DATA_HOME", "/x/../d")],
                _ => &[("HOME", "/h/u"), ("XDG_DATA_HOME", "")],
            };
            let p = StatePaths::resolve(env_of(pairs)).unwrap();
            assert_eq!(p.data_dir.as_deref(), Some(d), "{bad}");
        }
        let p = StatePaths::resolve(env_of(&[("XDG_STATE_HOME", "/x/s")])).unwrap();
        assert_eq!(p.data_dir, None);
        assert_eq!(p.shim_path(), None);
    }
}
