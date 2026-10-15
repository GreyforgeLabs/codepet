//! Shared helpers: every test gets its own temporary XDG_STATE_HOME and HOME and runs the
//! built `codepet` binary. Nothing touches the user's real state directory.
#![allow(dead_code)]
use codepet_storage::{Clock, ManualClock, StatePaths, Store, SystemClock};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

pub const BIN: &str = env!("CARGO_BIN_EXE_codepet");
pub const MIN: i64 = 60_000;
pub const HOUR: i64 = 3_600_000;
const SIGABRT: i32 = 6;

pub struct Env {
    dir: TempDir,
    next_id: AtomicU64,
}

/// One `codepet rpc` run: exit status, parsed stdout lines and stderr text.
#[derive(Debug)]
pub struct Reply {
    pub status: ExitStatus,
    pub lines: Vec<Value>,
    pub stdout_bytes: usize,
    pub stderr: String,
}
impl Reply {
    /// The single response frame.
    pub fn response(&self) -> &Value {
        assert_eq!(self.lines.len(), 1, "expected one frame: {self:?}");
        &self.lines[0]
    }
    pub fn aborted(&self) -> bool {
        self.status.signal() == Some(SIGABRT)
    }
}

impl Env {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("home")).unwrap();
        Self {
            dir,
            next_id: AtomicU64::new(1),
        }
    }
    pub fn xdg(&self) -> PathBuf {
        self.dir.path().join("state")
    }
    pub fn state_dir(&self) -> PathBuf {
        self.xdg().join("codepet")
    }
    pub fn home(&self) -> PathBuf {
        self.dir.path().join("home")
    }
    /// A project folder that is never created; the mock must never create or read it.
    pub fn folder(&self) -> PathBuf {
        self.dir.path().join("project-never-created")
    }
    pub fn id(&self) -> String {
        format!("t{}", self.next_id.fetch_add(1, Ordering::SeqCst))
    }

    /// `codepet <args>` with the test environment. A fault run goes through `sh` only to
    /// set `ulimit -c 0`, so injected aborts leave no core dumps even where the binary
    /// cannot mark itself non-dumpable (see `codepet_storage::fault`); `exec` keeps the
    /// status.
    pub fn command(&self, args: &[&str], fault: Option<&str>) -> Command {
        let mut c = match fault {
            None => Command::new(BIN),
            Some(_) => {
                let mut c = Command::new("/bin/sh");
                c.args(["-c", "ulimit -c 0 && exec \"$0\" \"$@\"", BIN]);
                c
            }
        };
        c.args(args)
            .env("XDG_STATE_HOME", self.xdg())
            .env("HOME", self.home())
            .env_remove("CODEPET_FAULT")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(f) = fault {
            c.env("CODEPET_FAULT", f);
        }
        c
    }

    pub fn run(&self, args: &[&str]) -> Output {
        let child = self
            .command(args, None)
            .stdin(Stdio::null())
            .spawn()
            .unwrap();
        wait_output(child)
    }

    /// Feeds raw bytes to `codepet rpc` and waits for it (and any supervisor) to exit.
    pub fn raw(&self, input: &[u8], fault: Option<&str>) -> Reply {
        self.raw_in(self.command(&["rpc"], fault), input)
    }
    /// One request with `HOME` removed from the environment.
    pub fn call_without_home(&self, method: &str, params: Value) -> Reply {
        let frame = json!({"v": 1, "id": self.id(), "method": method, "params": params});
        let mut c = self.command(&["rpc"], None);
        c.env_remove("HOME");
        self.raw_in(c, format!("{frame}\n").as_bytes())
    }
    fn raw_in(&self, mut command: Command, input: &[u8]) -> Reply {
        let mut child = command.spawn().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let data = input.to_vec();
        // A writer thread so a large frame never deadlocks against the reader.
        let writer = thread::spawn(move || {
            let _ = stdin.write_all(&data);
        });
        let out = wait_output(child);
        writer.join().unwrap();
        reply(out)
    }

    pub fn call_with(&self, id: &str, method: &str, params: Value, fault: Option<&str>) -> Reply {
        let frame = json!({"v": 1, "id": id, "method": method, "params": params}).to_string();
        self.raw(format!("{frame}\n").as_bytes(), fault)
    }
    pub fn call(&self, method: &str, params: Value) -> Reply {
        self.call_with(&self.id(), method, params, None)
    }

    /// Asserts success (exit 0, one ok frame echoing the id) and returns the result.
    pub fn ok_with(&self, id: &str, method: &str, params: Value) -> Value {
        let r = self.call_with(id, method, params, None);
        let resp = r.response().clone();
        assert_eq!(resp["ok"], true, "{method} failed: {resp}");
        assert_eq!(resp["id"], id);
        assert_eq!(resp["v"], 1);
        assert_eq!(r.status.code(), Some(0), "{r:?}");
        resp["result"].clone()
    }
    pub fn ok(&self, method: &str, params: Value) -> Value {
        self.ok_with(&self.id(), method, params)
    }

    /// Asserts a typed failure (exit 1, one error frame) and returns the error code.
    pub fn err_with(&self, id: &str, method: &str, params: Value) -> String {
        let r = self.call_with(id, method, params, None);
        error_code(&r)
    }
    pub fn err(&self, method: &str, params: Value) -> String {
        self.err_with(&self.id(), method, params)
    }

    pub fn draft(&self, category: &str, step_ms: u32, outcome: &str) -> Value {
        json!({
            "category": category,
            "provider": "mock",
            "folder": self.folder(),
            "scope": ["src"],
            "instructions": "Explain the mock state machine; do not edit files.",
            "mock": {"outcome": outcome, "step_ms": step_ms},
        })
    }
    pub fn create(&self, category: &str, step_ms: u32, outcome: &str) -> String {
        let t = self.ok("task.create", self.draft(category, step_ms, outcome));
        assert_eq!(t["state"], "draft");
        t["task_id"].as_str().unwrap().to_owned()
    }
    pub fn preflight(&self, task_id: &str, revision: u64) -> String {
        let p = self.ok(
            "task.preflight",
            json!({"task_id": task_id, "revision": revision}),
        );
        p["preflight_token"].as_str().unwrap().to_owned()
    }
    pub fn start_params(task_id: &str, revision: u64, token: &str) -> Value {
        json!({"task_id": task_id, "revision": revision, "preflight_token": token})
    }
    /// Starts a step_ms 0 task; the process returns after its supervisor finishes.
    pub fn start(&self, task_id: &str, revision: u64, token: &str) -> Value {
        self.ok("task.start", Self::start_params(task_id, revision, token))
    }
    /// Explicitly fill care before tests that exercise work rather than care policy.
    pub fn care_routine(&self) -> Value {
        self.ok("pet.care", json!({"action": "routine"}))
    }
    /// care → create → preflight → start; returns (task_id, review digest).
    pub fn reviewed(&self, category: &str, outcome: &str) -> (String, String) {
        self.care_routine();
        let id = self.create(category, 0, outcome);
        let token = self.preflight(&id, 1);
        let s = self.start(&id, 1, &token);
        assert_eq!(s["state"], "preparing");
        let t = self.task(&id);
        assert_eq!(t["state"], "review_ready", "{t}");
        (id, t["review"]["digest"].as_str().unwrap().to_owned())
    }
    pub fn state(&self) -> Value {
        self.ok("state.get", json!({}))
    }
    pub fn meter(&self, category: &str) -> u64 {
        self.state()["meters"][category].as_u64().unwrap()
    }
    /// The full task summary (any state).
    pub fn task(&self, task_id: &str) -> Value {
        self.ok("task.get", json!({"task_id": task_id}))
    }
    /// States recorded for a task by `task.state` events, oldest first.
    pub fn states(&self, task_id: &str) -> Vec<String> {
        let page = self.ok("events.subscribe", json!({"after_seq": 0, "limit": 100}));
        page["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["task_id"] == task_id)
            .map(|e| e["data"]["state"].as_str().unwrap().to_owned())
            .collect()
    }
    pub fn accept(&self, request_id: &str, task_id: &str, digest: &str, reason: &str) -> Reply {
        self.call_with(
            request_id,
            "task.accept_artifact",
            json!({
                "task_id": task_id,
                "review_digest": digest,
                "acknowledged": true,
                "override_reason": reason,
            }),
            None,
        )
    }
}

/// Lifecycle helpers. A `Store` opened here on a `ManualClock` stands in for time passing
/// between commands (no test-only clock ever reaches the CLI process); everything else goes
/// through the real binary.
impl Env {
    pub fn paths(&self) -> StatePaths {
        StatePaths::resolve(|key| match key {
            "XDG_STATE_HOME" => Some(self.xdg().into_os_string()),
            "HOME" => Some(self.home().into_os_string()),
            _ => None,
        })
        .unwrap()
    }
    /// A storage handle on the test state directory driven by `clock`.
    pub fn store(&self, clock: Arc<ManualClock>) -> Store {
        Store::open(&self.paths(), clock).unwrap()
    }
    /// Seeds an explicitly started nap whose wall-clock interval has already elapsed; the
    /// next CLI process settles it.
    pub fn elapsed_nap(&self) {
        let mut store = self.store(Arc::new(ManualClock::new(SystemClock.now_ms() - 120_001)));
        store.sleep(&self.id()).unwrap();
    }
    pub fn bar_state_path(&self) -> PathBuf {
        self.state_dir().join("bar-state.json")
    }
    /// The published `bar-state.json`, if any.
    pub fn bar_state(&self) -> Option<Value> {
        match std::fs::read(self.bar_state_path()) {
            Ok(bytes) => Some(serde_json::from_slice(&bytes).expect("bar-state.json is JSON")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => panic!("cannot read bar-state.json: {e}"),
        }
    }
    pub fn presence_path(&self) -> PathBuf {
        self.state_dir().join("presence.json")
    }
    /// Writes `presence.json` (mode 0600) as the bar leader would: `attended_ms` in total
    /// for `session`, last updated at `now_wall`, recorded since `started_wall`.
    pub fn write_presence(
        &self,
        session: &str,
        life_id: &str,
        started_wall: i64,
        now_wall: i64,
        attended_ms: i64,
    ) {
        let doc = json!({
            "v": 1, "session": session, "life_id": life_id,
            "started_wall_ms": started_wall, "updated_wall_ms": now_wall,
            "attended_ms": attended_ms, "ticks": (attended_ms + MIN - 1) / MIN,
        });
        self.write_presence_raw(&doc.to_string());
    }
    pub fn write_presence_raw(&self, text: &str) {
        let path = self.presence_path();
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, text).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    /// The living companion's id from a fresh `state.get`.
    pub fn life_id(&self) -> String {
        self.state()["life"]["life_id"]
            .as_str()
            .expect("a living companion")
            .to_owned()
    }
    /// Arms the current life in permadeath mode with the rules acknowledged.
    pub fn arm(&self) -> Value {
        let life_id = self.life_id();
        self.ok(
            "pet.rules",
            json!({"life_id": life_id, "mode": "permadeath", "acknowledge_rules": true}),
        )
    }
    /// Retires the living companion by its typed name.
    pub fn retire(&self) -> Value {
        let life = self.state()["life"].clone();
        self.ok(
            "pet.retire",
            json!({"life_id": life["life_id"], "confirm_name": life["name"]}),
        )
    }
    pub fn adopt(&self, species: &str, mode: &str, pace: &str) -> Value {
        self.ok(
            "pet.adopt",
            json!({"species": species, "mode": mode, "pace": pace, "acknowledge_rules": true}),
        )
    }
}

/// Drives attended time through one bar session, as the bar leader and storage would.
pub struct Timeline {
    pub clock: Arc<ManualClock>,
    pub store: Store,
    session: &'static str,
    life_id: String,
    started_wall: i64,
    attended: i64,
}
impl Timeline {
    /// Starts two days in the past, so the CLI's real clock is always later than any
    /// timestamp the timeline writes.
    pub fn new(env: &Env, session: &'static str) -> Self {
        let clock = Arc::new(ManualClock::new(SystemClock.now_ms() - 48 * HOUR));
        clock.set_steady("boot-timeline", 10_000_000);
        let store = env.store(clock.clone());
        let started_wall = clock.now_ms();
        Self {
            clock,
            store,
            session,
            life_id: env.life_id(),
            started_wall,
            attended: 0,
        }
    }
    /// `ms` of attended time pass; the next settlement credits them.
    pub fn attend(&mut self, env: &Env, ms: i64) {
        self.clock.advance(ms);
        self.attended += ms;
        env.write_presence(
            self.session,
            &self.life_id,
            self.started_wall,
            self.clock.now_ms(),
            self.attended,
        );
    }
    /// Attends up to `total` ms and settles through the timeline's store.
    pub fn settle_to(&mut self, env: &Env, total: i64) -> Value {
        self.attend(env, total - self.attended);
        serde_json::to_value(self.store.snapshot().unwrap()).unwrap()
    }
    /// Attends up to `total` ms, written with real timestamps, for a CLI process to settle.
    pub fn attend_for_cli(&mut self, env: &Env, total: i64) {
        self.attended = total;
        env.write_presence(
            self.session,
            &self.life_id,
            self.started_wall,
            SystemClock.now_ms(),
            total,
        );
    }
}

pub fn error_code(r: &Reply) -> String {
    let resp = r.response();
    assert_eq!(resp["ok"], false, "{resp}");
    assert_eq!(r.status.code(), Some(1), "{r:?}");
    let e = &resp["error"];
    assert!(
        e["message"].is_string() && e["retryable"].is_boolean(),
        "{resp}"
    );
    e["code"].as_str().unwrap().to_owned()
}

pub fn reply(out: Output) -> Reply {
    let text = String::from_utf8(out.stdout).expect("stdout is UTF-8");
    let lines = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|_| panic!("not JSON: {l:?}")))
        .collect();
    Reply {
        status: out.status,
        lines,
        stdout_bytes: text.len(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// Waits for exit with a hard bound so a hung child fails the test instead of hanging it.
/// Output is drained on threads so a full pipe never blocks the child.
pub fn wait_output(mut child: Child) -> Output {
    let drain = |r: Option<Box<dyn Read + Send>>| {
        thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut r) = r {
                let _ = r.read_to_end(&mut buf);
            }
            buf
        })
    };
    let stdout = drain(child.stdout.take().map(|r| Box::new(r) as _));
    let stderr = drain(child.stderr.take().map(|r| Box::new(r) as _));
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("codepet did not exit within 30 s");
        }
        thread::sleep(Duration::from_millis(5));
    };
    Output {
        status,
        stdout: stdout.join().unwrap(),
        stderr: stderr.join().unwrap(),
    }
}

/// Verify containment without relying on CONFIG_CHECKPOINT_RESTORE's optional
/// `/proc/PID/task/TID/children` interface, absent in some Linux test sandboxes.
pub fn assert_no_child_processes(pid: u32) {
    let tasks = std::fs::read_dir(format!("/proc/{pid}/task")).unwrap();
    let mut needs_parent_scan = false;
    for task in tasks {
        let path = task.unwrap().path().join("children");
        match std::fs::read_to_string(&path) {
            Ok(children) => assert!(children.trim().is_empty(), "{pid} has children: {children}"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => needs_parent_scan = true,
            Err(e) => panic!("cannot inspect {}: {e}", path.display()),
        }
    }
    if needs_parent_scan {
        for process in std::fs::read_dir("/proc").unwrap() {
            let process = process.unwrap();
            let Some(child_pid) = process
                .file_name()
                .to_str()
                .and_then(|n| n.parse::<u32>().ok())
            else {
                continue;
            };
            let status = match std::fs::read_to_string(process.path().join("status")) {
                Ok(status) => status,
                // Unrelated processes can exit between listing and inspection.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => panic!("cannot inspect process {child_pid}: {e}"),
            };
            let parent = status
                .lines()
                .find_map(|line| line.strip_prefix("PPid:"))
                .and_then(|value| value.trim().parse::<u32>().ok())
                .expect("Linux process status contains PPid");
            assert_ne!(
                parent, pid,
                "mock supervisor {pid} spawned child {child_pid}"
            );
        }
    }
}
