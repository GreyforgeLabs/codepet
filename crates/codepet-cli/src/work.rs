//! Explicit useful work. The outer PID/mount namespace owns every descendant; a staged
//! copy is the only project visible inside it. No hooks, user configuration or MCPs load.
use codepet_storage::{sha256_hex, work_readiness, StatePaths, Store, SystemClock};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod client;
mod runtime;
use runtime::{append_bounded, qualify};
pub use runtime::{run, sandbox};
mod review;
use review::{accept, ensure_parents, seal_review, workspace_event};
mod filesystem;
use client::{AuthRoute, ClientId};
use filesystem::{
    atomic, checked_file, copy_tree, excluded, json_write, private_dir, read_limit, relative, walk,
    walk_budgeted, walk_scope, ScopeBudget,
};

const MAX_FILES: usize = 10_000;
const MAX_WORKSPACE: usize = 64 * 1024 * 1024;
const MAX_FILE: usize = 2 * 1024 * 1024;
// Full before/after text and the report share this budget. Transport bounds are
// independent: JSON escaping can make a small text result much larger on the wire.
const MAX_REVIEW: usize = 384 * 1024;
const MAX_ENCODED_REVIEW: usize = 512 * 1024;
const MAX_EVENT: usize = 1024 * 1024;
const MAX_PROVIDER_OUTPUT: usize = 2 * 1024 * 1024;
const MAX_OUTPUT: usize = 64 * 1024;
const MAX_MANIFEST: usize = 4 * 1024 * 1024;
const MAX_BASELINE: usize = 2 * 1024 * 1024;
const DEADLINE: Duration = Duration::from_secs(20 * 60);
const SCOUT_DEADLINE: Duration = Duration::from_secs(5 * 60);
const SCOUT_GLOBAL_MS: u64 = 6 * 60 * 60 * 1000;
const SCOUT_REPO_MS: u64 = 24 * 60 * 60 * 1000;
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    hash: String,
    mode: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "is_codex")]
    client_id: ClientId,
    #[serde(
        default = "legacy_adapter_version",
        skip_serializing_if = "is_legacy_adapter"
    )]
    adapter_version: String,
    #[serde(default, skip_serializing_if = "is_codex_route")]
    auth_route: AuthRoute,
    pub project: String,
    pub prompt: String,
    pub status: String,
    pub progress: String,
    pub output: String,
    pub changes: Vec<Change>,
    pub review_digest: String,
    pub error: String,
    pub reward_xp: u32,
    pub reward_need: String,
    pub elapsed_ms: i64,
    #[serde(default)]
    acknowledged: bool,
    #[serde(default = "manual_source")]
    pub source_kind: String,
    #[serde(default)]
    pub finding: Option<Finding>,
    #[serde(default)]
    source_task_id: Option<String>,
    #[serde(default)]
    last_message: String,
    life_id: String,
    baseline: BTreeMap<String, Entry>,
    cancelled: bool,
    launched_at: u64,
}
fn is_codex(id: &ClientId) -> bool {
    *id == ClientId::Codex
}
fn is_codex_route(route: &AuthRoute) -> bool {
    *route == AuthRoute::CodexChatgpt
}
fn legacy_adapter_version() -> String {
    "0.163.0".into()
}
fn is_legacy_adapter(version: &str) -> bool {
    version == "0.163.0"
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClientSettings {
    client_id: ClientId,
}
fn selected_client(root: &Path) -> Result<ClientId> {
    let path = root.join("client-settings.json");
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ClientId::Codex),
        Err(e) => Err(io(e)),
        Ok(_) => Ok(
            serde_json::from_slice::<ClientSettings>(&read_limit(&path, 1024)?)
                .map_err(io)?
                .client_id,
        ),
    }
}
fn expected_client(root: &Path, params: &Value) -> Result<ClientId> {
    let requested: ClientId = serde_json::from_value(
        params
            .get("client_id")
            .cloned()
            .ok_or("Choose a coding client before starting")?,
    )
    .map_err(|_| "Unknown coding client")?;
    if requested != selected_client(root)? {
        return Err("The selected client changed; review the client and try again".into());
    }
    Ok(requested)
}
fn manual_source() -> String {
    "manual".into()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub title: String,
    pub path: String,
    pub line: u32,
    pub evidence: String,
    pub check: String,
    pub verification: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScoutReport {
    finding: Option<Finding>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScoutSettings {
    enabled: bool,
    next_due_ms: u64,
    attempts: BTreeMap<String, u64>,
    #[serde(default)]
    last_error: String,
    #[serde(default)]
    last_error_acknowledged: bool,
}
impl Default for ScoutSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            next_due_ms: 0,
            attempts: BTreeMap::new(),
            last_error: String::new(),
            last_error_acknowledged: false,
        }
    }
}
fn scout_settings(root: &Path) -> Result<ScoutSettings> {
    let path = root.join("scout-settings.json");
    if !path.try_exists().map_err(io)? {
        return Ok(ScoutSettings::default());
    }
    let settings: ScoutSettings =
        serde_json::from_slice(&read_limit(&path, 32 * 1024)?).map_err(io)?;
    if settings.attempts.len() > 200
        || settings.attempts.keys().any(|id| !valid_id(id))
        || settings.last_error.len() > 1024
    {
        return Err("The saved scouting settings exceed their limits".into());
    }
    Ok(settings)
}
fn readonly(kind: &str) -> bool {
    matches!(kind, "research" | "scout")
}
impl Task {
    fn finished(&self) -> bool {
        matches!(
            self.status.as_str(),
            "accepted" | "cancelled" | "failed" | "discarded"
        ) || (self.status == "scouted" && self.finding.is_none())
    }
    fn active(&self) -> bool {
        matches!(
            self.status.as_str(),
            "preparing" | "running" | "review" | "applying"
        ) || (self.status == "scouted" && self.finding.is_some())
    }
    fn executing(&self) -> bool {
        matches!(self.status.as_str(), "preparing" | "running")
    }
}

fn io(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn paths() -> Result<(StatePaths, PathBuf)> {
    let p = StatePaths::from_env().map_err(io)?;
    p.prepare().map_err(io)?;
    let root = p.state_dir.join("work");
    private_dir(&root)?;
    Ok((p, root))
}
fn lock_at(path: &Path, nonblocking: bool) -> Result<File> {
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(path)
        .map_err(io)?;
    let m = f.metadata().map_err(io)?;
    if !m.is_file() || m.mode() & 0o077 != 0 || m.uid() != rustix::process::geteuid().as_raw() {
        return Err("Unsafe work lock".into());
    }
    rustix::fs::flock(
        &f,
        if nonblocking {
            rustix::fs::FlockOperation::NonBlockingLockExclusive
        } else {
            rustix::fs::FlockOperation::LockExclusive
        },
    )
    .map_err(|e| {
        if nonblocking && e == rustix::io::Errno::WOULDBLOCK {
            "CodePet is busy or updating; wait a moment and try again".into()
        } else {
            io(e)
        }
    })?;
    Ok(f)
}
fn valid_id(id: &str) -> bool {
    id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit())
}
fn load(root: &Path, id: &str) -> Result<Task> {
    if !valid_id(id) {
        return Err("Invalid task id".into());
    }
    let t: Task =
        serde_json::from_slice(&read_limit(&root.join(id).join("task.json"), MAX_MANIFEST)?)
            .map_err(io)?;
    if t.id != id {
        return Err("Task identity changed".into());
    }
    Ok(t)
}
fn save(root: &Path, t: &Task) -> Result<()> {
    let bytes = serde_json::to_vec(t).map_err(io)?;
    if bytes.len() > MAX_MANIFEST {
        return Err("The saved job would exceed its state size limit".into());
    }
    atomic(&root.join(&t.id).join("task.json"), &bytes)
}
fn check_baseline_budget(baseline: &BTreeMap<String, Entry>) -> Result<()> {
    // Leave room for prompt, report and the bounded encoded review in every later save.
    if serde_json::to_vec(baseline).map_err(io)?.len() > MAX_BASELINE {
        return Err(
            "Choose a smaller project scope: its filenames exceed the saved job size limit".into(),
        );
    }
    Ok(())
}
fn merge_cancel(root: &Path, task: &mut Task) -> Result<()> {
    task.cancelled |= load(root, &task.id)?.cancelled;
    Ok(())
}

fn active(root: &Path) -> Result<Option<Task>> {
    let p = root.join("active");
    if !p.exists() {
        return Ok(None);
    }
    let id = String::from_utf8(read_limit(&p, 64)?).map_err(io)?;
    let mut t = load(root, &id)?;
    // The worker owns this lock throughout execution. A crashed supervisor cannot leave
    // hidden work: bwrap's parent-death signal tears down its PID namespace.
    if t.executing() && now().saturating_sub(t.launched_at) > 5000 {
        if let Ok(_guard) = lock_at(&root.join(&id).join("worker.lock"), true) {
            let _ = fs::remove_dir_all(root.join(&id).join("provider"));
            t.status = "failed".into();
            t.error =
                "The job was interrupted. No changes were applied; start a new job when ready."
                    .into();
            t.progress = "Interrupted".into();
            save(root, &t)?;
        }
    }
    Ok(Some(t))
}
fn store(p: &StatePaths) -> Result<Store> {
    Store::open_current(p, Arc::new(SystemClock)).map_err(io)
}

/// Serializes Off/nap/retirement with Start. Callers keep this guard until the mutation
/// commits, closing the readiness-check versus launch race.
pub fn idle_guard() -> Result<File> {
    let (_, root) = paths()?;
    let guard = lock_at(&root.join("control.lock"), false)?;
    if active(&root)?.is_some_and(|t| t.executing() || t.status == "applying") {
        return Err("Cancel CodePet's current job before turning it off, changing its life, or starting a nap".into());
    }
    Ok(guard)
}

// Acknowledgement changes presentation only. Keep the original result and evidence,
// and bind the click to the exact terminal task or preparation attempt under control.lock.
fn dismiss(root: &Path, task: Option<&mut Task>, params: &Value) -> Result<()> {
    let params = params.as_object().ok_or("Expected a result to dismiss")?;
    if params.len() != 1 {
        return Err("Choose exactly one saved result to dismiss".into());
    }
    if let Some(id) = params.get("task_id").and_then(Value::as_str) {
        let task = task.ok_or("No current job")?;
        if id != task.id || !task.finished() {
            return Err(
                "Only this finished result can be dismissed; review or stop active work first"
                    .into(),
            );
        }
        if !task.acknowledged {
            task.acknowledged = true;
            save(root, task)?;
        }
    } else if let Some(due) = params.get("scout_due_ms").and_then(Value::as_u64) {
        if task.is_some_and(|task| !task.finished()) {
            return Err("Review or stop active work first".into());
        }
        let mut settings = scout_settings(root)?;
        if due != settings.next_due_ms || settings.last_error.is_empty() {
            return Err("This check notice is no longer current".into());
        }
        if !settings.last_error_acknowledged {
            settings.last_error_acknowledged = true;
            json_write(&root.join("scout-settings.json"), &settings)?;
        }
    } else {
        return Err("Expected a task identity or check attempt".into());
    }
    Ok(())
}

fn status(p: &StatePaths, root: &Path, task: Option<Task>, export: bool) -> Result<Value> {
    let mut database = store(p)?;
    if !task.as_ref().is_some_and(Task::active) {
        database.set_work_active(None).map_err(io)?;
    }
    let s = database.snapshot().map_err(io)?;
    let mut readiness = work_readiness(&s);
    if task.as_ref().is_some_and(Task::active) {
        readiness.ready = false;
        readiness.reason = if task.as_ref().is_some_and(|t| t.status == "scouted") {
            "Review or dismiss the suspected bug first"
        } else {
            "Finish or discard your current job first"
        }
        .into();
    }
    let selected = selected_client(root)?;
    let clients: Vec<Value> = client::CLIENTS
        .iter()
        .copied()
        .map(client::availability)
        .collect();
    let provider = clients
        .iter()
        .find(|entry| entry["id"] == json!(selected))
        .cloned()
        .ok_or("Unknown selected client")?;
    let scouting = scout_settings(root)?;
    let task = task.filter(|task| !(task.acknowledged && task.finished()));
    let notice = !scouting.last_error.is_empty() && !scouting.last_error_acknowledged;
    let mut result = json!({"readiness":readiness,"provider":provider,"clients":clients,"task":task,
        "scout":{"enabled":scouting.enabled,"next_due_ms":scouting.next_due_ms,
        "notice":if notice { "The last bug check could not start." } else { "" },
        "last_error":if notice { scouting.last_error } else { String::new() }}});
    if let Some(t) = result["task"].as_object_mut() {
        t.insert("started_at_ms".into(), t["launched_at"].clone());
        let id: ClientId =
            serde_json::from_value(t.get("client_id").cloned().unwrap_or(json!("codex")))
                .map_err(io)?;
        t.insert("client_id".into(), json!(id));
        t.insert("client_name".into(), json!(id.name()));
        t.insert("client_detail".into(), json!(client::detail(id)));
        t.remove("acknowledged");
        t.insert(
            "has_finding".into(),
            Value::Bool(t.get("finding").is_some_and(|f| !f.is_null())),
        );
        for k in [
            "baseline",
            "life_id",
            "cancelled",
            "launched_at",
            "source_task_id",
            "last_message",
        ] {
            t.remove(k);
        }
    }
    if export {
        export_status(p, &result)?;
    }
    Ok(result)
}
fn export_status(p: &StatePaths, full: &Value) -> Result<()> {
    let mut display = full.clone();
    redact_display(&mut display);
    json_write(&p.state_dir.join("work-state.json"), &display)
}
fn redact_display(result: &mut Value) {
    if let Some(scout) = result["scout"].as_object_mut() {
        scout.remove("last_error");
    }
    if let Some(t) = result["task"].as_object_mut() {
        for k in [
            "project",
            "prompt",
            "output",
            "changes",
            "baseline",
            "life_id",
            "review_digest",
            "error",
            "finding",
            "source_task_id",
            "last_message",
        ] {
            t.remove(k);
        }
    }
}

/// Best-effort refresh after ordinary care changes; never blocks a pet mutation on a
/// failed task export. Only a short command is active; there is no idle worker.
pub fn refresh_export() {
    if let Ok((p, root)) = paths() {
        if let Ok(_lock) = lock_at(&root.join("control.lock"), true) {
            if let Ok(task) = active(&root) {
                let _ = status(&p, &root, task, true);
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    method: String,
    #[serde(default)]
    params: Value,
}
fn str_param<'a>(p: &'a Value, k: &str) -> Result<&'a str> {
    p.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Missing {k}"))
}
fn output(v: &Value) {
    let _ = writeln!(std::io::stdout().lock(), "{v}");
}
pub fn rpc() -> i32 {
    let r: Result<Value> = (|| {
        let mut bytes = Vec::new();
        std::io::stdin()
            .take(24 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(io)?;
        if bytes.len() > 24 * 1024 {
            return Err("Request is too large".into());
        }
        let req: Request = serde_json::from_slice(&bytes).map_err(io)?;
        if req.method == "repos" {
            if !req.params.is_null() && !req.params.as_object().is_some_and(|p| p.is_empty()) {
                return Err("Repository discovery takes no parameters".into());
            }
            return crate::repos::discover().map_err(io);
        }
        let (p, root) = paths()?;
        // Never queue an old executable's request behind an installation/update.
        let _lock = lock_at(&root.join("control.lock"), true)?;
        let mut t = active(&root)?;
        if let Some(id) = req
            .params
            .get("task_id")
            .and_then(Value::as_str)
            .filter(|_| req.method != "fix")
        {
            if t.as_ref().map(|t| t.id.as_str()) != Some(id) {
                return Err("This job is no longer the current job".into());
            }
        }
        match req.method.as_str() {
            "status" | "get" => {}
            "client-select" => {
                let params = req.params.as_object().ok_or("Expected client selection")?;
                if params.len() != 1 {
                    return Err("Only client_id can be selected".into());
                }
                let client_id: ClientId = serde_json::from_value(
                    params
                        .get("client_id")
                        .cloned()
                        .ok_or("Choose a coding client")?,
                )
                .map_err(|_| "Unknown coding client")?;
                json_write(
                    &root.join("client-settings.json"),
                    &ClientSettings { client_id },
                )?;
            }
            "dismiss" => dismiss(&root, t.as_mut(), &req.params)?,
            "start" => {
                t = Some(start(
                    &p,
                    &root,
                    &req.params,
                    t.as_ref(),
                    "manual",
                    None,
                    None,
                )?);
            }
            "scout" => {
                if !req.params.is_null() && !req.params.as_object().is_some_and(|p| p.is_empty()) {
                    return Err("Automatic scouting takes no project or prompt".into());
                }
                t = Some(scout(&p, &root, t.as_ref())?);
            }
            "scout-settings" => {
                let params = req.params.as_object().ok_or("Expected scouting settings")?;
                if params.len() != 1 {
                    return Err("Only enabled can be changed".into());
                }
                let enabled = params
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .ok_or("Expected enabled boolean")?;
                let mut settings = scout_settings(&root)?;
                settings.enabled = enabled;
                json_write(&root.join("scout-settings.json"), &settings)?;
            }
            "fix" => {
                t = Some(fix(&p, &root, &req.params, t.as_ref())?);
            }
            "cancel" => {
                let job = t.as_mut().ok_or("No current job")?;
                if job.executing() {
                    job.cancelled = true;
                    job.progress = "Stopping the job safely…".into();
                    save(&root, job)?;
                }
            }
            "discard" => {
                let job = t.as_mut().ok_or("No current job")?;
                if job.executing() || job.status == "applying" {
                    return Err("Stop the job or finish recovering its application first".into());
                }
                if job.status != "accepted" {
                    job.status = "discarded".into();
                    job.progress = "Result discarded; your project was not changed".into();
                    save(&root, job)?;
                }
            }
            "accept" => {
                let job = t.as_mut().ok_or("No current job")?;
                accept(&p, &root, job, &req.params)?;
            }
            _ => return Err("Unknown work method".into()),
        }
        let full = status(&p, &root, t, true)?;
        Ok(full)
    })();
    match r {
        Ok(v) => {
            output(&json!({"ok":true,"result":v}));
            0
        }
        Err(e) => {
            output(&json!({"ok":false,"error":{"code":"WORK_REFUSED","message":e}}));
            1
        }
    }
}

fn scout(p: &StatePaths, root: &Path, previous: Option<&Task>) -> Result<Task> {
    let mut settings = scout_settings(root)?;
    if !settings.enabled {
        return Err("Automatic bug finding is paused".into());
    }
    if previous.is_some_and(Task::active) {
        return Err("Finish or dismiss the current result first".into());
    }
    let snapshot = store(p)?.snapshot().map_err(io)?;
    if snapshot.life.as_ref().is_some_and(|life| !life.armed) {
        return Err("Choose Let's go before your pet starts finding bugs".into());
    }
    let ready = work_readiness(&snapshot);
    if !ready.ready {
        return Err(ready.reason);
    }
    let at = now();
    if at < settings.next_due_ms {
        return Err("Your pet is waiting before looking for another bug".into());
    }
    let mut candidates = crate::repos::candidates()?;
    candidates.sort_by(|a, b| {
        b.recent_at_ms
            .min(at)
            .cmp(&a.recent_at_ms.min(at))
            .then(a.path.cmp(&b.path))
    });
    // One discovery/preparation attempt; skipped checkouts are not provider retries.
    settings.next_due_ms = at.saturating_add(SCOUT_GLOBAL_MS);
    settings.last_error.clear();
    settings.last_error_acknowledged = false;
    json_write(&root.join("scout-settings.json"), &settings)?;
    let result = (|| {
        let (project, baseline) = choose_scout_scope(p, root, &mut settings, &candidates, at)?;
        let key = sha256_hex(project.as_bytes());
        start(
            p,
            root,
            &json!({"client_id":selected_client(root)?,"request_id":format!("scout:{at}:{key}"),"kind":"scout","project":project,
            "prompt":"Inspect the staged source for at most one concrete, useful bug. Known binary media, art, fonts and archive files were omitted; this is not complete repository coverage. Do not modify files. Prefer a small defect with specific evidence; report no finding when unsure.","network":true}),
            previous,
            "scout",
            None,
            Some(baseline),
        )
    })();
    if let Err(e) = &result {
        if settings.last_error.is_empty() {
            append_bounded(&mut settings.last_error, e, 1024);
        }
    }
    json_write(&root.join("scout-settings.json"), &settings)?;
    status(
        p,
        root,
        result.as_ref().ok().cloned().or_else(|| previous.cloned()),
        true,
    )?;
    result
}

fn choose_scout_scope(
    p: &StatePaths,
    root: &Path,
    settings: &mut ScoutSettings,
    candidates: &[crate::repos::Repository],
    at: u64,
) -> Result<(String, BTreeMap<String, Entry>)> {
    let mut budget = ScopeBudget::new(true, Duration::from_secs(5));
    let mut checked = 0;
    for repo in candidates {
        let key = sha256_hex(repo.path.as_bytes());
        if settings
            .attempts
            .get(&key)
            .is_some_and(|last| at < last.saturating_add(SCOUT_REPO_MS))
        {
            continue;
        }
        if checked == 10 || budget.exhausted() {
            break;
        }
        checked += 1;
        if !settings.attempts.contains_key(&key) && settings.attempts.len() == 200 {
            let oldest = settings
                .attempts
                .iter()
                .min_by_key(|(key, at)| (**at, *key))
                .map(|(key, _)| key.clone())
                .unwrap();
            settings.attempts.remove(&oldest);
        }
        settings.attempts.insert(key, at);
        // Persist before inspecting source, including checkouts that cannot be staged.
        json_write(&root.join("scout-settings.json"), settings)?;
        let prepared: Result<BTreeMap<String, Entry>> = (|| {
            let project = validate_project(p, Path::new(&repo.path))?;
            let baseline = walk_budgeted(&project, &mut budget)?;
            check_baseline_budget(&baseline)?;
            Ok(baseline)
        })();
        match prepared {
            Ok(baseline) => {
                settings.last_error.clear();
                return Ok((repo.path.clone(), baseline));
            }
            Err(e) => {
                settings.last_error.clear();
                append_bounded(
                    &mut settings.last_error,
                    &format!("Could not prepare {}: {e}", repo.path),
                    1024,
                );
            }
        }
    }
    if checked == 0 {
        settings.next_due_ms = candidates
            .iter()
            .filter_map(|repo| {
                settings
                    .attempts
                    .get(&sha256_hex(repo.path.as_bytes()))
                    .map(|last| last.saturating_add(SCOUT_REPO_MS))
            })
            .min()
            .unwrap_or(settings.next_due_ms);
    }
    Err("No eligible local repository could be prepared within this check's limits. Your pet will check again later.".into())
}

fn scout_digest(t: &Task) -> Result<String> {
    Ok(sha256_hex(
        &serde_json::to_vec(&(&t.id, &t.project, &t.finding)).map_err(io)?,
    ))
}
fn fix(p: &StatePaths, root: &Path, v: &Value, previous: Option<&Task>) -> Result<Task> {
    if v.as_object().is_none_or(|p| p.len() != 4) {
        return Err("Fix requires a task, digest, request id and client".into());
    }
    let client_id = expected_client(root, v)?;
    let source = str_param(v, "task_id")?;
    let digest = str_param(v, "review_digest")?;
    let request = str_param(v, "request_id")?;
    if request.is_empty() || request.len() > 128 {
        return Err("Invalid request id".into());
    }
    let current = previous.ok_or("No suspected bug is available")?;
    let replay = current.source_kind == "scout_fix"
        && current.source_task_id.as_deref() == Some(source)
        && current.id == sha256_hex(request.as_bytes());
    if current.id != source && !replay {
        return Err("This finding is no longer current".into());
    }
    let scouting = load(root, source)?;
    if scouting.kind != "scout"
        || scouting.status != "scouted"
        || scouting.finding.is_none()
        || scouting.review_digest != digest
        || scout_digest(&scouting)? != digest
    {
        return Err("The suspected bug changed; reopen it before choosing Fix".into());
    }
    if replay {
        if current.client_id != client_id {
            return Err("That request id belongs to another client".into());
        }
        return Ok(current.clone());
    }
    if store(p)?
        .snapshot()
        .map_err(io)?
        .life
        .as_ref()
        .map(|l| &l.life_id)
        != Some(&scouting.life_id)
    {
        return Err("This finding belongs to a different companion".into());
    }
    let finding = scouting.finding.as_ref().unwrap();
    let prompt = format!("Investigate this suspected bug first. Fix it only if the evidence holds, make the smallest change, and run available relevant checks. Known binary media, art, fonts and archives were omitted from this source copy; do not recreate or change omitted assets.\nTitle: {}\nLocation: {}:{}\nEvidence (provider-reported, unverified): {}\nSuggested check: {}",finding.title,finding.path,finding.line,finding.evidence,finding.check);
    start(
        p,
        root,
        &json!({"client_id":client_id,"request_id":request,"kind":"bug","project":scouting.project,"prompt":prompt,"network":true}),
        previous,
        "scout_fix",
        Some(source),
        None,
    )
}

fn validate_project(p: &StatePaths, project: &Path) -> Result<PathBuf> {
    if !project.is_absolute()
        || project
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err("Choose an absolute project folder".into());
    }
    let canonical = fs::canonicalize(project).map_err(io)?;
    let state_dir = fs::canonicalize(&p.state_dir).map_err(io)?;
    if canonical != project
        || !canonical.is_dir()
        || canonical.components().count() < 4
        || p.homes()
            .any(|h| canonical == *h || h.starts_with(&canonical))
        || canonical.starts_with(&state_dir)
        || state_dir.starts_with(&canonical)
        || fs::metadata(&canonical).map_err(io)?.uid() != rustix::process::geteuid().as_raw()
        || p.homes().any(|h| {
            [".codex", ".ssh", ".aws", ".gnupg", ".config", ".local"]
                .iter()
                .any(|n| canonical.starts_with(h.join(n)))
        })
    {
        return Err("Choose a specific project folder, not your home, system folders, state storage, or a symbolic link".into());
    }
    Ok(canonical)
}

fn start(
    p: &StatePaths,
    root: &Path,
    v: &Value,
    previous: Option<&Task>,
    source_kind: &str,
    source_task_id: Option<&str>,
    prepared: Option<BTreeMap<String, Entry>>,
) -> Result<Task> {
    let client_id = expected_client(root, v)?;
    let request = str_param(v, "request_id")?;
    if request.is_empty() || request.len() > 128 {
        return Err("Invalid request id".into());
    }
    let id = sha256_hex(request.as_bytes());
    if root.join(&id).exists() {
        if previous.map(|t| t.id.as_str()) != Some(id.as_str()) {
            return Err("This request belongs to an older job; use a new request id".into());
        }
        let old = load(root, &id)?;
        if old.client_id != client_id
            || old.kind != str_param(v, "kind")?
            || old.project != str_param(v, "project")?
            || old.prompt != str_param(v, "prompt")?
            || old.source_kind != source_kind
            || old.source_task_id.as_deref() != source_task_id
        {
            return Err("That request id already belongs to another task".into());
        }
        return Ok(old);
    }
    if previous.is_some_and(Task::active)
        && !(source_kind == "scout_fix" && previous.map(|t| t.id.as_str()) == source_task_id)
    {
        return Err("Finish or discard the current job first".into());
    }
    let s = store(p)?.snapshot().map_err(io)?;
    let ready = work_readiness(&s);
    if !ready.ready {
        return Err(ready.reason);
    }
    if v.get("network").and_then(Value::as_bool) != Some(true) {
        return Err("Starting requires acknowledgement that the selected client sends project context to its provider and can access the network".into());
    }
    let kind = str_param(v, "kind")?;
    let need = match kind {
        "bug" => "hunger",
        "research" => "thirst",
        "build" => "mental_health",
        "scout" if source_kind == "scout" => "",
        _ => return Err("Choose Fix a bug, Research, or Build something".into()),
    };
    let prompt = str_param(v, "prompt")?.trim();
    if prompt.is_empty() || prompt.len() > 8000 {
        return Err("Describe one job in 1–8,000 characters".into());
    }
    let canonical = validate_project(p, Path::new(str_param(v, "project")?))?;
    let resolved = client::resolve(client_id)?;
    qualify(client_id, &resolved.binary)?;
    let baseline = match prepared {
        Some(baseline) => baseline,
        None => walk_scope(&canonical, matches!(source_kind, "scout" | "scout_fix"))?,
    };
    check_baseline_budget(&baseline)?;
    let dir = root.join(&id);
    private_dir(&dir)?;
    let staged = (|| {
        copy_tree(&canonical, &dir.join("base"), &baseline)?;
        copy_tree(&dir.join("base"), &dir.join("project"), &baseline)
    })();
    if let Err(e) = staged {
        return Err(format!("Could not prepare the isolated copy: {e}"));
    }
    let task = Task {
        id,
        kind: kind.into(),
        client_id,
        adapter_version: client_id.version().into(),
        auth_route: resolved.route,
        project: canonical.to_string_lossy().into_owned(),
        prompt: prompt.into(),
        status: "preparing".into(),
        progress: "Preparing an isolated workspace…".into(),
        output: String::new(),
        changes: vec![],
        review_digest: String::new(),
        error: String::new(),
        reward_xp: if kind == "scout" { 0 } else { 20 },
        reward_need: need.into(),
        elapsed_ms: 0,
        acknowledged: false,
        source_kind: source_kind.into(),
        source_task_id: source_task_id.map(str::to_owned),
        finding: None,
        last_message: String::new(),
        life_id: s.life.ok_or("Choose a pet first")?.life_id,
        baseline,
        cancelled: false,
        launched_at: now(),
    };
    save(root, &task)?;
    atomic(&root.join("active"), task.id.as_bytes())?;
    store(p)?.set_work_active(Some(&task.id)).map_err(io)?;
    let mut child = Command::new("/usr/bin/systemd-run")
        .args([
            "--user",
            "--scope",
            "--collect",
            "--quiet",
            "--property=MemoryMax=1073741824",
            "--property=MemorySwapMax=0",
            "--property=TasksMax=128",
        ])
        .arg(if kind == "scout" {
            "--property=RuntimeMaxSec=350"
        } else {
            "--property=RuntimeMaxSec=1250"
        })
        .arg(format!("--unit=codepet-work-{}", task.id))
        .arg(std::env::current_exe().map_err(io)?)
        .arg("work-run")
        .arg(&task.id)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(io)?;
    // Reap if it already refused; otherwise the independent worker owns its lifetime.
    let _ = child.try_wait();
    Ok(task)
}

#[cfg(test)]
mod tests;
