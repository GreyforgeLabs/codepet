//! `codepet fold --state-dir <abs> [--lock-fd 0]` (spec §5.1, §6.4): folds the hook inbox
//! into the agent tables under `fold.lock`, then exports `bar-state.json`.
//!
//! - Lock order: `fold.lock` before the database, everywhere.
//! - A hook that wins `flock(LOCK_EX|LOCK_NB)` spawns this command with the held lock as
//!   its **stdin** (`--lock-fd 0`): the child holds the lock from birth, so at most one fold
//!   process exists. (The spec's fd 3 needs `pre_exec`, which `unsafe_code = "forbid"`
//!   rules out; stdin carries the same open file description safely.) Without
//!   `--lock-fd` the fold takes the lock itself and exits at once when it is busy.
//! - Isolation: `setsid`, `umask 077`, `nice 10`, cwd `/`, an empty environment and
//!   `/dev/null` output; the fold never migrates and never creates a directory.
//! - Loop: batches of at most 64 records (halved while a batch takes over 50 ms), each one
//!   `BEGIN IMMEDIATE`, until the inbox is empty or 5 s passed; then retention (hourly),
//!   the final export, unlock, and one more look: records left → `LOCK_NB` again (busy →
//!   another fold has them; exit). Every hook writes its record before trying the lock,
//!   so no record is stranded while hooks keep firing.
//! - Failure: `fold.backoff` (count, error class; mtime = retry time, 5 s doubling to
//!   10 min). Hooks skip spawning while it lies in the future; the first success removes it.
use codepet_core::agent::{
    EXPORT_MIN_INTERVAL_MS, FOLD_BACKOFF_MAX_MS, FOLD_BACKOFF_START_MS, FOLD_BATCH_EVENTS,
    FOLD_BATCH_MS, FOLD_RUN_MS,
};
use codepet_storage::{
    Inbox, InboxRecord, StorageError, Store, SystemClock, BAR_STATE_FILE, FOLD_BACKOFF_FILE,
    FOLD_LOCK_FILE, INBOX_OVERFLOW_FILE,
};
use rustix::fs::{FlockOperation, Mode, OFlags};
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The smallest batch the adaptive time budget shrinks to.
const MIN_BATCH: usize = 8;

/// Opens (creating if needed, never following a symlink) `fold.lock`, a private regular
/// file of this user.
fn open_lock_file(state_dir: &Path) -> io::Result<File> {
    let fd = rustix::fs::open(
        state_dir.join(FOLD_LOCK_FILE),
        OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    )?;
    let st = rustix::fs::fstat(&fd)?;
    let regular = rustix::fs::FileType::from_raw_mode(st.st_mode).is_file();
    if !regular || st.st_uid != rustix::process::geteuid().as_raw() {
        return Err(io::Error::other(
            "fold.lock is not a regular file of this user",
        ));
    }
    Ok(File::from(fd))
}

/// `fold.lock` with `LOCK_EX|LOCK_NB` held, or `None` when another holder has it.
pub(crate) fn try_lock(state_dir: &Path) -> Option<File> {
    let f = open_lock_file(state_dir).ok()?;
    rustix::fs::flock(&f, FlockOperation::NonBlockingLockExclusive).ok()?;
    Some(f)
}

/// Starts `codepet fold` with the held lock as stdin and does not wait for it.
pub(crate) fn spawn_detached(state_dir: &Path, lock: File) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut cmd = Command::new(exe);
    cmd.arg("fold")
        .arg("--state-dir")
        .arg(state_dir)
        .arg("--lock-fd")
        .arg("0")
        .env_clear()
        .current_dir("/")
        .stdin(Stdio::from(lock))
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(debug_assertions)]
    if let Some(f) = std::env::var_os("CODEPET_FAULT") {
        cmd.env("CODEPET_FAULT", f);
    }
    // The child is reparented when the hook exits; it is never waited for.
    let _ = cmd.spawn();
}

/// Where the held lock lives.
enum Lock {
    /// Inherited from the hook as fd 0.
    Stdin,
    Own(File),
}
impl Lock {
    fn flock(&self, op: FlockOperation) -> bool {
        match self {
            Self::Stdin => rustix::fs::flock(io::stdin().as_fd(), op).is_ok(),
            Self::Own(f) => rustix::fs::flock(f, op).is_ok(),
        }
    }
    fn unlock(&self) {
        self.flock(FlockOperation::Unlock);
    }
    fn relock(&self) -> bool {
        self.flock(FlockOperation::NonBlockingLockExclusive)
    }
}

/// stdin is this state dir's `fold.lock` (same device and inode).
fn stdin_is_lock(state_dir: &Path) -> bool {
    let Ok(st) = rustix::fs::fstat(io::stdin().as_fd()) else {
        return false;
    };
    fs::symlink_metadata(state_dir.join(FOLD_LOCK_FILE))
        .is_ok_and(|m| m.is_file() && m.dev() == st.st_dev && m.ino() == st.st_ino)
}

fn parse(args: &[OsString]) -> Option<(PathBuf, bool)> {
    let (dir, inherited) = match args {
        [flag, dir] if flag == "--state-dir" => (dir, false),
        [flag, dir, lf, zero] if flag == "--state-dir" && lf == "--lock-fd" && zero == "0" => {
            (dir, true)
        }
        _ => return None,
    };
    let dir = PathBuf::from(dir);
    let plain = dir.is_absolute()
        && !dir
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir));
    plain.then_some((dir, inherited))
}

/// A failed run, with the class the backoff records.
#[derive(Debug)]
enum Failure {
    Storage(StorageError),
    Inbox(io::Error),
}
impl Failure {
    fn class(&self) -> &'static str {
        match self {
            Self::Storage(StorageError::SchemaUnsupported(_)) => "schema",
            Self::Storage(StorageError::UnsafeStateDir(_)) => "unsafe",
            Self::Storage(StorageError::Corrupt) => "corrupt",
            Self::Storage(e) if e.retryable() => "busy",
            Self::Storage(_) => "storage",
            Self::Inbox(_) => "inbox",
        }
    }
}
impl From<StorageError> for Failure {
    fn from(e: StorageError) -> Self {
        Self::Storage(e)
    }
}
impl From<io::Error> for Failure {
    fn from(e: io::Error) -> Self {
        Self::Inbox(e)
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(e) => write!(f, "{e}"),
            Self::Inbox(e) => write!(f, "inbox unavailable: {e}"),
        }
    }
}

pub fn run(args: &[OsString]) -> i32 {
    let Some((state_dir, inherited)) = parse(args) else {
        let _ = writeln!(
            io::stderr(),
            "Usage: codepet fold --state-dir <absolute dir>. Use --help."
        );
        return 2;
    };
    // Detach from the client's session and stay out of its way.
    let _ = rustix::process::setsid();
    rustix::process::umask(Mode::from_raw_mode(0o077));
    let _ = rustix::process::nice(10);
    let lock = if inherited && stdin_is_lock(&state_dir) {
        Lock::Stdin
    } else {
        match open_lock_file(&state_dir) {
            Ok(f) => Lock::Own(f),
            Err(e) => {
                if !inherited {
                    let _ = writeln!(io::stderr(), "codepet fold: cannot open fold.lock: {e}");
                }
                return 1;
            }
        }
    };
    // Busy: another fold holds the lock and will see every record already written.
    if !lock.relock() {
        return 0;
    }
    match fold_loop(&state_dir, &lock) {
        Ok(()) => {
            let _ = fs::remove_file(state_dir.join(FOLD_BACKOFF_FILE));
            0
        }
        Err(e) => {
            write_backoff(&state_dir, e.class());
            if !inherited {
                let _ = writeln!(io::stderr(), "codepet fold: {e}");
            }
            1
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Milliseconds since `bar-state.json` last changed (large when missing).
fn export_age_ms(state_dir: &Path) -> i64 {
    fs::symlink_metadata(state_dir.join(BAR_STATE_FILE))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .map_or(i64::MAX, |d| {
            i64::try_from(d.as_millis()).unwrap_or(i64::MAX)
        })
}

/// Exports now when the batch changed something urgent or the last export is older than
/// 250 ms; otherwise leaves it pending for the run's final export (§8 rule 6).
fn export_after(store: &Store, changed: bool, urgent: bool, pending: &mut bool) {
    if !changed {
        return;
    }
    if urgent || export_age_ms(&store.paths().state_dir) >= EXPORT_MIN_INTERVAL_MS {
        let _ = store.export_bar_state();
        *pending = false;
    } else {
        *pending = true;
    }
}

/// Folds everything in the inbox in batches, until it is empty or the run budget is spent.
fn drain(
    store: &mut Store,
    inbox: &Inbox,
    started: Instant,
    pending: &mut bool,
) -> Result<(), Failure> {
    let budget = Duration::from_millis(FOLD_RUN_MS);
    let mut size = FOLD_BATCH_EVENTS;
    while started.elapsed() < budget {
        let scan = inbox.scan(now_ms())?;
        let dropped = scan.poison.len() + usize::from(scan.overflow);
        if scan.events.is_empty() && dropped == 0 {
            return Ok(());
        }
        let mut rest = &scan.events[..];
        let mut first = true;
        loop {
            let (chunk, tail) = rest.split_at(size.min(rest.len()));
            let records: Vec<InboxRecord> = chunk.iter().map(|(_, r)| r.clone()).collect();
            let t0 = Instant::now();
            let b = store.fold_batch(&records, if first { dropped } else { 0 })?;
            // Committed: the folded (and duplicate or failed) files go.
            for (name, _) in chunk {
                inbox.remove(name);
            }
            if first {
                for p in &scan.poison {
                    inbox.remove(p);
                }
                if scan.overflow {
                    inbox.remove(INBOX_OVERFLOW_FILE);
                }
                first = false;
            }
            export_after(store, b.changed, b.urgent, pending);
            if t0.elapsed() > Duration::from_millis(FOLD_BATCH_MS) {
                size = (size / 2).max(MIN_BATCH);
            }
            rest = tail;
            if rest.is_empty() || started.elapsed() >= budget {
                break;
            }
        }
    }
    Ok(())
}

fn fold_loop(state_dir: &Path, lock: &Lock) -> Result<(), Failure> {
    let started = Instant::now();
    let mut store = Store::open_for_fold(state_dir, Arc::new(SystemClock))?;
    let inbox = Inbox::open(state_dir)?;
    let mut pending = false;
    let mut retention_done = false;
    loop {
        drain(&mut store, &inbox, started, &mut pending)?;
        if !retention_done {
            let _ = store.agent_retention();
            retention_done = true;
        }
        if pending {
            let _ = store.export_bar_state();
            pending = false;
        }
        // Unlock before the last look: a hook that wrote after it either sees the lock
        // free and starts a fold, or its record is in this look.
        lock.unlock();
        if started.elapsed() >= Duration::from_millis(FOLD_RUN_MS)
            || !inbox.has_records()?
            || !lock.relock()
        {
            return Ok(());
        }
    }
}

/// Records a failed run: the retry time is the file's mtime.
fn write_backoff(state_dir: &Path, class: &str) {
    let path = state_dir.join(FOLD_BACKOFF_FILE);
    let previous = fs::symlink_metadata(&path)
        .ok()
        .filter(|m| m.is_file() && m.len() <= 256)
        .and_then(|_| {
            let mut text = String::new();
            File::open(&path)
                .ok()?
                .take(256)
                .read_to_string(&mut text)
                .ok()?;
            serde_json::from_str::<serde_json::Value>(&text).ok()?["count"].as_u64()
        })
        .unwrap_or(0);
    let count = previous.saturating_add(1);
    let shift = u32::try_from(count - 1).unwrap_or(u32::MAX).min(16);
    let delay = FOLD_BACKOFF_START_MS
        .saturating_mul(1i64 << shift)
        .min(FOLD_BACKOFF_MAX_MS);
    let retry = SystemTime::now() + Duration::from_millis(u64::try_from(delay).unwrap_or(0));
    let body = serde_json::json!({"count": count, "class": class}).to_string();
    let tmp = state_dir.join(format!(
        ".{FOLD_BACKOFF_FILE}.partial-{}",
        std::process::id()
    ));
    let written = (|| -> io::Result<()> {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(body.as_bytes())?;
        f.set_modified(retry)?;
        fs::rename(&tmp, &path)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
}

/// `codepet state` and `state.get`: fold at most one batch first, when no fold holds the
/// lock (`LOCK_NB`) and the database is v5. Errors are ignored (the snapshot still answers).
pub(crate) fn drain_once(store: &mut Store) {
    let dir = store.paths().state_dir.clone();
    if !store.agent_ready().unwrap_or(false) {
        return;
    }
    let Some(_lock) = try_lock(&dir) else {
        return;
    };
    let Ok(inbox) = Inbox::open(&dir) else {
        return;
    };
    let Ok(scan) = inbox.scan(now_ms()) else {
        return;
    };
    let take = scan.events.len().min(FOLD_BATCH_EVENTS);
    let dropped = scan.poison.len() + usize::from(scan.overflow);
    if take == 0 && dropped == 0 {
        return;
    }
    let records: Vec<InboxRecord> = scan.events[..take].iter().map(|(_, r)| r.clone()).collect();
    if store.fold_batch(&records, dropped).is_ok() {
        for (name, _) in &scan.events[..take] {
            inbox.remove(name);
        }
        for p in &scan.poison {
            inbox.remove(p);
        }
        if scan.overflow {
            inbox.remove(INBOX_OVERFLOW_FILE);
        }
        let _ = fs::remove_file(dir.join(FOLD_BACKOFF_FILE));
    }
}
