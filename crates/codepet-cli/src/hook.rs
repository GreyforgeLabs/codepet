//! `codepet hook --state-dir <abs> <client-id>` (spec §5.1, §5.8, §5.9).
//!
//! Invariant O2: every path exits 0 with empty stdout and stderr; the hook never opens
//! SQLite, never creates a directory, takes only a non-blocking lock and makes no network
//! call. It is bounded by a 1 s watchdog (the `alarm(1)` of the spec, as a thread because
//! unsafe code is forbidden) and a 250 ms stdin deadline. In order:
//! 1. the `off` flag and the client's `disconnected` flag (one `stat` each, no DB);
//! 2. a bounded read (≤ 64 KiB) through the top-level-only tokenizer, normalization
//!    (argv client id authoritative, schema sanity, child events dropped, content never
//!    read) and the interactivity check;
//! 3. the inbox write (`.tmp-<id>-<pid>` → `renameat2(RENAME_NOREPLACE)` → `<id>.ev`);
//! 4. unless `fold.backoff` lies in the future, `flock(fold.lock, LOCK_EX|LOCK_NB)`; when
//!    acquired, a detached `codepet fold` inherits the held lock as its stdin;
//! 5. a drain of the rest of stdin to EOF within the same deadline.
mod input;
mod procwalk;
mod scan;
pub(crate) mod shim;

use codepet_core::agent::{
    ancestor_comm, interactive, normalize, ClientId, Normalized, HOOK_DRAIN_MS, MAX_HOOK_READ_BYTES,
};
use codepet_storage::{
    event_id, payload_digest, Inbox, InboxRecord, CLIENTS_DIR, DISCONNECTED_SUFFIX,
    FOLD_BACKOFF_FILE, OFF_FLAG, TEST_NONCE_FILE,
};
use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The watchdog ends a stuck hook after this long (exit 0).
const WATCHDOG: Duration = Duration::from_secs(1);
/// The nonce file is tiny; anything larger is refused.
const MAX_NONCE_BYTES: u64 = 64;

/// Runs the hook and returns its exit code, which is always 0.
pub fn run(args: &[OsString]) -> i32 {
    let started = Instant::now();
    // Silent: a panic prints nothing and ends in exit 0.
    std::panic::set_hook(Box::new(|_| {}));
    let _ = std::panic::catch_unwind(|| hook(args, started));
    0
}

fn hook(args: &[OsString], started: Instant) {
    let deadline = started + Duration::from_millis(HOOK_DRAIN_MS);
    let Some((state_dir, client)) = parse_args(args) else {
        input::drain(deadline);
        return;
    };
    if exists(&state_dir.join(OFF_FLAG))
        || exists(
            &state_dir
                .join(CLIENTS_DIR)
                .join(format!("{}{DISCONNECTED_SUFFIX}", client.as_str())),
        )
    {
        input::drain(deadline);
        return;
    }
    std::thread::spawn(|| {
        std::thread::sleep(WATCHDOG);
        std::process::exit(0);
    });
    #[cfg(debug_assertions)]
    debug_faults();
    let at_ms = now_ms();
    let read = input::read_bounded(MAX_HOOK_READ_BYTES, deadline);
    if let Some(record) = record_for(&state_dir, client, &read, at_ms) {
        if let Ok(inbox) = Inbox::open(&state_dir) {
            let id = record.event_id();
            if inbox.write(&id, &record).is_ok() {
                trigger_fold(&state_dir);
            }
        }
    }
    if !read.eof {
        input::drain(deadline);
    }
}

/// `--state-dir <abs> <client-id>`; anything else is ignored silently (O2).
fn parse_args(args: &[OsString]) -> Option<(PathBuf, ClientId)> {
    let [flag, dir, id] = args else {
        return None;
    };
    if flag != "--state-dir" {
        return None;
    }
    let dir = PathBuf::from(dir);
    let plain = dir.is_absolute()
        && !dir
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir));
    let client = ClientId::parse(id.to_str()?)?;
    plain.then_some((dir, client))
}

fn exists(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok()
}

fn now_ms() -> i64 {
    #[cfg(debug_assertions)]
    if let Some(ms) = std::env::var("CODEPET_TEST_NOW_MS")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        return ms;
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Debug builds only: `CODEPET_FAULT=panic_in_hook` panics (caught, exit 0) and
/// `CODEPET_FAULT=stuck_in_hook` hangs until the watchdog ends the process.
#[cfg(debug_assertions)]
fn debug_faults() {
    match std::env::var("CODEPET_FAULT").as_deref() {
        Ok("panic_in_hook") => panic!("injected hook panic"),
        Ok("stuck_in_hook") => std::thread::sleep(Duration::from_secs(30)),
        _ => {}
    }
}

/// `$STATE/test.nonce`: a private regular file of this user, at most 64 bytes, trailing
/// newline stripped. Read only for payloads that carry `codepet_test`.
fn read_nonce(state_dir: &Path) -> Option<String> {
    let path = state_dir.join(TEST_NONCE_FILE);
    let lmeta = fs::symlink_metadata(&path).ok()?;
    let uid = rustix::process::geteuid().as_raw();
    if !lmeta.is_file() || lmeta.uid() != uid || lmeta.len() > MAX_NONCE_BYTES {
        return None;
    }
    let fd = rustix::fs::open(
        &path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .ok()?;
    let file = fs::File::from(fd);
    let meta = file.metadata().ok()?;
    if (meta.dev(), meta.ino()) != (lmeta.dev(), lmeta.ino()) {
        return None;
    }
    let mut text = String::new();
    file.take(MAX_NONCE_BYTES).read_to_string(&mut text).ok()?;
    Some(text.trim_end_matches('\n').to_owned())
}

/// The inbox record for one payload, or `None` when it is dropped.
fn record_for(
    state_dir: &Path,
    client: ClientId,
    read: &input::Read,
    at_ms: i64,
) -> Option<InboxRecord> {
    let (payload, end) = scan::scan(&read.bytes, read.capped).ok()?;
    let truncated = end == scan::End::Truncated;
    let n: Normalized = normalize(client, &payload, truncated, || read_nonce(state_dir)).ok()?;
    let interactive = match n.interactive {
        Some(i) => i,
        None => {
            let entrypoint = (client == ClientId::ClaudeCode)
                .then(|| std::env::var("CLAUDE_CODE_ENTRYPOINT").ok())
                .flatten();
            // The environment decides when it can; /proc only otherwise.
            let argv = match (&entrypoint, ancestor_comm(client)) {
                (None, Some(comm)) => procwalk::ancestor_argv(comm),
                _ => None,
            };
            interactive(client, entrypoint.as_deref(), argv.as_deref())
        }
    };
    let record = InboxRecord {
        v: 1,
        client: client.as_str().to_owned(),
        kind: n.kind.as_str().to_owned(),
        session: n.session,
        turn: n.turn,
        tool: n.tool,
        akey: n.akey,
        note: n.class.map(str::to_owned),
        tools: n.tools,
        interactive,
        test: n.test,
        dig: payload_digest(&read.bytes),
        at_ms,
    };
    debug_assert_eq!(
        record.event_id(),
        event_id(
            &record.client,
            &record.kind,
            &record.session_field(),
            &record.dig,
            at_ms
        )
    );
    record.valid().then_some(record)
}

/// Step 4: start a fold unless one runs (lock busy) or the last one is backing off.
fn trigger_fold(state_dir: &Path) {
    if let Ok(m) = fs::symlink_metadata(state_dir.join(FOLD_BACKOFF_FILE)) {
        if m.modified().is_ok_and(|t| t > SystemTime::now()) {
            return;
        }
    }
    let Some(lock) = crate::fold::try_lock(state_dir) else {
        return;
    };
    crate::fold::spawn_detached(state_dir, lock);
}
