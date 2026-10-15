//! `codepet rpc`: one request frame in, one response frame out. Malformed or unimplemented
//! requests are refused before the state directory is touched.
use crate::errors::Failure;
use crate::supervisor::{self, Job};
use codepet_core::{Category, Draft, Provider, Settings, PROTOCOL_VERSION};
use codepet_protocol::{
    parse_params, parse_request, read_frame, DraftParams, EmptyParams, ErrorCode,
    EventsSubscribeParams, HelloParams, HistoryListParams, Method, PetPauseParams, ProtocolError,
    Request, Response, SettingsUpdateParams, TaskAcceptArtifactParams, TaskDiscardParams,
    TaskIdParams, TaskPreflightParams, TaskStartParams, TaskUpdateParams, NOT_IMPLEMENTED_MESSAGE,
};
use codepet_storage::{
    AcceptRequest, MockOptions, MockOutcome, ProviderInfo, StatePaths, Store, Supervisor,
    SystemClock, TaskInput,
};
use serde_json::{json, Value};
use std::io::{self, Write};
use std::sync::Arc;

type Outcome = Result<(Value, Option<Job>), Failure>;

/// Reads one frame from stdin, answers it on stdout and, after a successful `task.start`,
/// continues as that task's mock supervisor. Returns the process exit code.
pub fn run() -> i32 {
    let read = read_frame(&mut io::stdin().lock());
    let (response, job) = match read {
        Ok(Some(frame)) => handle(&frame),
        Ok(None) => (
            Response::err(None, ErrorCode::InvalidRequest, "no request frame on stdin"),
            None,
        ),
        Err(e) => (ProtocolError::from(e).to_response(), None),
    };
    let (frame, ok) = match response.to_frame() {
        Ok(frame) => (frame, response.ok),
        Err(_) => (response.to_frame_or_limit(), false),
    };
    let mut out = io::stdout().lock();
    let written = out.write_all(frame.as_bytes()).and_then(|()| out.flush());
    drop(out);
    // The start already committed `preparing` under this PID, so supervise even if the
    // client went away: the job then ends in a durable state instead of `interrupted`.
    if let Some(job) = job {
        let _ = supervisor::run(job);
    }
    match (written, ok) {
        (Ok(()), true) => 0,
        _ => 1,
    }
}

/// `codepet state`: the `state.get` result, or a typed failure.
pub fn state_get() -> Result<Value, Failure> {
    Ok(serde_json::to_value(open()?.snapshot()?)?)
}

/// Parses and dispatches one frame (without its newline).
pub fn handle(frame: &str) -> (Response, Option<Job>) {
    let request = match parse_request(frame) {
        Ok(r) => r,
        Err(e) => return (e.to_response(), None),
    };
    let id = request.id.clone();
    let method = request.method;
    match dispatch(request) {
        Ok((result, job)) => {
            let response = Response::ok(id.clone(), result);
            // Storage bounds every result to fit one frame (tested); if that ever fails
            // after a mutation, say the change may have been applied instead of implying
            // that nothing happened.
            if method.is_mutating() && response.to_frame().is_err() {
                let f = Failure::new(ErrorCode::OutputLimit, COMMITTED_TOO_LARGE);
                return (f.into_response(Some(id)), job);
            }
            (response, job)
        }
        Err(f) => (f.into_response(Some(id)), None),
    }
}

const COMMITTED_TOO_LARGE: &str = "The request may have been applied, but its result is too \
    large to return. Reload with state.get before retrying.";

fn open() -> Result<Store, Failure> {
    let paths = StatePaths::from_env()?;
    Ok(Store::open(&paths, Arc::new(SystemClock))?)
}

fn done(value: Value) -> Outcome {
    Ok((value, None))
}

fn encode(value: serde_json::Result<Value>) -> Outcome {
    done(value?)
}

fn dispatch(req: Request) -> Outcome {
    let p = req.params;
    match req.method {
        // Parsed but refused before any params or state are touched.
        Method::TaskLogs | Method::TaskApply | Method::TaskRollback => Err(Failure::new(
            ErrorCode::InvalidState,
            NOT_IMPLEMENTED_MESSAGE,
        )),
        Method::Hello => {
            let h: HelloParams = parse_params(p)?;
            if h.versions.is_some_and(|v| !v.contains(&PROTOCOL_VERSION)) {
                return Err(Failure::new(
                    ErrorCode::ProtocolMismatch,
                    "no common protocol version; this build speaks v1",
                ));
            }
            let last_seq = open()?.last_seq()?;
            done(json!({
                "protocol_version": PROTOCOL_VERSION,
                "supervisor": null,
                "last_seq": last_seq,
            }))
        }
        Method::StateGet => {
            let _: EmptyParams = parse_params(p)?;
            done(state_get()?)
        }
        Method::ProvidersInspect => {
            let _: EmptyParams = parse_params(p)?;
            done(json!({ "providers": ProviderInfo::all() }))
        }
        Method::EventsSubscribe => {
            let e: EventsSubscribeParams = parse_params(p)?;
            let after = to_i64(e.after_seq, "after_seq")?;
            encode(serde_json::to_value(
                open()?.events_after(after, Some(e.limit_or_default()))?,
            ))
        }
        Method::HistoryList => {
            let h: HistoryListParams = parse_params(p)?;
            encode(serde_json::to_value(open()?.history(
                h.before_ms,
                h.before_task_id.as_deref(),
                Some(h.limit_or_default()),
            )?))
        }
        Method::TaskGet => {
            let t: TaskIdParams = parse_params(p)?;
            encode(serde_json::to_value(open()?.task(&t.task_id)?))
        }
        Method::TaskCreate => {
            let d: DraftParams = parse_params(p)?;
            let input = task_input(&d)?;
            encode(serde_json::to_value(open()?.create_task(&input)?))
        }
        Method::TaskUpdate => {
            let u: TaskUpdateParams = parse_params(p)?;
            let input = task_input(&u.draft())?;
            let expected = to_i64(u.expected_revision, "expected_revision")?;
            encode(serde_json::to_value(
                open()?.update_task(&u.task_id, expected, &input)?,
            ))
        }
        Method::TaskPreflight => {
            let t: TaskPreflightParams = parse_params(p)?;
            let revision = to_i64(t.revision, "revision")?;
            encode(serde_json::to_value(
                open()?.preflight(&t.task_id, revision)?,
            ))
        }
        Method::TaskStart => {
            let t: TaskStartParams = parse_params(p)?;
            let revision = to_i64(t.revision, "revision")?;
            let mut store = open()?;
            let supervisor = Supervisor::current().map_err(|e| {
                Failure::new(
                    ErrorCode::Internal,
                    format!("cannot identify supervisor: {e}"),
                )
            })?;
            let started = store.start(&t.task_id, revision, &t.preflight_token, supervisor)?;
            let value = serde_json::to_value(&started)?;
            let job = Job {
                store,
                task_id: started.task_id,
                supervisor,
            };
            Ok((value, Some(job)))
        }
        Method::TaskCancel => {
            let t: TaskIdParams = parse_params(p)?;
            encode(serde_json::to_value(open()?.cancel(&t.task_id)?))
        }
        Method::TaskReview => {
            let t: TaskIdParams = parse_params(p)?;
            encode(serde_json::to_value(open()?.review(&t.task_id)?))
        }
        Method::TaskAcceptArtifact => {
            let a: TaskAcceptArtifactParams = parse_params(p)?;
            let request = AcceptRequest {
                request_id: &req.id,
                task_id: &a.task_id,
                review_digest: &a.review_digest,
                acknowledged: a.acknowledged,
                override_reason: a.override_reason.as_deref(),
            };
            encode(serde_json::to_value(open()?.accept_artifact(&request)?))
        }
        Method::TaskDiscard => {
            let d: TaskDiscardParams = parse_params(p)?;
            encode(serde_json::to_value(
                open()?.discard(&d.task_id, &d.review_digest)?,
            ))
        }
        Method::TaskRecover => {
            let _: EmptyParams = parse_params(p)?;
            encode(serde_json::to_value(open()?.recover()?))
        }
        Method::SettingsUpdate => {
            let s: SettingsUpdateParams = parse_params(p)?;
            let expected = to_i64(s.expected_revision, "expected_revision")?;
            let settings = Settings {
                decay_enabled: s.decay_enabled,
                decay_per_hour: s.decay_per_hour,
                timeout_seconds: s.timeout_seconds,
            };
            settings
                .validate()
                .map_err(|_| Failure::invalid_params("settings out of range"))?;
            encode(serde_json::to_value(
                open()?.update_settings(expected, &settings)?,
            ))
        }
        Method::PetPause => {
            let s: PetPauseParams = parse_params(p)?;
            encode(serde_json::to_value(open()?.set_paused(s.paused)?))
        }
    }
}

fn to_i64(v: u64, field: &str) -> Result<i64, Failure> {
    i64::try_from(v).map_err(|_| Failure::invalid_params(format!("{field} is out of range")))
}

/// Validates enum-valued strings with core's parsers; content checks stay in storage/core.
fn task_input(d: &DraftParams) -> Result<TaskInput, Failure> {
    let category =
        Category::parse(&d.category).ok_or_else(|| Failure::invalid_params("unknown category"))?;
    let provider =
        Provider::parse(&d.provider).ok_or_else(|| Failure::invalid_params("unknown provider"))?;
    let mock = match &d.mock {
        None => MockOptions::default(),
        Some(m) => MockOptions {
            outcome: match m.outcome.as_deref() {
                None => MockOutcome::default(),
                Some(o) => MockOutcome::parse(o).ok_or_else(|| {
                    Failure::invalid_params(
                        "mock.outcome must be artifact, provider_failure or check_failed",
                    )
                })?,
            },
            step_ms: m.step_ms.unwrap_or(0),
        },
    };
    Ok(TaskInput {
        draft: Draft {
            category,
            provider,
            folder: d.folder.clone(),
            scope: d.scope.clone(),
            instructions: d.instructions.clone(),
        },
        mock,
    })
}
