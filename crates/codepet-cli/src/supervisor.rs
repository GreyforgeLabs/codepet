//! In-process per-job mock supervisor. It spawns no process, opens no socket, and never
//! reads or writes the selected project folder: the artifact is built from stored task
//! fields only. Every step is a durable compare-and-set that honors `cancelling`, and each
//! committed step republishes `bar-state.json` (failures only reach stderr).
use crate::rpc::publish_bar_state;
use codepet_core::{State, Verification};
use codepet_storage::{MockOutcome, Result, SealOutcome, Step, Store, Supervisor, TaskSummary};
use std::io::Write;
use std::thread;
use std::time::Duration;

/// Bytes of the instructions quoted in the simulated artifact.
const INSTRUCTIONS_EXCERPT_BYTES: usize = 240;

/// Work the `task.start` process continues with after printing its response.
pub struct Job {
    pub store: Store,
    pub task_id: String,
    pub supervisor: Supervisor,
}

/// How a supervised run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    ReviewReady,
    Failed,
    Cancelled,
}

/// Runs the job to a terminal or review state. On an unexpected storage error it tries
/// once to record `failed` (execution failure is terminal) and reports to stderr.
pub fn run(mut job: Job) -> Result<End> {
    let result = drive(&mut job.store, &job.task_id, job.supervisor);
    if let Err(e) = &result {
        let _ = writeln!(std::io::stderr(), "codepet: mock supervisor stopped: {e}");
        if job.store.fail(&job.task_id, job.supervisor).is_ok() {
            publish_bar_state(&job.store);
        }
    }
    result
}

/// Runs one storage step and, once it has committed, republishes the bar state.
fn step<T>(store: &mut Store, f: impl FnOnce(&mut Store) -> Result<T>) -> Result<T> {
    let out = f(store)?;
    publish_bar_state(store);
    Ok(out)
}

fn drive(store: &mut Store, id: &str, sup: Supervisor) -> Result<End> {
    let task = store.task(id)?;
    let pause = Duration::from_millis(u64::from(task.mock.step_ms));
    wait(pause);
    let running = step(store, |s| {
        s.supervisor_step(id, sup, State::Preparing, State::Running)
    })?;
    if running == Step::Cancelling {
        return cancelled(store, id, sup);
    }
    wait(pause);
    if task.mock.outcome == MockOutcome::ProviderFailure {
        return match step(store, |s| s.fail(id, sup))? {
            Step::Cancelling => cancelled(store, id, sup),
            Step::Advanced { .. } => Ok(End::Failed),
        };
    }
    let verifying = step(store, |s| {
        s.supervisor_step(id, sup, State::Running, State::Verifying)
    })?;
    if verifying == Step::Cancelling {
        return cancelled(store, id, sup);
    }
    wait(pause);
    let verification = match task.mock.outcome {
        MockOutcome::CheckFailed => Verification::Failed,
        MockOutcome::Artifact | MockOutcome::ProviderFailure => Verification::NotVerified,
    };
    let text = artifact(&task, verification);
    match step(store, |s| s.seal_review(id, sup, &text, verification, true))? {
        SealOutcome::Cancelling => cancelled(store, id, sup),
        SealOutcome::Sealed { .. } => Ok(End::ReviewReady),
    }
}

/// Mock containment: no child process ever existed, so cancellation is confirmed at once.
fn cancelled(store: &mut Store, id: &str, sup: Supervisor) -> Result<End> {
    step(store, |s| s.confirm_cancelled(id, sup))?;
    Ok(End::Cancelled)
}

fn wait(pause: Duration) {
    if !pause.is_zero() {
        thread::sleep(pause);
    }
}

/// Plain-text simulated artifact derived only from the task's stored fields.
pub fn artifact(task: &TaskSummary, verification: Verification) -> String {
    let mut excerpt = task.instructions.trim();
    if excerpt.len() > INSTRUCTIONS_EXCERPT_BYTES {
        let mut end = INSTRUCTIONS_EXCERPT_BYTES;
        while !excerpt.is_char_boundary(end) {
            end -= 1;
        }
        excerpt = &excerpt[..end];
    }
    let check = match verification {
        Verification::Failed => "SIMULATED check result: failed (no real check ran)",
        _ => "No check ran: NOT VERIFIED",
    };
    // The folder and scope are not echoed: they are already in the task summary, and
    // repeating them could push the artifact past its size bound.
    format!(
        "SIMULATED artifact (mock provider; no command ran, no project files were read or \
         written).\nTask {} revision {}: {} via {}.\nFolder and {} scope entries: not \
         accessed.\nInstructions ({} bytes, excerpt): {}\n{check}.",
        task.task_id,
        task.revision,
        task.category.as_str(),
        task.provider.label(),
        task.scope.len(),
        task.instructions.len(),
        excerpt,
    )
}
