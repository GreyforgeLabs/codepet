//! The hook inbox (spec §5.1, §5.8): bounded event records written by `codepet hook` and
//! drained by the fold, plus the hashes both sides share (payload digest, event id and
//! the HMAC keys of `agent_vectors.json` `hmac_keys`).
//!
//! Writer invariants: it never opens SQLite and never creates directories. It opens
//! `<state>/inbox` with `O_DIRECTORY|O_NOFOLLOW` and requires owner = euid and mode 0700,
//! writes `.tmp-<event_id>-<pid>` with `O_CREAT|O_EXCL|O_NOFOLLOW` (0600, ≤ 512 B, no
//! fsync) and publishes it as `<event_id>.ev` with `renameat2(RENAME_NOREPLACE)`, so a
//! duplicate delivery of one hook firing meets `EEXIST`. Reader invariants: dotfiles are
//! ignored, `.tmp-*` files older than 60 s are removed, and any `.ev` file that is
//! unparsable, over 512 B, empty, not regular or not ours is reported as poison (the fold
//! unlinks and counts it).
use crate::sys::hex;
use codepet_core::agent::{
    valid_id, ClientId, EventKind, EVENT_BUCKET_MS, MAX_INBOX_FILES, MAX_INBOX_RECORD_BYTES,
};
use rustix::fs::{AtFlags, Mode, OFlags, RenameFlags};
use rustix::io::Errno;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// File extension of a published inbox record.
pub const INBOX_EXT: &str = ".ev";
/// Prefix of an unpublished temporary record.
pub const INBOX_TMP_PREFIX: &str = ".tmp-";
/// Stale temporary records (a hook killed between write and rename) are removed after this.
pub const STALE_TMP_MS: i64 = 60 * 1000;

/// HMAC domains (`agent_vectors.json` `hmac_keys.domains`).
pub const DOMAIN_SESSION: &str = "codepet-session-v1";
pub const DOMAIN_APPROVAL: &str = "codepet-approval-v1";
pub const DOMAIN_TURN: &str = "codepet-turn-v1";
pub const DOMAIN_TOOL: &str = "codepet-tool-v1";
pub const DOMAIN_DEDUPE: &str = "codepet-dedupe-v1";

/// `dig`: the first 16 hex of SHA-256 over the payload bytes the hook read.
pub fn payload_digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))[..16].to_owned()
}

/// The event id (and inbox file stem): duplicate deliveries of one hook firing share it.
/// `session_field` is the raw session id, or `~ak:<approval_key>` for an approval event.
pub fn event_id(client: &str, kind: &str, session_field: &str, dig: &str, at_ms: i64) -> String {
    let key = format!(
        "codepet-ev1|{client}|{kind}|{session_field}|{dig}|{}",
        at_ms.div_euclid(EVENT_BUCKET_MS)
    );
    hex(&Sha256::digest(key.as_bytes()))[..32].to_owned()
}

/// HMAC-SHA256 (RFC 2104) over `msg`.
fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut k = [0u8; BLOCK];
    if key.len() > BLOCK {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(k.map(|b| b ^ 0x36));
    inner.update(msg);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(k.map(|b| b ^ 0x5c));
    outer.update(inner);
    outer.finalize().into()
}

/// `hex(HMAC-SHA256(salt, domain US client US part1 [US part2 ...]))[..32]`, US = 0x1f.
pub fn hmac_key(salt: &[u8], domain: &str, client: &str, parts: &[&str]) -> String {
    let mut msg = Vec::with_capacity(64);
    msg.extend_from_slice(domain.as_bytes());
    msg.push(0x1f);
    msg.extend_from_slice(client.as_bytes());
    for p in parts {
        msg.push(0x1f);
        msg.extend_from_slice(p.as_bytes());
    }
    hex(&hmac_sha256(salt, &msg))[..32].to_owned()
}

/// Notification and error classes an inbox record may carry (the `agent_events.class` enum).
pub const CLASSES: [&str; 14] = [
    "permission_prompt",
    "elicitation_dialog",
    "elicitation_url_dialog",
    "agent_needs_input",
    "idle_prompt",
    "ask_user",
    "rate_limit",
    "authentication_failed",
    "invalid_request",
    "server_error",
    "max_output_tokens",
    "overloaded",
    "billing_error",
    "unknown",
];

/// One inbox record (spec §5.8), one JSON line of at most 512 bytes. Raw ids live only in
/// this short-lived 0600 file inside the 0700 inbox; the database keeps only HMACs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InboxRecord {
    pub v: u8,
    pub client: String,
    pub kind: String,
    pub session: Option<String>,
    pub turn: Option<String>,
    pub tool: Option<String>,
    pub akey: Option<String>,
    pub note: Option<String>,
    pub tools: u32,
    pub interactive: bool,
    pub test: bool,
    pub dig: String,
    pub at_ms: i64,
}
impl InboxRecord {
    /// The encoded line (with its newline), or `None` when it would exceed 512 bytes.
    pub fn encode(&self) -> Option<Vec<u8>> {
        let mut out = serde_json::to_vec(self).ok()?;
        out.push(b'\n');
        (out.len() <= MAX_INBOX_RECORD_BYTES).then_some(out)
    }
    /// Parses and validates one record (the fold's poison rules); `None` = poison.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() || bytes.len() > MAX_INBOX_RECORD_BYTES {
            return None;
        }
        let r: Self = serde_json::from_slice(bytes).ok()?;
        r.valid().then_some(r)
    }
    /// The record obeys the contract a correct hook writes.
    pub fn valid(&self) -> bool {
        let id_ok = |v: &Option<String>| v.as_deref().is_none_or(valid_id);
        let Some(client) = ClientId::parse(&self.client) else {
            return false;
        };
        let akey_ok = match (&self.session, &self.akey) {
            (Some(_), None) => true,
            (None, Some(_)) => {
                client == ClientId::Hermes
                    && matches!(self.kind.as_str(), "waiting" | "activity")
                    && self.turn.is_none()
                    && self.tool.is_none()
            }
            _ => false,
        };
        self.v == 1
            && EventKind::parse(&self.kind).is_some()
            && id_ok(&self.session)
            && id_ok(&self.turn)
            && id_ok(&self.tool)
            && id_ok(&self.akey)
            && akey_ok
            && self.note.as_deref().is_none_or(|n| CLASSES.contains(&n))
            && self.tools <= 1000
            && self.dig.len() == 16
            && self
                .dig
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
            && self.at_ms >= 0
    }
    /// The event id's session field: the session id, or `~ak:<approval_key>`.
    pub fn session_field(&self) -> String {
        match (&self.session, &self.akey) {
            (Some(s), _) => s.clone(),
            (None, Some(a)) => format!("~ak:{a}"),
            (None, None) => String::new(),
        }
    }
    /// This record's event id (equal to the file stem a correct hook chose).
    pub fn event_id(&self) -> String {
        event_id(
            &self.client,
            &self.kind,
            &self.session_field(),
            &self.dig,
            self.at_ms,
        )
    }
}

/// What a hook's write did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpoolWrite {
    Written,
    /// `<event_id>.ev` already existed: a duplicate delivery.
    Duplicate,
    /// More than 2048 entries: dropped, `inbox/.overflow` touched.
    Overflow,
    /// The record would exceed 512 bytes: dropped.
    TooLarge,
}

/// The inbox directory, opened and checked once (owner = euid, mode 0700, not a symlink).
#[derive(Debug)]
pub struct Inbox {
    dir: OwnedFd,
    uid: u32,
}

fn errno_io(e: Errno) -> io::Error {
    io::Error::from_raw_os_error(e.raw_os_error())
}

impl Inbox {
    /// Opens `<state_dir>/inbox`. Never creates it. Fails when it is missing, a symlink,
    /// not owned by the effective user or not exactly mode 0700.
    pub fn open(state_dir: &Path) -> io::Result<Self> {
        let dir = rustix::fs::open(
            state_dir.join(crate::INBOX_DIR),
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(errno_io)?;
        let st = rustix::fs::fstat(&dir).map_err(errno_io)?;
        let uid = rustix::process::geteuid().as_raw();
        if st.st_uid != uid || st.st_mode & 0o7777 != 0o700 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the inbox is not a private 0700 directory owned by this user",
            ));
        }
        Ok(Self { dir, uid })
    }

    fn entry_count(&self, cap: usize) -> io::Result<usize> {
        let mut n = 0;
        let dir = rustix::fs::Dir::read_from(&self.dir).map_err(errno_io)?;
        for entry in dir {
            let entry = entry.map_err(errno_io)?;
            let name = entry.file_name().to_bytes();
            if name == b"." || name == b".." {
                continue;
            }
            n += 1;
            if n > cap {
                break;
            }
        }
        Ok(n)
    }

    /// Writes one record as `<event_id>.ev` (the hook's only filesystem write besides
    /// `.overflow`). `event_id` must be 32 lowercase hex.
    pub fn write(&self, event_id: &str, record: &InboxRecord) -> io::Result<SpoolWrite> {
        if event_id.len() != 32 || !event_id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "bad event id"));
        }
        let Some(bytes) = record.encode() else {
            return Ok(SpoolWrite::TooLarge);
        };
        if self.entry_count(MAX_INBOX_FILES)? > MAX_INBOX_FILES {
            let flag = rustix::fs::openat(
                &self.dir,
                crate::INBOX_OVERFLOW_FILE,
                OFlags::WRONLY | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR,
            );
            drop(flag);
            return Ok(SpoolWrite::Overflow);
        }
        let tmp = format!("{INBOX_TMP_PREFIX}{event_id}-{}", std::process::id());
        let fd = rustix::fs::openat(
            &self.dir,
            tmp.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(errno_io)?;
        let written = File::from(fd).write_all(&bytes);
        let fin = format!("{event_id}{INBOX_EXT}");
        let mut renamed = false;
        let published = written.and_then(|()| {
            match rustix::fs::renameat_with(
                &self.dir,
                tmp.as_str(),
                &self.dir,
                fin.as_str(),
                RenameFlags::NOREPLACE,
            ) {
                Ok(()) => {
                    renamed = true;
                    Ok(SpoolWrite::Written)
                }
                Err(Errno::EXIST) => Ok(SpoolWrite::Duplicate),
                // A filesystem without renameat2 flags: link(2) also never replaces.
                Err(Errno::INVAL | Errno::NOSYS) => match rustix::fs::linkat(
                    &self.dir,
                    tmp.as_str(),
                    &self.dir,
                    fin.as_str(),
                    AtFlags::empty(),
                ) {
                    Ok(()) => Ok(SpoolWrite::Written),
                    Err(Errno::EXIST) => Ok(SpoolWrite::Duplicate),
                    Err(e) => Err(errno_io(e)),
                },
                Err(e) => Err(errno_io(e)),
            }
        });
        // Our temporary name (created with O_EXCL, so it is ours) never stays behind.
        if !renamed {
            let _ = rustix::fs::unlinkat(&self.dir, tmp.as_str(), AtFlags::empty());
        }
        published
    }

    /// Lists the inbox for the fold: valid records sorted by `(at_ms, name)`, poison
    /// names, and whether `.overflow` is present. Removes stale `.tmp-*` files.
    pub fn scan(&self, now_ms: i64) -> io::Result<InboxScan> {
        let mut scan = InboxScan::default();
        let dir = rustix::fs::Dir::read_from(&self.dir).map_err(errno_io)?;
        for entry in dir {
            let entry = entry.map_err(errno_io)?;
            let name = OsStr::from_bytes(entry.file_name().to_bytes());
            let Some(name) = name.to_str() else {
                continue;
            };
            if name == crate::INBOX_OVERFLOW_FILE {
                scan.overflow = true;
                continue;
            }
            if name.starts_with(INBOX_TMP_PREFIX) {
                let stale = rustix::fs::statat(&self.dir, name, AtFlags::SYMLINK_NOFOLLOW)
                    .is_ok_and(|s| {
                        let mtime_ms = s.st_mtime.saturating_mul(1000);
                        mtime_ms < now_ms - STALE_TMP_MS
                    });
                if stale {
                    let _ = rustix::fs::unlinkat(&self.dir, name, AtFlags::empty());
                }
                continue;
            }
            if name.starts_with('.') || !name.ends_with(INBOX_EXT) {
                continue;
            }
            match self.read_record(name) {
                Some(r) => scan.events.push((name.to_owned(), r)),
                None => scan.poison.push(name.to_owned()),
            }
        }
        scan.events
            .sort_by(|(an, a), (bn, b)| a.at_ms.cmp(&b.at_ms).then_with(|| an.cmp(bn)));
        scan.poison.sort();
        Ok(scan)
    }

    fn read_record(&self, name: &str) -> Option<InboxRecord> {
        let fd = rustix::fs::openat(
            &self.dir,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .ok()?;
        let st = rustix::fs::fstat(&fd).ok()?;
        let regular = rustix::fs::FileType::from_raw_mode(st.st_mode).is_file();
        let size = usize::try_from(st.st_size).ok()?;
        if !regular || st.st_uid != self.uid || size == 0 || size > MAX_INBOX_RECORD_BYTES {
            return None;
        }
        let mut buf = Vec::with_capacity(size);
        File::from(fd)
            .take(MAX_INBOX_RECORD_BYTES as u64 + 1)
            .read_to_end(&mut buf)
            .ok()?;
        InboxRecord::decode(&buf)
    }

    /// Removes one entry by name (a folded record, poison, or `.overflow`). A directory
    /// that was planted under an `.ev` name is removed only when empty.
    pub fn remove(&self, name: &str) -> bool {
        match rustix::fs::unlinkat(&self.dir, name, AtFlags::empty()) {
            Ok(()) => true,
            Err(Errno::ISDIR) => rustix::fs::unlinkat(&self.dir, name, AtFlags::REMOVEDIR).is_ok(),
            Err(_) => false,
        }
    }

    /// True when the inbox holds at least one published record (`*.ev`, not a dotfile).
    pub fn has_records(&self) -> io::Result<bool> {
        let dir = rustix::fs::Dir::read_from(&self.dir).map_err(errno_io)?;
        for entry in dir {
            let entry = entry.map_err(errno_io)?;
            let name = entry.file_name().to_bytes();
            if !name.starts_with(b".") && name.ends_with(INBOX_EXT.as_bytes()) {
                return Ok(true);
            }
            if name == crate::INBOX_OVERFLOW_FILE.as_bytes() {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// One inbox listing (see [`Inbox::scan`]).
#[derive(Debug, Default)]
pub struct InboxScan {
    /// Valid records with their file names, in fold order.
    pub events: Vec<(String, InboxRecord)>,
    /// `.ev` files that failed validation; the fold unlinks and counts them.
    pub poison: Vec<String>,
    /// `inbox/.overflow` exists (events were dropped by hooks).
    pub overflow: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{symlink, DirBuilderExt, PermissionsExt};

    const VECTORS: &str = include_str!("../../../docs/v0.3/contracts/agent_vectors.json");

    fn record(at_ms: i64) -> InboxRecord {
        InboxRecord {
            v: 1,
            client: "claude-code".into(),
            kind: "turn_done".into(),
            session: Some("11111111-2222-3333-4444-555555555555".into()),
            turn: Some("p-1".into()),
            tool: None,
            akey: None,
            note: None,
            tools: 0,
            interactive: true,
            test: false,
            dig: "35e6e00cfb35360a".into(),
            at_ms,
        }
    }

    fn state() -> (tempfile::TempDir, std::path::PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let s = t.path().join("state");
        fs::DirBuilder::new().mode(0o700).create(&s).unwrap();
        fs::DirBuilder::new()
            .mode(0o700)
            .create(s.join("inbox"))
            .unwrap();
        (t, s)
    }

    #[test]
    fn event_id_and_hmac_vectors() {
        let v: serde_json::Value = serde_json::from_str(VECTORS).unwrap();
        for vec in v["event_ids"]["vectors"].as_array().unwrap() {
            let dig = payload_digest(vec["payload_utf8"].as_str().unwrap().as_bytes());
            let id = event_id(
                vec["client"].as_str().unwrap(),
                vec["kind"].as_str().unwrap(),
                vec["session"].as_str().unwrap(),
                &dig,
                vec["at_ms"].as_i64().unwrap(),
            );
            assert_eq!(dig, vec["expect"]["dig"].as_str().unwrap());
            assert_eq!(id, vec["expect"]["event_id"].as_str().unwrap());
        }
        let h = &v["hmac_keys"];
        let salt: Vec<u8> = (0u8..32).collect();
        assert_eq!(hex(&salt), h["salt_hex"].as_str().unwrap());
        for vec in h["vectors"].as_array().unwrap() {
            let domain = h["domains"][vec["use"].as_str().unwrap()]["domain"]
                .as_str()
                .unwrap();
            let parts: Vec<&str> = vec["parts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p.as_str().unwrap())
                .collect();
            let got = hmac_key(&salt, domain, vec["client"].as_str().unwrap(), &parts);
            assert_eq!(got, vec["expect"].as_str().unwrap(), "{vec}");
        }
        for (name, d) in [
            ("session", DOMAIN_SESSION),
            ("approval", DOMAIN_APPROVAL),
            ("turn", DOMAIN_TURN),
            ("tool", DOMAIN_TOOL),
            ("dedupe", DOMAIN_DEDUPE),
        ] {
            assert_eq!(h["domains"][name]["domain"], d);
        }
    }

    #[test]
    fn hmac_matches_rfc_4231_case_2() {
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            hex(&mac),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        // A key longer than one block is hashed first (RFC 4231 case 6).
        let mac = hmac_sha256(
            &[0xaa; 131],
            b"Test Using Larger Than Block-Size Key - Hash Key First",
        );
        assert_eq!(
            hex(&mac),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn records_round_trip_and_stay_bounded() {
        let r = record(1_792_123_876_111);
        let bytes = r.encode().unwrap();
        assert!(bytes.len() <= 512 && bytes.ends_with(b"\n"));
        assert_eq!(InboxRecord::decode(&bytes).unwrap(), r);
        assert!(String::from_utf8(bytes).unwrap().starts_with(
            "{\"v\":1,\"client\":\"claude-code\",\"kind\":\"turn_done\",\"session\":"
        ));
        let mut big = r.clone();
        big.turn = Some("t".repeat(128));
        big.tool = Some("u".repeat(128));
        big.session = Some("s".repeat(128));
        assert_eq!(big.encode(), None, "over 512 bytes is never written");
        // Poison: unknown keys, bad ids, a session and an approval key together.
        for bad in [
            br#"{"v":1}"#.to_vec(),
            String::from_utf8(r.encode().unwrap())
                .unwrap()
                .replace("\"v\":1", "\"v\":1,\"prompt\":\"x\"")
                .into_bytes(),
            String::from_utf8(r.encode().unwrap())
                .unwrap()
                .replace("p-1", "../p")
                .into_bytes(),
            String::from_utf8(r.encode().unwrap())
                .unwrap()
                .replace("\"akey\":null", "\"akey\":\"k\"")
                .into_bytes(),
            Vec::new(),
            vec![b' '; 513],
        ] {
            assert_eq!(InboxRecord::decode(&bad), None, "{bad:?}");
        }
        let mut approval = r.clone();
        approval.client = "hermes".into();
        approval.kind = "waiting".into();
        approval.session = None;
        approval.turn = None;
        approval.akey = Some("sk-cli-1".into());
        assert!(approval.valid());
        assert_eq!(approval.session_field(), "~ak:sk-cli-1");
        approval.client = "opencode".into();
        assert!(!approval.valid());
    }

    #[test]
    fn write_publishes_once_and_dedupes() {
        let (_t, s) = state();
        let inbox = Inbox::open(&s).unwrap();
        let r = record(1000);
        let id = r.event_id();
        assert_eq!(inbox.write(&id, &r).unwrap(), SpoolWrite::Written);
        assert_eq!(inbox.write(&id, &r).unwrap(), SpoolWrite::Duplicate);
        let names: Vec<String> = fs::read_dir(s.join("inbox"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(
            names,
            [format!("{id}.ev")],
            "no temporary file stays behind"
        );
        let meta = fs::metadata(s.join("inbox").join(format!("{id}.ev"))).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        let scan = inbox.scan(2000).unwrap();
        assert_eq!(scan.events.len(), 1);
        assert_eq!(scan.events[0].1, r);
        assert!(inbox.has_records().unwrap());
        assert!(inbox.remove(&scan.events[0].0));
        assert!(!inbox.has_records().unwrap());
    }

    #[test]
    fn unsafe_inboxes_are_refused_and_never_created() {
        let t = tempfile::tempdir().unwrap();
        let s = t.path().join("state");
        fs::DirBuilder::new().mode(0o700).create(&s).unwrap();
        assert!(Inbox::open(&s).is_err(), "missing inbox");
        assert!(!s.join("inbox").exists(), "never created");
        fs::DirBuilder::new()
            .mode(0o755)
            .create(s.join("inbox"))
            .unwrap();
        fs::set_permissions(s.join("inbox"), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(Inbox::open(&s).is_err(), "group/other bits");
        fs::remove_dir(s.join("inbox")).unwrap();
        let real = t.path().join("elsewhere");
        fs::DirBuilder::new().mode(0o700).create(&real).unwrap();
        symlink(&real, s.join("inbox")).unwrap();
        assert!(Inbox::open(&s).is_err(), "symlinked inbox");
    }

    #[test]
    fn overflow_drops_and_touches_the_marker() {
        let (_t, s) = state();
        let inbox = Inbox::open(&s).unwrap();
        for i in 0..=MAX_INBOX_FILES {
            fs::write(s.join("inbox").join(format!("junk{i}")), b"").unwrap();
        }
        let r = record(1000);
        assert_eq!(
            inbox.write(&r.event_id(), &r).unwrap(),
            SpoolWrite::Overflow
        );
        assert!(s.join("inbox/.overflow").exists());
        assert!(!s
            .join("inbox")
            .join(format!("{}.ev", r.event_id()))
            .exists());
        assert!(inbox.scan(0).unwrap().overflow);
    }

    #[test]
    fn scan_orders_reports_poison_and_sweeps_stale_temps() {
        let (_t, s) = state();
        let inbox = Inbox::open(&s).unwrap();
        let (a, b) = (record(2000), record(1000));
        let mut b = b;
        b.turn = Some("p-2".into());
        // Same client, kind, session and 3 s bucket: only the digest tells them apart.
        b.dig = "0000000000000002".into();
        inbox.write(&a.event_id(), &a).unwrap();
        inbox.write(&b.event_id(), &b).unwrap();
        let dir = s.join("inbox");
        fs::write(dir.join("ffffffffffffffffffffffffffffffff.ev"), b"not json").unwrap();
        fs::write(dir.join("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee.ev"), b"").unwrap();
        fs::write(
            dir.join("dddddddddddddddddddddddddddddddd.ev"),
            vec![b'x'; 600],
        )
        .unwrap();
        symlink(
            "/etc/passwd",
            dir.join("cccccccccccccccccccccccccccccccc.ev"),
        )
        .unwrap();
        fs::write(dir.join(".hidden.ev"), b"x").unwrap();
        fs::write(dir.join(".tmp-old-1"), b"x").unwrap();
        let scan = inbox.scan(i64::MAX / 2).unwrap();
        let order: Vec<i64> = scan.events.iter().map(|(_, r)| r.at_ms).collect();
        assert_eq!(order, [1000, 2000]);
        assert_eq!(scan.poison.len(), 4, "{:?}", scan.poison);
        assert!(!dir.join(".tmp-old-1").exists(), "stale temp swept");
        assert!(dir.join(".hidden.ev").exists(), "dotfiles are ignored");
        for p in &scan.poison {
            assert!(inbox.remove(p));
        }
        assert!(Path::new("/etc/passwd").exists());
        // A fresh temp file (a hook mid-write) is left alone.
        fs::write(dir.join(".tmp-new-1"), b"x").unwrap();
        inbox.scan(0).unwrap();
        assert!(dir.join(".tmp-new-1").exists());
    }
}
