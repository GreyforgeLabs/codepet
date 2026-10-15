//! Pure validation of the bar leader's `presence.json` sidecar (spec §4.4, §5).
//!
//! `codepet-core` has no serde dependency: storage parses the file with a strict
//! serde mirror, converts it to [`PresenceDoc`], then calls [`validate`]. Passing
//! validation only bounds the claim; storage still caps credit by CLOCK_BOOTTIME
//! and consumes each session exactly once.

/// Milliseconds credited per committed presence tick.
pub const TICK_MS: i64 = 60_000;
/// Largest accepted tick count (one year of minutes).
pub const MAX_TICKS: i64 = 525_600;
/// Largest accepted wall-clock lead of `updated_wall_ms` over the reader's clock.
pub const FUTURE_SLACK_MS: i64 = 120_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresenceDoc {
    pub v: u8,
    pub session: String,
    pub life_id: String,
    pub started_wall_ms: i64,
    pub updated_wall_ms: i64,
    pub attended_ms: i64,
    pub ticks: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reject {
    /// `v != 1`.
    Version,
    /// Session id does not match `^bar-[a-z0-9-]{6,58}$`.
    Session,
    /// Life id does not match `^life-[0-9a-f]{24}$`.
    LifeId,
    /// `ticks` outside `0..=525600`.
    Ticks,
    /// `attended_ms` negative or above `ticks × 60000`.
    Attended,
    /// `started_wall_ms > updated_wall_ms`.
    Order,
    /// `attended_ms > (updated − started) + 60000`.
    Window,
    /// `updated_wall_ms > now_wall + 120000`.
    Future,
}

impl Reject {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Version => "version",
            Self::Session => "session",
            Self::LifeId => "life_id",
            Self::Ticks => "ticks",
            Self::Attended => "attended",
            Self::Order => "order",
            Self::Window => "window",
            Self::Future => "future",
        }
    }
}

/// `^bar-[a-z0-9-]{6,58}$`
pub fn is_session_id(s: &str) -> bool {
    s.strip_prefix("bar-").is_some_and(|rest| {
        (6..=58).contains(&rest.len())
            && rest
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    })
}

/// `^life-[0-9a-f]{24}$`
pub fn is_life_id(s: &str) -> bool {
    s.strip_prefix("life-").is_some_and(|rest| {
        rest.len() == 24
            && rest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// Checks every §4.4 bound. Arithmetic is widened so extreme values are rejected,
/// never wrapped.
pub fn validate(doc: &PresenceDoc, now_wall: i64) -> Result<(), Reject> {
    if doc.v != 1 {
        return Err(Reject::Version);
    }
    if !is_session_id(&doc.session) {
        return Err(Reject::Session);
    }
    if !is_life_id(&doc.life_id) {
        return Err(Reject::LifeId);
    }
    if !(0..=MAX_TICKS).contains(&doc.ticks) {
        return Err(Reject::Ticks);
    }
    let attended = i128::from(doc.attended_ms);
    if attended < 0 || attended > i128::from(doc.ticks) * i128::from(TICK_MS) {
        return Err(Reject::Attended);
    }
    let (started, updated) = (
        i128::from(doc.started_wall_ms),
        i128::from(doc.updated_wall_ms),
    );
    if started > updated {
        return Err(Reject::Order);
    }
    if attended > (updated - started) + i128::from(TICK_MS) {
        return Err(Reject::Window);
    }
    if updated > i128::from(now_wall) + i128::from(FUTURE_SLACK_MS) {
        return Err(Reject::Future);
    }
    Ok(())
}
