//! `codepet rpc`: one request frame in, one response frame out. Malformed or unimplemented
//! requests are refused before the state directory is touched.
//!
//! After every successful mutating method (including `state.get`, which may settle) the
//! command republishes `bar-state.json` from committed state before it answers. The export
//! takes the database writer lock itself and only ever moves the file forward; a failed
//! export is reported on one bounded stderr line and never changes the response or exit
//! code.
use crate::errors::Failure;
#[cfg(feature = "legacy-tasks")]
use crate::supervisor::{self, Job};
use codepet_core::lifecycle::{Mode, Pace};
use codepet_core::PROTOCOL_VERSION;
#[cfg(feature = "legacy-tasks")]
use codepet_core::{Category, Draft, Provider, Settings};
use codepet_protocol::{
    parse_params, parse_request, read_frame, AgentForgetHistoryParams, AppSetEnabledParams,
    AppSetHelpersParams, ClientsConnectParams, ClientsDisconnectParams, ClientsStatusParams,
    ClientsTestParams, EmptyParams, ErrorCode, HelloParams, LifecycleRulesParams, Method,
    PetAdoptParams, PetCareParams, PetConfigureLifeParams, PetEventsSeenParams, PetMemorialParams,
    PetPauseParams, PetRenameParams, PetRetireParams, PetRulesParams, PetSelectParams,
    PetStartParams, ProtocolError, Request, Response, TreatsGiveParams, V03_PENDING_MESSAGE,
};
#[cfg(feature = "legacy-tasks")]
use codepet_protocol::{
    DraftParams, EventsSubscribeParams, HistoryListParams, SettingsUpdateParams,
    TaskAcceptArtifactParams, TaskDiscardParams, TaskIdParams, TaskPreflightParams,
    TaskStartParams, TaskUpdateParams, NOT_IMPLEMENTED_MESSAGE,
};
use codepet_storage::{
    lifecycle_rules_json, AdoptRequest, Leniency, StartRequest, StatePaths, Store, SystemClock,
};
#[cfg(feature = "legacy-tasks")]
use codepet_storage::{
    AcceptRequest, MockOptions, MockOutcome, ProviderInfo, Supervisor, TaskInput,
};
use serde_json::{json, Value};
use std::io::{self, Write};
use std::sync::Arc;

type Outcome = Result<(Value, Option<Job>), Failure>;

/// The default build never continues as a task supervisor: no task can start (spec §6.5).
#[cfg(not(feature = "legacy-tasks"))]
pub enum Job {}

/// The store a started job took over.
fn job_store(job: &Job) -> &Store {
    #[cfg(feature = "legacy-tasks")]
    return &job.store;
    #[cfg(not(feature = "legacy-tasks"))]
    match *job {}
}

/// Bytes of an export failure quoted on stderr.
const MAX_EXPORT_ERROR_BYTES: usize = 200;

/// Republishes `bar-state.json` from committed state. Never fails the caller: an error
/// becomes one bounded, single-line stderr message. `Ok(false)` (the file already holds
/// this or a newer state) is silent.
pub fn publish_bar_state(store: &Store) {
    if let Err(e) = store.export_bar_state() {
        let line = export_error_line(&e.to_string());
        let mut err = io::stderr().lock();
        let _ = writeln!(err, "{line}").and_then(|()| err.flush());
    }
}

/// `codepet: bar-state.json not updated: <reason>`, on one line, the reason cut to
/// [`MAX_EXPORT_ERROR_BYTES`] at a character boundary.
pub fn export_error_line(reason: &str) -> String {
    let flat: String = reason
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let mut end = flat.len().min(MAX_EXPORT_ERROR_BYTES);
    while !flat.is_char_boundary(end) {
        end -= 1;
    }
    format!("codepet: bar-state.json not updated: {}", &flat[..end])
}

/// The storage handle of one request, opened only when a method needs it. Only
/// `state.get` may upgrade an older database (migration ownership, spec §6.2); every other
/// method answers `SCHEMA_UNSUPPORTED` ("needs `codepet migrate`") for one, untouched.
#[derive(Default)]
struct Ctx {
    store: Option<Store>,
    may_migrate: bool,
}
impl Ctx {
    fn store(&mut self) -> Result<&mut Store, Failure> {
        if self.store.is_none() {
            self.store = Some(if self.may_migrate {
                open()?
            } else {
                open_current()?
            });
        }
        Ok(self.store.as_mut().expect("store was just opened"))
    }
}

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
        #[cfg(feature = "legacy-tasks")]
        let _ = supervisor::run(job);
        #[cfg(not(feature = "legacy-tasks"))]
        match job {}
    }
    match (written, ok) {
        (Ok(()), true) => 0,
        _ => 1,
    }
}

/// `codepet state`: the `state.get` result, or a typed failure. A successful snapshot
/// (which may have settled presence, a nap or a due end) republishes `bar-state.json`.
/// It never migrates (spec §6.2: only `codepet migrate` and `rpc` `state.get` may); an
/// older database is refused untouched with `SCHEMA_UNSUPPORTED` ("needs codepet migrate").
pub fn state_get() -> Result<Value, Failure> {
    let store = open_current()?;
    let value = serde_json::to_value(store.snapshot()?)?;
    publish_bar_state(&store);
    Ok(value)
}

/// Parses and dispatches one frame (without its newline).
pub fn handle(frame: &str) -> (Response, Option<Job>) {
    let request = match parse_request(frame) {
        Ok(r) => r,
        Err(e) => return (e.to_response(), None),
    };
    let id = request.id.clone();
    let method = request.method;
    let mut ctx = Ctx {
        store: None,
        may_migrate: method == Method::StateGet,
    };
    match dispatch(request, &mut ctx) {
        Ok((result, job)) => {
            // Publish before answering, so a client that sees this response finds the
            // passive state file at least as new. A started task's store moved into its job.
            if method.is_mutating() {
                if let Some(store) = job.as_ref().map(job_store).or(ctx.store.as_ref()) {
                    publish_bar_state(store);
                }
            }
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
        Err(f) => {
            // A refused write may still have committed its settlement (a nap, presence, or
            // an end that became final): republish so the bar never keeps a stale state.
            if method.is_mutating() {
                if let Some(store) = ctx.store.as_ref() {
                    publish_bar_state(store);
                }
            }
            (f.into_response(Some(id)), None)
        }
    }
}

const COMMITTED_TOO_LARGE: &str = "The request may have been applied, but its result is too \
    large to return. Reload with state.get before retrying.";

/// Opens and, when older, migrates the database (`rpc` `state.get` only).
fn open() -> Result<Store, Failure> {
    let paths = StatePaths::from_env()?;
    Ok(Store::open(&paths, Arc::new(SystemClock))?)
}

/// Opens a current database; an older one is refused untouched (run `codepet migrate`).
pub(crate) fn open_current() -> Result<Store, Failure> {
    let paths = StatePaths::from_env()?;
    Ok(Store::open_current(&paths, Arc::new(SystemClock))?)
}

/// The v0.3 skeleton's refusal: params were valid, the body lands in a later work package.
/// Answered before the state directory is opened, so nothing is created or published.
fn pending() -> Outcome {
    Err(Failure::new(ErrorCode::NotImplemented, V03_PENDING_MESSAGE))
}

fn done(value: Value) -> Outcome {
    Ok((value, None))
}

fn encode(value: serde_json::Result<Value>) -> Outcome {
    done(value?)
}

fn dispatch(req: Request, ctx: &mut Ctx) -> Outcome {
    let p = req.params;
    match req.method {
        // The retired task pipeline (spec §6.5): unrouted in the default build, where these
        // names never parse; with `legacy-tasks` they keep their v0.2 behaviour.
        #[cfg(not(feature = "legacy-tasks"))]
        Method::EventsSubscribe
        | Method::ProvidersInspect
        | Method::TaskCreate
        | Method::TaskGet
        | Method::TaskUpdate
        | Method::TaskPreflight
        | Method::TaskStart
        | Method::TaskCancel
        | Method::TaskReview
        | Method::TaskLogs
        | Method::TaskApply
        | Method::TaskAcceptArtifact
        | Method::TaskDiscard
        | Method::TaskRollback
        | Method::TaskRecover
        | Method::HistoryList
        | Method::SettingsUpdate => Err(Failure::new(ErrorCode::UnknownMethod, "unknown method")),
        // Parsed but refused before any params or state are touched.
        #[cfg(feature = "legacy-tasks")]
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
            let last_seq = ctx.store()?.last_seq()?;
            // v0.3 retires the supervisor handshake with the task pipeline (spec §6.2).
            #[cfg(not(feature = "legacy-tasks"))]
            let hello = json!({
                "protocol_version": PROTOCOL_VERSION,
                "last_seq": last_seq,
            });
            #[cfg(feature = "legacy-tasks")]
            let hello = json!({
                "protocol_version": PROTOCOL_VERSION,
                "supervisor": null,
                "last_seq": last_seq,
            });
            done(hello)
        }
        Method::StateGet => {
            let _: EmptyParams = parse_params(p)?;
            encode(serde_json::to_value(ctx.store()?.snapshot()?))
        }
        #[cfg(feature = "legacy-tasks")]
        Method::ProvidersInspect => {
            let _: EmptyParams = parse_params(p)?;
            done(json!({ "providers": ProviderInfo::all() }))
        }
        #[cfg(feature = "legacy-tasks")]
        Method::EventsSubscribe => {
            let e: EventsSubscribeParams = parse_params(p)?;
            let after = to_i64(e.after_seq, "after_seq")?;
            encode(serde_json::to_value(
                ctx.store()?
                    .events_after(after, Some(e.limit_or_default()))?,
            ))
        }
        #[cfg(feature = "legacy-tasks")]
        Method::HistoryList => {
            let h: HistoryListParams = parse_params(p)?;
            encode(serde_json::to_value(ctx.store()?.history(
                h.before_ms,
                h.before_task_id.as_deref(),
                Some(h.limit_or_default()),
            )?))
        }
        #[cfg(feature = "legacy-tasks")]
        Method::TaskGet => {
            let t: TaskIdParams = parse_params(p)?;
            encode(serde_json::to_value(ctx.store()?.task(&t.task_id)?))
        }
        #[cfg(feature = "legacy-tasks")]
        Method::TaskCreate => {
            let d: DraftParams = parse_params(p)?;
            let input = task_input(&d)?;
            encode(serde_json::to_value(ctx.store()?.create_task(&input)?))
        }
        #[cfg(feature = "legacy-tasks")]
        Method::TaskUpdate => {
            let u: TaskUpdateParams = parse_params(p)?;
            let input = task_input(&u.draft())?;
            let expected = to_i64(u.expected_revision, "expected_revision")?;
            encode(serde_json::to_value(
                ctx.store()?.update_task(&u.task_id, expected, &input)?,
            ))
        }
        #[cfg(feature = "legacy-tasks")]
        Method::TaskPreflight => {
            let t: TaskPreflightParams = parse_params(p)?;
            let revision = to_i64(t.revision, "revision")?;
            encode(serde_json::to_value(
                ctx.store()?.preflight(&t.task_id, revision)?,
            ))
        }
        #[cfg(feature = "legacy-tasks")]
        Method::TaskStart => {
            let t: TaskStartParams = parse_params(p)?;
            let revision = to_i64(t.revision, "revision")?;
            let supervisor = Supervisor::current().map_err(|e| {
                Failure::new(
                    ErrorCode::Internal,
                    format!("cannot identify supervisor: {e}"),
                )
            })?;
            let started =
                ctx.store()?
                    .start(&t.task_id, revision, &t.preflight_token, supervisor)?;
            let value = serde_json::to_value(&started)?;
            let job = Job {
                store: ctx.store.take().expect("start opened the store"),
                task_id: started.task_id,
                supervisor,
            };
            Ok((value, Some(job)))
        }
        #[cfg(feature = "legacy-tasks")]
        Method::TaskCancel => {
            let t: TaskIdParams = parse_params(p)?;
            encode(serde_json::to_value(ctx.store()?.cancel(&t.task_id)?))
        }
        #[cfg(feature = "legacy-tasks")]
        Method::TaskReview => {
            let t: TaskIdParams = parse_params(p)?;
            encode(serde_json::to_value(ctx.store()?.review(&t.task_id)?))
        }
        #[cfg(feature = "legacy-tasks")]
        Method::TaskAcceptArtifact => {
            let a: TaskAcceptArtifactParams = parse_params(p)?;
            let request = AcceptRequest {
                request_id: &req.id,
                task_id: &a.task_id,
                review_digest: &a.review_digest,
                acknowledged: a.acknowledged,
                override_reason: a.override_reason.as_deref(),
            };
            encode(serde_json::to_value(
                ctx.store()?.accept_artifact(&request)?,
            ))
        }
        #[cfg(feature = "legacy-tasks")]
        Method::TaskDiscard => {
            let d: TaskDiscardParams = parse_params(p)?;
            encode(serde_json::to_value(
                ctx.store()?.discard(&d.task_id, &d.review_digest)?,
            ))
        }
        #[cfg(feature = "legacy-tasks")]
        Method::TaskRecover => {
            let _: EmptyParams = parse_params(p)?;
            encode(serde_json::to_value(ctx.store()?.recover()?))
        }
        #[cfg(feature = "legacy-tasks")]
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
                ctx.store()?.update_settings(expected, &settings)?,
            ))
        }
        Method::PetCare => {
            let c: PetCareParams = parse_params(p)?;
            encode(serde_json::to_value(ctx.store()?.care(&req.id, &c.action)?))
        }
        Method::PetSleep => {
            let _: EmptyParams = parse_params(p)?;
            encode(serde_json::to_value(ctx.store()?.sleep(&req.id)?))
        }
        Method::PetWake => {
            let _: EmptyParams = parse_params(p)?;
            encode(serde_json::to_value(ctx.store()?.wake(&req.id)?))
        }
        Method::PetPause => {
            let s: PetPauseParams = parse_params(p)?;
            encode(serde_json::to_value(ctx.store()?.set_paused(s.paused)?))
        }
        Method::PetSelect => {
            let s: PetSelectParams = parse_params(p)?;
            encode(serde_json::to_value(
                ctx.store()?
                    .select_appearance(&req.id, &s.life_id, &s.pet_id)?,
            ))
        }
        Method::PetAdopt => {
            let a: PetAdoptParams = parse_params(p)?;
            let request = AdoptRequest {
                species: &a.species,
                name: a.name.as_deref(),
                mode: mode(&a.mode)?,
                pace: Pace::parse(a.pace_or_default())
                    .ok_or_else(|| Failure::invalid_params("pace must be steady or playtest"))?,
                acknowledge_rules: a.acknowledge_rules,
            };
            encode(serde_json::to_value(ctx.store()?.adopt(&req.id, &request)?))
        }
        Method::PetRules => {
            let r: PetRulesParams = parse_params(p)?;
            let mode = mode(&r.mode)?;
            encode(serde_json::to_value(ctx.store()?.set_rules(
                &req.id,
                &r.life_id,
                mode,
                r.acknowledge_rules,
                r.confirm_name.as_deref(),
            )?))
        }
        Method::PetConfigureLife => {
            let c: PetConfigureLifeParams = parse_params(p)?;
            let expected = to_i64(c.expected_revision, "expected_revision")?;
            let leniency = Leniency {
                grace_hours: c.grace_hours,
                critical_after_hours: c.critical_after_hours,
                decay_percent: c.decay_percent,
            };
            encode(serde_json::to_value(
                ctx.store()?
                    .configure_life(&req.id, &c.life_id, expected, &leniency)?,
            ))
        }
        Method::PetRetire => {
            let r: PetRetireParams = parse_params(p)?;
            encode(serde_json::to_value(ctx.store()?.retire(
                &req.id,
                &r.life_id,
                &r.confirm_name,
            )?))
        }
        Method::PetEventsSeen => {
            let e: PetEventsSeenParams = parse_params(p)?;
            encode(serde_json::to_value(ctx.store()?.events_seen(
                &req.id,
                &e.life_id,
                e.through_seq,
            )?))
        }
        Method::PetMemorial => {
            let m: PetMemorialParams = parse_params(p)?;
            encode(serde_json::to_value(
                ctx.store()?
                    .memorial(m.before_ordinal, Some(m.limit_or_default()))?,
            ))
        }
        // Static ruleset text: answered without opening (or creating) the state directory.
        Method::LifecycleRules => {
            let _: LifecycleRulesParams = parse_params(p)?;
            encode(serde_json::from_str(&lifecycle_rules_json()))
        }
        // ---- v0.3 (spec §6.2). Each returns the `state.get` snapshot. ----
        Method::PetStart => {
            let s: PetStartParams = parse_params(p)?;
            let request = StartRequest {
                life_id: s.life_id.as_deref(),
                species: &s.species,
                name: s.name.as_deref(),
                mode: mode(&s.mode)?,
                acknowledge_rules: s.acknowledge_rules,
            };
            encode(serde_json::to_value(
                ctx.store()?.start_pet(&req.id, &request)?,
            ))
        }
        Method::PetRename => {
            let r: PetRenameParams = parse_params(p)?;
            encode(serde_json::to_value(
                ctx.store()?.rename(&req.id, &r.life_id, &r.name)?,
            ))
        }
        Method::TreatsGive => {
            let t: TreatsGiveParams = parse_params(p)?;
            encode(serde_json::to_value(
                ctx.store()?.give_treat(&req.id, &t.life_id)?,
            ))
        }
        Method::AppSetEnabled => {
            let e: AppSetEnabledParams = parse_params(p)?;
            encode(serde_json::to_value(
                ctx.store()?.set_enabled(&req.id, e.enabled)?,
            ))
        }
        Method::AppSetHelpers => {
            let e: AppSetHelpersParams = parse_params(p)?;
            encode(serde_json::to_value(
                ctx.store()?.set_helpers(&req.id, e.enabled)?,
            ))
        }
        Method::AgentForgetHistory => {
            let _: AgentForgetHistoryParams = parse_params(p)?;
            encode(serde_json::to_value(ctx.store()?.forget_history(&req.id)?))
        }
        // WP3: the clients methods. connect/disconnect refuse TEST_STATE under a test
        // state home; every job is detached and never holds the bar's RPC slot.
        Method::ClientsStatus => {
            let _: ClientsStatusParams = parse_params(p)?;
            pending()
        }
        Method::ClientsConnect => {
            let _: ClientsConnectParams = parse_params(p)?;
            pending()
        }
        Method::ClientsDisconnect => {
            let _: ClientsDisconnectParams = parse_params(p)?;
            pending()
        }
        Method::ClientsTest => {
            let _: ClientsTestParams = parse_params(p)?;
            pending()
        }
    }
}

/// Validates a life mode with core's parser (protocol already checked the closed set).
fn mode(value: &str) -> Result<Mode, Failure> {
    Mode::parse(value).ok_or_else(|| Failure::invalid_params("mode must be permadeath or gentle"))
}

fn to_i64(v: u64, field: &str) -> Result<i64, Failure> {
    i64::try_from(v).map_err(|_| Failure::invalid_params(format!("{field} is out of range")))
}

/// Validates enum-valued strings with core's parsers; content checks stay in storage/core.
#[cfg(feature = "legacy-tasks")]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_errors_are_one_bounded_line() {
        assert_eq!(
            export_error_line("Storage unavailable: disk full"),
            "codepet: bar-state.json not updated: Storage unavailable: disk full"
        );
        let long = format!("a\nb\r{}", "é".repeat(400));
        let line = export_error_line(&long);
        assert!(!line.contains('\n') && !line.contains('\r'), "{line}");
        let prefix = "codepet: bar-state.json not updated: ";
        assert!(line.starts_with(prefix));
        assert!(line.len() - prefix.len() <= MAX_EXPORT_ERROR_BYTES);
        assert!(line.len() - prefix.len() >= MAX_EXPORT_ERROR_BYTES - 1);
    }

    /// In-process tests never open storage (the process environment is the developer's own):
    /// the WP1 methods are exercised end to end by `tests/v03.rs` under a throwaway home.
    #[test]
    fn v03_methods_are_validated_before_storage() {
        let cases = [
            (
                "pet.start",
                r#"{"species":"cat","mode":"gentle","acknowledge_rules":true}"#,
            ),
            ("pet.rename", r#"{"life_id":"life-1","name":"Ginger"}"#),
            ("treats.give", r#"{"life_id":"life-1"}"#),
            ("app.set_enabled", r#"{"enabled":false}"#),
            ("app.set_helpers", r#"{"enabled":true}"#),
            ("agent.forget_history", "{}"),
            ("clients.status", "{}"),
            ("clients.connect", r#"{"clients":["claude-code"]}"#),
            ("clients.disconnect", r#"{"client":"codex"}"#),
            ("clients.test", "{}"),
        ];
        assert_eq!(cases.len(), Method::V03_ADDED.len());
        for (method, params) in cases {
            let m = Method::parse(method).unwrap();
            // Bad params are INVALID_PARAMS before anything is opened.
            let frame = format!(r#"{{"v":1,"id":"r","method":"{method}","params":{{"bogus":1}}}}"#);
            let (response, job) = handle(&frame);
            assert!(job.is_none());
            assert_eq!(
                response.error.expect(method).code,
                ErrorCode::InvalidParams,
                "{method}"
            );
            if !Method::V03_PENDING.contains(&m) {
                continue;
            }
            // The clients methods (WP3) still answer NOT_IMPLEMENTED without storage.
            let frame = format!(r#"{{"v":1,"id":"r","method":"{method}","params":{params}}}"#);
            let (response, job) = handle(&frame);
            assert!(job.is_none());
            let error = response.error.expect(method);
            assert_eq!(error.code, ErrorCode::NotImplemented, "{method}");
            assert!(!error.retryable);
            assert_eq!(response.id.as_deref(), Some("r"));
        }
    }

    #[test]
    fn lifecycle_rules_answer_without_storage() {
        let frame = r#"{"v":1,"id":"r","method":"lifecycle.rules","params":{}}"#;
        let (response, job) = handle(frame);
        assert!(job.is_none());
        assert!(response.ok, "{response:?}");
        let result = response.result.unwrap();
        assert_eq!(result.to_string(), lifecycle_rules_json());
        assert_eq!(result["id"], "lifecycle-v1");
    }
}
