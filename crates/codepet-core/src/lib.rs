//! Pure, in-memory mock model. No filesystem, network, provider, or process IO.
//! Receipts and snapshots are NOT durable storage or an IPC security boundary.
use std::collections::{BTreeMap, VecDeque};
use std::path::{Component, Path};
use std::sync::atomic::{AtomicU64, Ordering};
static INSTANCE: AtomicU64 = AtomicU64::new(1);

pub const MAX_PROMPT_BYTES: usize = 16 * 1024;
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
pub const MAX_EVENTS: usize = 128;
pub const MAX_TASKS: usize = 256;
pub const PROTOCOL_VERSION: u16 = 1;
pub type JobId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Hunger,
    Thirst,
    MentalHealth,
    Hygiene,
}
impl Category {
    fn index(self) -> usize {
        match self {
            Self::Hunger => 0,
            Self::Thirst => 1,
            Self::MentalHealth => 2,
            Self::Hygiene => 3,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meters {
    values: [u16; 4],
}
impl Default for Meters {
    fn default() -> Self {
        Self { values: [7000; 4] }
    }
}
impl Meters {
    pub fn hundredths(&self, category: Category) -> u16 {
        self.values[category.index()]
    }
    fn reward(&mut self, category: Category) {
        let v = &mut self.values[category.index()];
        *v = v.saturating_add(2000).min(10000);
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub decay_enabled: bool,
    pub decay_per_hour: [u16; 4],
    pub timeout_seconds: u32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            decay_enabled: false,
            decay_per_hour: [100, 100, 50, 50],
            timeout_seconds: 1800,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), Error> {
        if self.decay_per_hour.iter().any(|r| *r > 500)
            || !(1..=7200).contains(&self.timeout_seconds)
        {
            return Err(Error::InvalidSettings);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Mock,
    Codex,
    OpenCode,
    Hermes,
    ClaudeCode,
}
impl Provider {
    pub fn supported(self) -> bool {
        self == Self::Mock
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    pub category: Category,
    pub provider: Provider,
    pub folder: String,
    pub scope: Vec<String>,
    pub instructions: String,
}
impl Draft {
    pub fn validate(&self) -> Result<(), Error> {
        if self.instructions.trim().is_empty()
            || self.instructions.len() > MAX_PROMPT_BYTES
            || self.instructions.contains('\0')
        {
            return Err(Error::InvalidDraft);
        }
        // Lexical validation only: real execution must canonicalize and check ownership,
        // home/XDG ancestors, symlinks, mounts, filesystem types, and containment.
        let root = Path::new(&self.folder);
        if self.folder.len() > 4096
            || self.folder.contains('\0')
            || !root.is_absolute()
            || root.components().any(|c| matches!(c, Component::ParentDir))
            || root == Path::new("/")
            || ["/proc", "/sys", "/dev"]
                .iter()
                .any(|p| root.starts_with(p))
        {
            return Err(Error::InvalidPath);
        }
        if self.scope.is_empty() || self.scope.len() > 64 {
            return Err(Error::InvalidPath);
        }
        for path in &self.scope {
            let p = Path::new(path);
            if path.is_empty()
                || path.len() > 4096
                || path.contains('\0')
                || p.is_absolute()
                || p.components()
                    .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
            {
                return Err(Error::InvalidPath);
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Draft,
    Preparing,
    Running,
    WaitingForUser,
    Verifying,
    ReviewReady,
    Cancelling,
    Cancelled,
    Failed,
    Interrupted,
    AcceptedArtifact,
    Discarded,
    RecoveryRequired,
}
impl State {
    pub fn occupies_slot(self) -> bool {
        matches!(
            self,
            Self::Preparing
                | Self::Running
                | Self::WaitingForUser
                | Self::Verifying
                | Self::ReviewReady
                | Self::Cancelling
                | Self::RecoveryRequired
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verification {
    Passed,
    Failed,
    NotVerified,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Review {
    pub digest: u64,
    pub artifact: String,
    pub verification: Verification,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub job_id: JobId,
    pub category: Category,
    pub verification: Verification,
    pub override_reason: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Task {
    pub id: JobId,
    pub revision: u64,
    pub draft: Draft,
    pub state: State,
    pub review: Option<Review>,
    pub receipt: Option<Receipt>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub seq: u64,
    pub job_id: JobId,
    pub state: State,
}
/// Opaque model capability. Not serialized, not a production authentication token.
#[derive(Clone, Debug)]
pub struct StartToken {
    instance: u64,
    job: JobId,
    revision: u64,
    generation: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidDraft,
    InvalidPath,
    InvalidSettings,
    UnsupportedProvider,
    UnknownJob,
    InvalidTransition,
    Busy,
    StaleRevision,
    InvalidToken,
    ReviewChanged,
    AcknowledgementRequired,
    OverrideRequired,
    Capacity,
    ResyncRequired,
    InvalidFrame,
}

/// Bounded protocol framing checks, not a JSON parser or implemented IPC transport.
pub fn validate_frame(version: u16, bytes: &[u8]) -> Result<&str, Error> {
    if version != PROTOCOL_VERSION
        || bytes.is_empty()
        || bytes.len() > MAX_FRAME_BYTES
        || !bytes.ends_with(b"\n")
        || bytes[..bytes.len() - 1].contains(&b'\n')
        || bytes.contains(&0)
    {
        return Err(Error::InvalidFrame);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| Error::InvalidFrame)?;
    if text.trim().is_empty() {
        return Err(Error::InvalidFrame);
    }
    Ok(text)
}

#[derive(Debug)]
pub struct Engine {
    instance: u64,
    tasks: BTreeMap<JobId, Task>,
    meters: Meters,
    settings: Settings,
    events: VecDeque<Event>,
    next_id: u64,
    seq: u64,
    generation: u64,
    tokens: BTreeMap<JobId, StartToken>,
}
impl Default for Engine {
    fn default() -> Self {
        Self {
            instance: INSTANCE.fetch_add(1, Ordering::Relaxed),
            tasks: BTreeMap::new(),
            meters: Meters::default(),
            settings: Settings::default(),
            events: VecDeque::new(),
            next_id: 0,
            seq: 0,
            generation: 0,
            tokens: BTreeMap::new(),
        }
    }
}
impl Engine {
    pub fn meters(&self) -> &Meters {
        &self.meters
    }
    pub fn settings(&self) -> &Settings {
        &self.settings
    }
    pub fn set_settings(&mut self, settings: Settings) -> Result<(), Error> {
        settings.validate()?;
        self.settings = settings;
        Ok(())
    }
    pub fn task(&self, id: JobId) -> Result<&Task, Error> {
        self.tasks.get(&id).ok_or(Error::UnknownJob)
    }
    pub fn active_job(&self) -> Option<JobId> {
        self.tasks
            .values()
            .find(|t| t.state.occupies_slot())
            .map(|t| t.id)
    }
    pub fn create(&mut self, draft: Draft) -> Result<JobId, Error> {
        draft.validate()?;
        if self.tasks.len() >= MAX_TASKS {
            return Err(Error::Capacity);
        }
        self.next_id = self.next_id.checked_add(1).ok_or(Error::Capacity)?;
        let id = self.next_id;
        self.tasks.insert(
            id,
            Task {
                id,
                revision: 1,
                draft,
                state: State::Draft,
                review: None,
                receipt: None,
            },
        );
        self.publish(id);
        Ok(id)
    }
    pub fn update(&mut self, id: JobId, expected: u64, draft: Draft) -> Result<(), Error> {
        draft.validate()?;
        let task = self.tasks.get_mut(&id).ok_or(Error::UnknownJob)?;
        if task.state != State::Draft {
            return Err(Error::InvalidTransition);
        }
        if task.revision != expected {
            return Err(Error::StaleRevision);
        }
        task.revision = task.revision.checked_add(1).ok_or(Error::Capacity)?;
        task.draft = draft;
        self.tokens.remove(&id);
        Ok(())
    }
    /// Mock validation only. No sandbox, provider probe, or filesystem inspection.
    pub fn preflight(&mut self, id: JobId) -> Result<StartToken, Error> {
        let task = self.task(id)?;
        if task.state != State::Draft {
            return Err(Error::InvalidTransition);
        }
        task.draft.validate()?;
        if !task.draft.provider.supported() {
            return Err(Error::UnsupportedProvider);
        }
        self.generation = self.generation.checked_add(1).ok_or(Error::Capacity)?;
        let token = StartToken {
            instance: self.instance,
            job: id,
            revision: self.task(id)?.revision,
            generation: self.generation,
        };
        self.tokens.insert(id, token.clone());
        Ok(token)
    }
    /// Call only for an explicit Start action on the exact reviewed draft.
    pub fn start(&mut self, id: JobId, token: StartToken) -> Result<(), Error> {
        if self.active_job().is_some() {
            return Err(Error::Busy);
        }
        let task = self.task(id)?;
        if task.state != State::Draft {
            return Err(Error::InvalidTransition);
        }
        let stored = self.tokens.get(&id).ok_or(Error::InvalidToken)?;
        if token.instance != self.instance
            || token.job != id
            || token.revision != task.revision
            || token.generation != stored.generation
        {
            return Err(Error::InvalidToken);
        }
        self.tokens.remove(&id);
        self.transition(id, State::Preparing)
    }
    pub fn mock_prepared(&mut self, id: JobId) -> Result<(), Error> {
        self.require(id, &[State::Preparing])?;
        self.transition(id, State::Running)
    }
    pub fn mock_wait(&mut self, id: JobId) -> Result<(), Error> {
        self.require(id, &[State::Running])?;
        self.transition(id, State::WaitingForUser)
    }
    pub fn mock_continue_after_user(&mut self, id: JobId) -> Result<(), Error> {
        self.require(id, &[State::WaitingForUser])?;
        self.transition(id, State::Running)
    }
    pub fn mock_exited(&mut self, id: JobId, success: bool) -> Result<(), Error> {
        self.require(id, &[State::Running])?;
        self.transition(
            id,
            if success {
                State::Verifying
            } else {
                State::Failed
            },
        )
    }
    pub fn mock_review(
        &mut self,
        id: JobId,
        artifact: String,
        verification: Verification,
    ) -> Result<u64, Error> {
        self.require(id, &[State::Verifying])?;
        if artifact.trim().is_empty() || artifact.len() > MAX_FRAME_BYTES || artifact.contains('\0')
        {
            return Err(Error::InvalidDraft);
        }
        self.generation = self.generation.checked_add(1).ok_or(Error::Capacity)?;
        let digest = self.generation; // Model-only identity, NOT a content hash.
        self.tasks.get_mut(&id).ok_or(Error::UnknownJob)?.review = Some(Review {
            digest,
            artifact,
            verification,
        });
        self.transition(id, State::ReviewReady)?;
        Ok(digest)
    }
    pub fn cancel(&mut self, id: JobId) -> Result<(), Error> {
        if matches!(self.task(id)?.state, State::Cancelling | State::Cancelled) {
            return Ok(());
        }
        self.require(
            id,
            &[
                State::Preparing,
                State::Running,
                State::WaitingForUser,
                State::Verifying,
            ],
        )?;
        self.transition(id, State::Cancelling)
    }
    /// Represents containment confirmation; no actual process tree exists in mock mode.
    pub fn mock_confirm_stopped(&mut self, id: JobId) -> Result<(), Error> {
        self.require(id, &[State::Cancelling])?;
        self.transition(id, State::Cancelled)
    }
    pub fn discard(&mut self, id: JobId) -> Result<(), Error> {
        self.require(id, &[State::ReviewReady])?;
        self.transition(id, State::Discarded)
    }
    /// In-memory exactly-once acceptance. Production needs a SQLite transaction.
    pub fn accept_artifact(
        &mut self,
        id: JobId,
        digest: u64,
        acknowledged: bool,
        override_reason: Option<String>,
    ) -> Result<Receipt, Error> {
        let task = self.task(id)?;
        let review = task.review.as_ref().ok_or(Error::InvalidTransition)?;
        if digest != review.digest {
            return Err(Error::ReviewChanged);
        }
        if !acknowledged {
            return Err(Error::AcknowledgementRequired);
        }
        if let Some(receipt) = &task.receipt {
            return Ok(receipt.clone());
        }
        self.require(id, &[State::ReviewReady])?;
        let reason = override_reason.map(|r| r.trim().to_owned());
        if review.verification != Verification::Passed
            && reason
                .as_ref()
                .is_none_or(|r| r.is_empty() || r.len() > 512 || r.contains('\0'))
        {
            return Err(Error::OverrideRequired);
        }
        if reason
            .as_ref()
            .is_some_and(|r| r.len() > 512 || r.contains('\0'))
        {
            return Err(Error::OverrideRequired);
        }
        let receipt = Receipt {
            job_id: id,
            category: task.draft.category,
            verification: review.verification,
            override_reason: reason,
        };
        self.meters.reward(receipt.category);
        self.tasks.get_mut(&id).ok_or(Error::UnknownJob)?.receipt = Some(receipt.clone());
        self.transition(id, State::AcceptedArtifact)?;
        Ok(receipt)
    }
    /// Testable restart policy on an in-memory snapshot. Does not load disk state.
    /// Running work is interrupted; pending review requires explicit revalidation.
    pub fn recover_snapshot(mut self) -> Self {
        self.tokens.clear();
        self.instance = INSTANCE.fetch_add(1, Ordering::Relaxed);
        let ids: Vec<_> = self.tasks.keys().copied().collect();
        for id in ids {
            let next = match self.tasks[&id].state {
                State::Preparing
                | State::Running
                | State::WaitingForUser
                | State::Verifying
                | State::Cancelling => Some(State::Interrupted),
                State::ReviewReady => Some(State::RecoveryRequired),
                _ => None,
            };
            if let Some(state) = next {
                self.tasks.get_mut(&id).expect("existing job").state = state;
                self.publish(id);
            }
        }
        self
    }
    /// Mock-only abandonment; production must export safely and disclose recovery status.
    pub fn mock_abandon_recovery(&mut self, id: JobId) -> Result<(), Error> {
        self.require(id, &[State::RecoveryRequired])?;
        self.transition(id, State::Discarded)
    }
    pub fn events_after(&self, after: u64) -> Result<Vec<Event>, Error> {
        if after > self.seq
            || self
                .events
                .front()
                .is_some_and(|e| after < e.seq.saturating_sub(1))
        {
            return Err(Error::ResyncRequired);
        }
        Ok(self
            .events
            .iter()
            .filter(|e| e.seq > after)
            .cloned()
            .collect())
    }
    fn require(&self, id: JobId, states: &[State]) -> Result<(), Error> {
        if states.contains(&self.task(id)?.state) {
            Ok(())
        } else {
            Err(Error::InvalidTransition)
        }
    }
    fn transition(&mut self, id: JobId, state: State) -> Result<(), Error> {
        self.tasks.get_mut(&id).ok_or(Error::UnknownJob)?.state = state;
        self.publish(id);
        Ok(())
    }
    fn publish(&mut self, id: JobId) {
        self.seq += 1;
        if self.events.len() == MAX_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(Event {
            seq: self.seq,
            job_id: id,
            state: self.tasks[&id].state,
        });
    }
}

#[cfg(test)]
mod tests;
