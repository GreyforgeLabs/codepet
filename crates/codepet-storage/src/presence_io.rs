//! Reading the bar leader's `presence.json` sidecar (spec §5.2 step 2).
//!
//! The file is untrusted input from the same user: it must be a regular file (never a
//! symlink), owned by the effective UID, not group- or other-writable, at most 2048 bytes,
//! and parse strictly. The handle opened is checked against the earlier `lstat` by
//! `(dev, ino)`, so a swap between the two calls is refused rather than followed. Nothing
//! here writes the file; only the bar plugin does.
use crate::sys;
use codepet_core::presence::{self, PresenceDoc, Reject};
use serde::Deserialize;
use std::fs::{self, File};
use std::io::{ErrorKind, Read};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

/// Largest accepted sidecar.
pub const MAX_PRESENCE_BYTES: u64 = 2048;

/// Strict serde mirror of [`PresenceDoc`] (core has no serde dependency).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    v: u8,
    session: String,
    life_id: String,
    started_wall_ms: i64,
    updated_wall_ms: i64,
    attended_ms: i64,
    ticks: i64,
}

/// Why a present file was not accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    Symlink,
    NotRegular,
    Swapped,
    ForeignOwner,
    Writable,
    TooLarge,
    Unreadable,
    Malformed,
    Invalid(Reject),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PresenceRead {
    /// No file: nothing to settle.
    Missing,
    /// A file exists but must not be charged; counted in `meta.presence_rejections`.
    Refused(Refusal),
    Doc(PresenceDoc),
}

/// Reads and validates the sidecar at `path` against `now_wall`.
pub(crate) fn read(path: &Path, now_wall: i64) -> PresenceRead {
    match read_inner(path, now_wall) {
        Ok(Some(doc)) => PresenceRead::Doc(doc),
        Ok(None) => PresenceRead::Missing,
        Err(why) => PresenceRead::Refused(why),
    }
}

fn read_inner(path: &Path, now_wall: i64) -> Result<Option<PresenceDoc>, Refusal> {
    let link = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(Refusal::Unreadable),
    };
    if link.file_type().is_symlink() {
        return Err(Refusal::Symlink);
    }
    if !link.is_file() {
        return Err(Refusal::NotRegular);
    }
    let file = match File::open(path) {
        Ok(f) => f,
        // Deleted between the two calls: as if it never existed.
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(Refusal::Unreadable),
    };
    let meta = file.metadata().map_err(|_| Refusal::Unreadable)?;
    if (meta.dev(), meta.ino()) != (link.dev(), link.ino()) {
        return Err(Refusal::Swapped);
    }
    if !meta.is_file() {
        return Err(Refusal::NotRegular);
    }
    let uid = sys::current_uid().map_err(|_| Refusal::Unreadable)?;
    vet(meta.uid(), uid, meta.mode(), meta.len())?;
    let mut text = Vec::new();
    file.take(MAX_PRESENCE_BYTES + 1)
        .read_to_end(&mut text)
        .map_err(|_| Refusal::Unreadable)?;
    if text.len() as u64 > MAX_PRESENCE_BYTES {
        return Err(Refusal::TooLarge);
    }
    let wire: Wire = serde_json::from_slice(&text).map_err(|_| Refusal::Malformed)?;
    let doc = PresenceDoc {
        v: wire.v,
        session: wire.session,
        life_id: wire.life_id,
        started_wall_ms: wire.started_wall_ms,
        updated_wall_ms: wire.updated_wall_ms,
        attended_ms: wire.attended_ms,
        ticks: wire.ticks,
    };
    presence::validate(&doc, now_wall).map_err(Refusal::Invalid)?;
    Ok(Some(doc))
}

/// Ownership, permission and size checks on the opened handle's metadata.
pub(crate) fn vet(file_uid: u32, uid: u32, mode: u32, len: u64) -> Result<(), Refusal> {
    if file_uid != uid {
        return Err(Refusal::ForeignOwner);
    }
    if mode & 0o022 != 0 {
        return Err(Refusal::Writable);
    }
    if len > MAX_PRESENCE_BYTES {
        return Err(Refusal::TooLarge);
    }
    Ok(())
}

/// True when something exists at `path` that a settlement would have to look at (a
/// regular file to charge or anything else to count as a rejection).
pub(crate) fn present(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}
