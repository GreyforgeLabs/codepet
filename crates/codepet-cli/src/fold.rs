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
//!   the final export (rate-limited: if the last export is under 250 ms old, the fold
//!   waits out the rest under the lock and folds what arrived meanwhile, so bursts of
//!   short fold runs never export more than about four times a second), unlock, and one
//!   more look: records left → `LOCK_NB` again (busy → another fold has them; exit). When
//!   the run budget is spent, the rest goes to a fresh detached fold (its own budget)
//!   instead of waiting for the next hook. Every hook writes its record before trying the
//!   lock, and every holder of the lock looks again after releasing it (the fold here,
//!   `drain_once` in [`drain_once_then`]), so no record is stranded.
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
    // The child is reparented when this process exits; a longer-lived caller reaps it
    // from a thread so it never lingers as a zombie.
    if let Ok(mut child) = cmd.spawn() {
        std::thread::spawn(move || child.wait());
    }
}

/// `fold.backoff` lies in the future: the last fold failed and is backing off.
fn backing_off(state_dir: &Path) -> bool {
    fs::symlink_metadata(state_dir.join(FOLD_BACKOFF_FILE))
        .and_then(|m| m.modified())
        .is_ok_and(|t| t > SystemTime::now())
}

/// Starts a detached fold unless the last one is backing off or a fold holds the lock
/// (it will see every record already written). Used by the hook (§5.1 step 4).
pub(crate) fn trigger(state_dir: &Path) {
    if backing_off(state_dir) {
        return;
    }
    if let Some(lock) = try_lock(state_dir) {
        spawn_detached(state_dir, lock);
    }
}

/// The re-check of a holder of `fold.lock` that folds nothing itself (`agent.forget_history`
/// holds the lock across its transaction): records a hook wrote meanwhile found the lock
/// busy and started no fold, so they go to a detached fold now (§5.1 ordering invariant).
pub(crate) fn hand_off_leftovers(state_dir: &Path) {
    let has = Inbox::open(state_dir).is_ok_and(|i| i.has_records().unwrap_or(false));
    if has {
        trigger(state_dir);
    }
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
        Ok(more) => {
            let _ = fs::remove_file(state_dir.join(FOLD_BACKOFF_FILE));
            if more {
                // The run budget is spent and records are left: a fresh fold takes them.
                trigger(&state_dir);
            }
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
/// Returns how many inbox entries it removed.
fn drain(
    store: &mut Store,
    inbox: &Inbox,
    started: Instant,
    budget: Duration,
    pending: &mut bool,
) -> Result<usize, Failure> {
    let mut size = FOLD_BATCH_EVENTS;
    let mut removed = 0;
    while !budget_spent(started, budget, removed) {
        let scan = inbox.scan(now_ms())?;
        let dropped = scan.poison.len() + usize::from(scan.overflow);
        if scan.events.is_empty() && dropped == 0 {
            return Ok(removed);
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
                removed += usize::from(inbox.remove(name));
            }
            if first {
                for p in &scan.poison {
                    removed += usize::from(inbox.remove(p));
                }
                if scan.overflow {
                    removed += usize::from(inbox.remove(INBOX_OVERFLOW_FILE));
                }
                first = false;
            }
            export_after(store, b.changed, b.urgent, pending);
            if t0.elapsed() > Duration::from_millis(FOLD_BATCH_MS) {
                size = (size / 2).max(MIN_BATCH);
            }
            rest = tail;
            if rest.is_empty() || budget_spent(started, budget, removed) {
                break;
            }
        }
    }
    Ok(removed)
}

/// The real run budget remains 5 s. The debug handoff fault spends it only after a
/// completed batch, so scheduler/setup latency cannot consume a tiny test budget
/// before any progress and accidentally exercise the zero-progress stop instead.
fn budget_spent(started: Instant, budget: Duration, _removed: usize) -> bool {
    #[cfg(debug_assertions)]
    if _removed > 0 && debug_short_run() {
        return true;
    }
    started.elapsed() >= budget
}

#[cfg(debug_assertions)]
fn debug_short_run() -> bool {
    std::env::var_os("CODEPET_FAULT")
        .is_some_and(|f| f == "fold_short_run" || f == "fold_short_run_slow_start")
}

/// One fold run. `Ok(true)`: the budget is spent, records are left and this run made
/// progress, so the caller hands the rest to a fresh fold. (A run that removed nothing in
/// its whole budget does not hand off; the next hook retries.)
fn fold_loop(state_dir: &Path, lock: &Lock) -> Result<bool, Failure> {
    let started = Instant::now();
    let budget = Duration::from_millis(FOLD_RUN_MS);
    #[cfg(debug_assertions)]
    if std::env::var_os("CODEPET_FAULT").is_some_and(|f| f == "fold_short_run_slow_start") {
        std::thread::sleep(Duration::from_millis(150));
    }
    let mut store = Store::open_for_fold(state_dir, Arc::new(SystemClock))?;
    let inbox = Inbox::open(state_dir)?;
    let mut pending = false;
    let mut retention_done = false;
    let mut removed = 0;
    loop {
        removed += drain(&mut store, &inbox, started, budget, &mut pending)?;
        if !retention_done {
            let _ = store.agent_retention();
            retention_done = true;
        }
        // The last batch always exports, but never sooner than 250 ms after the previous
        // export: wait out the rest (holding the lock, so hooks keep writing and this fold
        // folds what arrives), then export once. Past the run budget it exports at once.
        while pending {
            let age = export_age_ms(state_dir);
            if age >= EXPORT_MIN_INTERVAL_MS || budget_spent(started, budget, removed) {
                let _ = store.export_bar_state();
                pending = false;
            } else {
                let rest = u64::try_from(EXPORT_MIN_INTERVAL_MS - age).unwrap_or(0);
                std::thread::sleep(Duration::from_millis(rest));
                removed += drain(&mut store, &inbox, started, budget, &mut pending)?;
            }
        }
        // Unlock before the last look: a hook that wrote after it either sees the lock
        // free and starts a fold, or its record is in this look.
        lock.unlock();
        if !inbox.has_records()? {
            return Ok(false);
        }
        if budget_spent(started, budget, removed) {
            return Ok(removed > 0);
        }
        if !lock.relock() {
            // Busy: another fold holds the lock and sees these records.
            return Ok(false);
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
/// After releasing the lock it looks again: records left (beyond the batch, or written by
/// a hook that found the lock busy meanwhile) go to a detached fold.
pub(crate) fn drain_once(store: &mut Store) {
    drain_once_then(store, trigger);
}

/// [`drain_once`] with the hand-off as a parameter (tests observe it without spawning).
/// This is the re-check every holder of `fold.lock` makes after releasing it (§5.1
/// ordering invariant): a hook that wrote while the lock was held found it busy and
/// started no fold, so records left now go to `hand_off` instead of waiting for the next
/// hook.
fn drain_once_then(store: &mut Store, hand_off: impl FnOnce(&Path)) {
    let dir = store.paths().state_dir.clone();
    if !store.agent_ready().unwrap_or(false) {
        return;
    }
    let Ok(inbox) = Inbox::open(&dir) else {
        return;
    };
    let Some(lock) = try_lock(&dir) else {
        return;
    };
    fold_one_batch(store, &dir, &inbox);
    drop(lock);
    if inbox.has_records().unwrap_or(false) {
        hand_off(&dir);
    }
}

/// `drain_once`'s batch, under the held lock.
fn fold_one_batch(store: &mut Store, dir: &Path, inbox: &Inbox) {
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
    #[cfg(debug_assertions)]
    if std::env::var_os("CODEPET_FAULT").is_some_and(|f| f == "drain_once_slow") {
        // Debug builds: hold the lock a while after the batch, so tests can write a
        // record that finds the lock busy.
        std::thread::sleep(Duration::from_millis(1500));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codepet_storage::{StatePaths, INBOX_DIR};
    use std::cell::Cell;
    use std::os::unix::fs::DirBuilderExt;

    /// A v5 state dir with an inbox holding `n` activity records of one open turn.
    fn state(n: usize) -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let xdg = tmp.path().join("xdg");
        let paths =
            StatePaths::resolve(|k| (k == "XDG_STATE_HOME").then(|| xdg.clone().into())).unwrap();
        drop(Store::open_with_first_run_placeholder(&paths, Arc::new(SystemClock)).unwrap());
        let dir = paths.state_dir.clone();
        fs::DirBuilder::new()
            .mode(0o700)
            .create(dir.join(INBOX_DIR))
            .unwrap();
        let inbox = Inbox::open(&dir).unwrap();
        let now = now_ms();
        for i in 0..n {
            let r = InboxRecord {
                v: 1,
                client: "codex".into(),
                kind: if i == 0 { "turn_start" } else { "activity" }.into(),
                session: Some("s1".into()),
                turn: Some("1".into()),
                tool: (i > 0).then(|| format!("call_{i}")),
                akey: None,
                note: None,
                tools: 0,
                interactive: true,
                test: false,
                dig: format!("{i:016x}"),
                at_ms: now - 1000 + i64::try_from(i).unwrap(),
            };
            inbox.write(&r.event_id(), &r).unwrap();
        }
        (tmp, dir)
    }

    fn inbox_len(dir: &Path) -> usize {
        fs::read_dir(dir.join(INBOX_DIR))
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".ev")
            })
            .count()
    }

    /// §6.3: one batch of at most 64 records, then (lock released) the rest is handed off.
    #[test]
    fn drain_once_folds_one_batch_and_hands_off_the_rest() {
        let (_t, dir) = state(100);
        let mut store = Store::open_for_fold(&dir, Arc::new(SystemClock)).unwrap();
        let handed = Cell::new(None);
        drain_once_then(&mut store, |d| handed.set(Some(d.to_path_buf())));
        assert_eq!(inbox_len(&dir), 100 - FOLD_BATCH_EVENTS);
        assert_eq!(handed.take().as_deref(), Some(dir.as_path()));
        // The lock was released before the hand-off (a fold can take it).
        assert!(try_lock(&dir).is_some());
    }

    /// Everything folded in the batch: nothing is handed off.
    #[test]
    fn drain_once_with_an_empty_rest_hands_off_nothing() {
        let (_t, dir) = state(10);
        let mut store = Store::open_for_fold(&dir, Arc::new(SystemClock)).unwrap();
        let handed = Cell::new(false);
        drain_once_then(&mut store, |_| handed.set(true));
        assert_eq!(inbox_len(&dir), 0);
        assert!(!handed.get());
    }

    /// Another holder has the lock: nothing is folded and nothing handed off (the holder
    /// sees the records when it looks again).
    #[test]
    fn drain_once_skips_a_held_lock() {
        let (_t, dir) = state(5);
        let held = try_lock(&dir).unwrap();
        let mut store = Store::open_for_fold(&dir, Arc::new(SystemClock)).unwrap();
        let handed = Cell::new(false);
        drain_once_then(&mut store, |_| handed.set(true));
        assert_eq!(inbox_len(&dir), 5);
        assert!(!handed.get());
        drop(held);
    }
}
