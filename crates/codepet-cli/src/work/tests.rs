//! Cross-boundary work-controller regression tests.
use super::filesystem::read_beneath;
use super::review::{check_destination, collect_changes, seal_scout};
use super::runtime::{
    base_sandbox, drain_after_exit, import_workspace, job_instruction, parse_event, permission_args,
};
use super::*;
use std::os::unix::fs::symlink;

fn assert_fifo_rejected_without_blocking(
    fifo: &Path,
    operation: impl FnOnce() -> Result<()> + Send + 'static,
) {
    let (send, receive) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || send.send(operation()).unwrap());
    let result = receive.recv_timeout(Duration::from_secs(2));
    if result.is_err() {
        // Release a regressed blocking reader so the test leaves no worker.
        let _release = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32)
            .open(fifo)
            .unwrap();
        receive
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap_err();
    }
    worker.join().unwrap();
    assert_eq!(
        result
            .expect("opening a project FIFO must not block")
            .unwrap_err(),
        "Expected a regular project file"
    );
}

#[test]
fn read_beneath_rejects_fifo_without_blocking() {
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("input");
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .unwrap();
    let root = dir.path().to_owned();
    assert_fifo_rejected_without_blocking(&fifo, move || {
        read_beneath(&root, Path::new("input"), MAX_FILE).map(|_| ())
    });
}

#[test]
fn copy_tree_rejects_regular_file_replaced_by_fifo_without_blocking() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let destination = dir.path().join("copy");
    fs::create_dir(&source).unwrap();
    let fifo = source.join("main.rs");
    fs::write(&fifo, "fn main() {}\n").unwrap();
    let baseline = walk(&source).unwrap();
    fs::remove_file(&fifo).unwrap();
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .unwrap();
    let target = destination.clone();
    assert_fifo_rejected_without_blocking(&fifo, move || copy_tree(&source, &target, &baseline));
    assert!(!destination.join("main.rs").exists());
}

#[test]
fn scope_preflight_shares_read_and_time_budgets() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    fs::write(first.path().join("a.rs"), "abc").unwrap();
    fs::write(second.path().join("b.rs"), "def").unwrap();
    let mut budget = ScopeBudget::new(true, Duration::from_secs(5));
    budget.bytes = MAX_WORKSPACE - 4;
    assert_eq!(walk_budgeted(first.path(), &mut budget).unwrap().len(), 1);
    assert!(walk_budgeted(second.path(), &mut budget)
        .unwrap_err()
        .contains("64 MiB"));
    assert_eq!(budget.bytes, MAX_WORKSPACE - 1);
    budget.started = Instant::now() - Duration::from_secs(6);
    assert!(walk_budgeted(second.path(), &mut budget).is_err());
}

#[test]
fn private_scout_errors_fit_saved_settings_and_never_enter_display() {
    let root = tempfile::tempdir().unwrap();
    let mut settings = ScoutSettings::default();
    for i in 0..200 {
        settings
            .attempts
            .insert(sha256_hex(i.to_string().as_bytes()), u64::MAX);
    }
    settings.last_error = "\u{0001}".repeat(1024);
    json_write(&root.path().join("scout-settings.json"), &settings).unwrap();
    assert_eq!(scout_settings(root.path()).unwrap().last_error.len(), 1024);
    let mut export = json!({"scout":{"last_error":"PRIVATE_PATH", "notice":"The last bug check could not start."}});
    redact_display(&mut export);
    assert!(!export.to_string().contains("PRIVATE_PATH"));
    assert!(export["scout"]["notice"].is_string());
}

#[test]
fn staging_excludes_credentials_and_refuses_links() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("main.py"), "print(1)\n").unwrap();
    fs::write(dir.path().join(".env"), "SECRET=yes").unwrap();
    fs::create_dir(dir.path().join("__pycache__")).unwrap();
    fs::write(dir.path().join("__pycache__/main.pyc"), [0, 1, 2]).unwrap();
    let files = walk(dir.path()).unwrap();
    assert_eq!(files.len(), 1);
    assert!(files.contains_key("main.py"));
    symlink("/etc/passwd", dir.path().join("linked")).unwrap();
    assert!(walk(dir.path()).unwrap_err().contains("links"));
}
#[test]
fn staging_refuses_hardlinked_source_before_reading_or_copying() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    private_dir(&source).unwrap();
    let outside = root.path().join("outside-secret");
    fs::write(&outside, "PRIVATE_OUTSIDE_CANARY").unwrap();
    fs::hard_link(&outside, source.join("main.rs")).unwrap();
    assert!(walk(&source).unwrap_err().contains("links"));
    assert!(read_beneath(&source, Path::new("main.rs"), MAX_FILE)
        .unwrap_err()
        .contains("hard-linked"));
}
#[test]
fn copy_tree_refuses_source_replaced_by_same_content_hardlink() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    private_dir(&source).unwrap();
    let input = source.join("main.rs");
    fs::write(&input, "fn main() {}\n").unwrap();
    let baseline = walk(&source).unwrap();
    let outside = root.path().join("outside");
    fs::write(&outside, "fn main() {}\n").unwrap();
    fs::remove_file(&input).unwrap();
    fs::hard_link(&outside, &input).unwrap();
    let destination = root.path().join("copy");
    assert!(copy_tree(&source, &destination, &baseline)
        .unwrap_err()
        .contains("hard-linked"));
    assert!(!destination.join("main.rs").exists());
}
#[test]
fn project_scope_refuses_state_storage_in_either_direction() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let project = home.join("Projects/repo");
    let sibling = home.join("Projects/other");
    let paths = StatePaths::resolve(|key| match key {
        "HOME" => Some(home.clone().into()),
        "XDG_STATE_HOME" => Some(project.join("state").into()),
        _ => None,
    })
    .unwrap();
    fs::create_dir_all(paths.state_dir.join("work")).unwrap();
    fs::create_dir_all(&sibling).unwrap();
    let private = paths.state_dir.join("private-task.json");
    fs::write(&private, "PRIVATE_WORK_SENTINEL").unwrap();

    for scope in [
        project.as_path(),
        paths.state_dir.as_path(),
        paths.state_dir.join("work").as_path(),
    ] {
        assert!(
            validate_project(&paths, scope).is_err(),
            "project scope must not overlap private state: {}",
            scope.display()
        );
    }
    assert_eq!(validate_project(&paths, &sibling).unwrap(), sibling);

    // XDG state ancestors may be symlinks even though the state directory is not.
    let alias = root.path().join("state-alias");
    symlink(project.join("state"), &alias).unwrap();
    let mut aliased_paths = paths.clone();
    aliased_paths.state_dir = alias.join("codepet");
    assert!(validate_project(&aliased_paths, &project).is_err());
    assert_eq!(validate_project(&aliased_paths, &sibling).unwrap(), sibling);
    aliased_paths.state_dir = root.path().join("missing-state");
    assert!(validate_project(&aliased_paths, &sibling).is_err());
    assert_eq!(
        fs::read_to_string(private).unwrap(),
        "PRIVATE_WORK_SENTINEL"
    );
}
#[test]
fn apply_conflicts_and_symlink_parents_preserve_original_files() {
    let dir = tempfile::tempdir().unwrap();
    let change = Change {
        path: "src/main.py".into(),
        before: Some("old".into()),
        after: Some("new".into()),
    };
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.py"), "edited").unwrap();
    assert!(check_destination(dir.path(), &change, false).is_err());
    assert_eq!(
        fs::read_to_string(dir.path().join("src/main.py")).unwrap(),
        "edited"
    );
    fs::write(dir.path().join("src/main.py"), "new").unwrap();
    assert!(check_destination(dir.path(), &change, true).is_ok());
    assert!(check_destination(dir.path(), &change, false).is_err());
    fs::rename(dir.path().join("src"), dir.path().join("real")).unwrap();
    symlink("real", dir.path().join("src")).unwrap();
    assert!(check_destination(dir.path(), &change, true).is_err());
}
#[test]
fn review_supports_a_small_status_append_alongside_a_source_fix() {
    let dir = tempfile::tempdir().unwrap();
    let paths = unused_paths(dir.path());
    let original = dir.path().join("original");
    private_dir(&original).unwrap();
    let status = "Fixture progress entry.\n".repeat(6_625);
    assert_eq!(status.len(), 159_000);
    let updated = format!("{status}Fixed arithmetic.\n");
    let old_code = "def add(a, b): return a - b\n";
    let new_code = "def add(a, b): return a + b\n";
    fs::write(original.join("STATUS.md"), &status).unwrap();
    fs::write(original.join("main.py"), old_code).unwrap();
    let initial = Store::open_with_first_run_placeholder(&paths, Arc::new(SystemClock))
        .unwrap()
        .snapshot()
        .unwrap()
        .life
        .unwrap();
    let mut task = task_fixture();
    task.source_kind = "scout_fix".into();
    task.life_id = initial.life_id;
    task.project = original.to_str().unwrap().into();
    task.output = "Checked the arithmetic.\n".repeat(MAX_OUTPUT / 24);
    task.baseline = walk(&original).unwrap();
    let job = dir.path().join(&task.id);
    private_dir(&job).unwrap();
    let base = job.join("base");
    let project = job.join("project");
    copy_tree(&original, &base, &task.baseline).unwrap();
    copy_tree(&base, &project, &task.baseline).unwrap();
    let candidate = dir.path().join("candidate");
    copy_tree(&base, &candidate, &task.baseline).unwrap();
    fs::write(candidate.join("STATUS.md"), &updated).unwrap();
    fs::write(candidate.join("main.py"), new_code).unwrap();
    let event = workspace_event(&base, &candidate, &task.baseline, "bug")
        .expect("a small source fix plus a 159 kB status file must be reviewable");
    let mut completed = true;
    let mut workspace = None;
    let mut failure = None;
    parse_event(
        &serde_json::to_vec(&event).unwrap(),
        &mut task,
        &mut completed,
        &mut workspace,
        &mut failure,
    );
    assert!(failure.is_none());
    import_workspace(dir.path(), &task, workspace.unwrap()).unwrap();
    seal_review(dir.path(), &mut task).unwrap();
    assert_eq!(task.status, "review");
    assert_eq!(
        task.changes.len(),
        2,
        "no changed file may disappear from review"
    );
    assert_eq!(task.changes[0].path, "STATUS.md");
    assert_eq!(task.changes[0].before.as_deref(), Some(status.as_str()));
    assert_eq!(task.changes[0].after.as_deref(), Some(updated.as_str()));
    assert_eq!(task.changes[1].path, "main.py");
    assert_eq!(
        fs::read_to_string(original.join("STATUS.md")).unwrap(),
        status
    );
    assert_eq!(
        fs::read_to_string(original.join("main.py")).unwrap(),
        old_code
    );
    assert_eq!(
        store(&paths).unwrap().snapshot().unwrap().life.unwrap().xp,
        0
    );
    let review = json!({"review_digest":task.review_digest,"acknowledge_unverified":true});
    assert!(accept(
        &paths,
        dir.path(),
        &mut task,
        &json!({"review_digest":"stale","acknowledge_unverified":true})
    )
    .is_err());
    fs::write(original.join("main.py"), "concurrent edit").unwrap();
    let conflict = accept(&paths, dir.path(), &mut task, &review).unwrap_err();
    assert!(conflict.contains("Conflict in main.py"), "{conflict}");
    assert_eq!(
        fs::read_to_string(original.join("STATUS.md")).unwrap(),
        status
    );
    fs::write(original.join("main.py"), old_code).unwrap();
    accept(&paths, dir.path(), &mut task, &review).unwrap();
    // Reopen the durable job before replaying acceptance.
    let mut reopened = load(dir.path(), &task.id).unwrap();
    accept(&paths, dir.path(), &mut reopened, &review).unwrap();
    assert_eq!(
        fs::read_to_string(original.join("STATUS.md")).unwrap(),
        updated
    );
    assert_eq!(
        fs::read_to_string(original.join("main.py")).unwrap(),
        new_code
    );
    let life = store(&paths).unwrap().snapshot().unwrap().life.unwrap();
    assert_eq!((life.xp, life.tasks_accepted), (20, 1));
    assert_eq!(life.adopted_at_ms, initial.adopted_at_ms);
}

#[test]
fn review_limits_refuse_raw_and_escaped_overflow_without_dropping_changes() {
    let dir = tempfile::tempdir().unwrap();
    let mut task = task_fixture();
    let job = dir.path().join(&task.id);
    private_dir(&job).unwrap();
    let base = job.join("base");
    let project = job.join("project");
    private_dir(&base).unwrap();
    private_dir(&project).unwrap();
    task.output = "Checks reported by provider".into();
    fs::write(project.join("large.txt"), "a".repeat(MAX_REVIEW + 1)).unwrap();
    let too_large = collect_changes(&base, &project, &task.baseline, "bug", 0).unwrap_err();
    assert!(too_large.contains("large.txt") && too_large.contains("384 KiB"));
    fs::write(project.join("large.txt"), "a".repeat(MAX_REVIEW)).unwrap();
    let combined = collect_changes(&base, &project, &task.baseline, "bug", 1).unwrap_err();
    assert!(combined.contains("large.txt") && combined.contains("384 KiB total"));
    assert!(seal_review(dir.path(), &mut task).is_err());
    assert!(task.review_digest.is_empty());
    // Escapes expand to six JSON bytes while the raw text stays below 384 KiB.
    fs::write(project.join("large.txt"), "\u{1}".repeat(90_000)).unwrap();
    let changes = collect_changes(&base, &project, &task.baseline, "bug", 0).unwrap();
    assert_eq!(changes.len(), 1);
    assert!(workspace_event(&base, &project, &task.baseline, "bug")
        .unwrap_err()
        .contains("512 KiB after JSON escaping"));
    assert!(seal_review(dir.path(), &mut task)
        .unwrap_err()
        .contains("512 KiB"));
    assert!(task.review_digest.is_empty());
    assert_ne!(task.status, "review");
    // Deletions and additions must both survive collection; binary output fails closed.
    fs::remove_file(project.join("large.txt")).unwrap();
    fs::write(base.join("deleted.txt"), "old").unwrap();
    fs::write(project.join("new.txt"), "new").unwrap();
    task.baseline = walk(&base).unwrap();
    let changes = collect_changes(&base, &project, &task.baseline, "bug", 0).unwrap();
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0].path, "deleted.txt");
    assert!(changes[0].after.is_none());
    assert_eq!(changes[1].path, "new.txt");
    assert!(changes[1].before.is_none());
    fs::write(project.join("new.txt"), [0, 1]).unwrap();
    assert!(workspace_event(&base, &project, &task.baseline, "bug")
        .unwrap_err()
        .contains("Binary changes"));
}

#[test]
fn export_errors_are_codepet_errors_and_transport_bounds_stay_independent() {
    let mut task = task_fixture();
    let instruction = job_instruction(&task);
    assert!(instruction.contains("384 KiB") && instruction.contains("512 KiB"));
    assert!(instruction.contains("64 KiB") && instruction.contains("both complete versions"));
    assert_eq!(MAX_EVENT, 1024 * 1024);
    assert_eq!(MAX_PROVIDER_OUTPUT, 2 * 1024 * 1024);
    let mut completed = true;
    let mut workspace = None;
    let mut failure = None;
    parse_event(
        &serde_json::to_vec(&json!({"type":"codepet.workspace_error",
        "message":"Reviewing large.txt exceeds 384 KiB"}))
        .unwrap(),
        &mut task,
        &mut completed,
        &mut workspace,
        &mut failure,
    );
    assert_eq!(
        failure.as_deref(),
        Some("CodePet could not export this result: Reviewing large.txt exceeds 384 KiB")
    );
    assert!(workspace.is_none());
    parse_event(
        &vec![b' '; MAX_EVENT + 1],
        &mut task,
        &mut completed,
        &mut workspace,
        &mut failure,
    );
    assert!(failure.unwrap().contains("oversized event"));
}

#[test]
fn text_review_includes_actual_change_and_rejects_binary_or_large_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("base");
    let project = dir.path().join("project");
    private_dir(&base).unwrap();
    private_dir(&project).unwrap();
    fs::write(base.join("x"), "old").unwrap();
    fs::write(project.join("x"), "new").unwrap();
    let baseline = walk(&base).unwrap();
    let changes = collect_changes(&base, &project, &baseline, "bug", 0).unwrap();
    assert_eq!(changes[0].before.as_deref(), Some("old"));
    assert_eq!(changes[0].after.as_deref(), Some("new"));
    assert!(collect_changes(&base, &project, &baseline, "research", 0).is_err());
    fs::write(project.join("x"), [0, 1, 2]).unwrap();
    assert!(collect_changes(&base, &project, &baseline, "bug", 0).is_err());
}
#[test]
fn sandbox_home_is_private_writable_and_independent_of_host_home_paths() {
    if !Path::new("/usr/bin/bwrap").exists() {
        return;
    }
    let status = base_sandbox()
        .args([
            "/bin/sh",
            "-c",
            "test \"$HOME\" = /pet-home && test ! -e /home && test -d \"$HOME\" && printf test > \"$HOME/probe\" && test \"$(cat \"$HOME/probe\")\" = test",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn namespace_kills_setsid_descendants_and_tmpfs_has_a_hard_size_limit() {
    if !Path::new("/usr/bin/bwrap").exists() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let probe = dir.path().join("ticks");
    let mut cmd = base_sandbox();
    cmd.arg("--bind")
        .arg(dir.path())
        .arg("/probe")
        .args([
            "/bin/sh",
            "-c",
            "setsid /bin/sh -c 'while :; do printf x >> /probe/ticks; sleep 0.03; done' & wait",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !probe.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        probe.exists(),
        "Bubblewrap containment must be available for this adapter"
    );
    child.kill().unwrap();
    child.wait().unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let bytes = fs::read(&probe).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(
        fs::read(&probe).unwrap(),
        bytes,
        "setsid descendant survived containment teardown"
    );
    let mut limited = base_sandbox();
    let status = limited
        .args([
            "--size",
            "1048576",
            "--tmpfs",
            "/probe",
            "/bin/sh",
            "-c",
            "dd if=/dev/zero of=/probe/full bs=1048576 count=2 status=none",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(!status.success());
}
fn task_fixture() -> Task {
    Task {
        id: "1".repeat(64),
        kind: "bug".into(),
        client_id: ClientId::Codex,
        adapter_version: legacy_adapter_version(),
        auth_route: AuthRoute::CodexChatgpt,
        project: "/project".into(),
        prompt: "Fix one bug".into(),
        status: "running".into(),
        progress: String::new(),
        output: String::new(),
        changes: vec![],
        review_digest: String::new(),
        error: String::new(),
        reward_xp: 20,
        reward_need: "hunger".into(),
        elapsed_ms: 0,
        acknowledged: false,
        source_kind: "manual".into(),
        source_task_id: None,
        finding: None,
        last_message: String::new(),
        life_id: "test-life".into(),
        baseline: BTreeMap::new(),
        cancelled: false,
        launched_at: 0,
    }
}
#[test]
fn review_messages_attribute_the_pinned_client() {
    for client in [ClientId::ClaudeCode, ClientId::OpenCode] {
        let root = tempfile::tempdir().unwrap();
        let mut task = task_fixture();
        task.client_id = client;
        let dir = root.path().join(&task.id);
        private_dir(&dir).unwrap();
        private_dir(&dir.join("base")).unwrap();
        fs::write(dir.join("base/main.py"), "return a - b\n").unwrap();
        task.baseline = walk(&dir.join("base")).unwrap();
        copy_tree(&dir.join("base"), &dir.join("project"), &task.baseline).unwrap();
        assert!(seal_review(root.path(), &mut task)
            .unwrap_err()
            .contains(client.name()));
        fs::write(dir.join("project/main.py"), "return a + b\n").unwrap();
        assert!(seal_review(root.path(), &mut task)
            .unwrap_err()
            .contains(client.name()));
        task.output = "One change; checks were not run.".into();
        seal_review(root.path(), &mut task).unwrap();
        assert_eq!(task.status, "review");
        assert!(task.progress.contains(client.name()));
        assert!(!task.progress.contains("Codex"));
    }
}
#[test]
fn cancellation_wins_over_stale_start_and_fast_completion_snapshots() {
    let root = tempfile::tempdir().unwrap();
    let initial = task_fixture();
    private_dir(&root.path().join(&initial.id)).unwrap();
    save(root.path(), &initial).unwrap();
    for state in ["preparing", "running"] {
        let mut stale = initial.clone();
        stale.status = state.into();
        let mut cancelled = stale.clone();
        cancelled.cancelled = true;
        save(root.path(), &cancelled).unwrap();
        merge_cancel(root.path(), &mut stale).unwrap();
        assert!(stale.cancelled, "cancel was lost at {state}");
        stale.status = "cancelled".into();
        save(root.path(), &stale).unwrap();
        assert_eq!(load(root.path(), &stale.id).unwrap().status, "cancelled");
    }
}
#[test]
fn final_drain_waits_for_delayed_workspace_export_and_eof() {
    struct DelayedRead<R>(R, bool);
    impl<R: Read> Read for DelayedRead<R> {
        fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
            if !self.1 {
                self.1 = true;
                std::thread::sleep(Duration::from_millis(200));
            }
            self.0.read(output)
        }
    }
    let bytes = b"{\"type\":\"codepet.workspace\",\"changes\":[]}\n".to_vec();
    let (tx, rx) = mpsc::sync_channel(16);
    runtime::reader(
        DelayedRead(std::io::Cursor::new(bytes.clone()), false),
        true,
        tx,
    );
    let mut pending = Vec::new();
    let mut total = 0;
    let mut failure = None;
    drain_after_exit(
        &rx,
        &mut pending,
        &mut total,
        &mut failure,
        Duration::from_secs(3),
    );
    assert!(failure.is_none());
    assert_eq!(pending, bytes);
    assert_eq!(total, bytes.len());
    let mut task = task_fixture();
    let mut completed = true;
    let mut workspace = None;
    parse_event(
        &pending,
        &mut task,
        &mut completed,
        &mut workspace,
        &mut failure,
    );
    assert!(failure.is_none());
    assert!(workspace.unwrap().is_empty());
}
#[test]
fn final_drain_refuses_unclosed_streams_and_preserves_output_budget() {
    let (held_sender, rx) = mpsc::sync_channel(16);
    let mut pending = Vec::new();
    let mut total = 0;
    let mut failure = None;
    drain_after_exit(
        &rx,
        &mut pending,
        &mut total,
        &mut failure,
        Duration::from_millis(20),
    );
    assert!(failure.unwrap().contains("streams did not close"));
    assert!(pending.is_empty());
    drop(held_sender);

    let (tx, rx) = mpsc::sync_channel(16);
    runtime::reader(
        std::io::Cursor::new(vec![b'x'; MAX_PROVIDER_OUTPUT + 1]),
        true,
        tx,
    );
    let mut failure = None;
    drain_after_exit(
        &rx,
        &mut pending,
        &mut total,
        &mut failure,
        Duration::from_secs(3),
    );
    assert!(failure.unwrap().contains("output limit"));
    assert!(pending.len() <= MAX_PROVIDER_OUTPUT);
}
#[test]
fn oversized_reports_fail_and_private_errors_never_enter_display_export() {
    let mut task = task_fixture();
    let mut completed = false;
    let mut workspace = None;
    let mut failure = None;
    let event = |text: String| {
        serde_json::to_vec(
            &json!({"type":"item.completed","item":{"type":"agent_message","text":text}}),
        )
        .unwrap()
    };
    parse_event(
        &event("a".repeat(MAX_OUTPUT + 1)),
        &mut task,
        &mut completed,
        &mut workspace,
        &mut failure,
    );
    assert!(failure.is_some());
    assert!(task.output.is_empty());
    failure = None;
    parse_event(
        &event("a".repeat(MAX_OUTPUT / 2)),
        &mut task,
        &mut completed,
        &mut workspace,
        &mut failure,
    );
    parse_event(
        &event("b".repeat(MAX_OUTPUT / 2)),
        &mut task,
        &mut completed,
        &mut workspace,
        &mut failure,
    );
    assert!(failure.is_some(), "cumulative report overflow must fail");
    parse_event(
        &serde_json::to_vec(&json!({"type":"error","message":"PRIVATE_SENTINEL".repeat(10000)}))
            .unwrap(),
        &mut task,
        &mut completed,
        &mut workspace,
        &mut failure,
    );
    assert!(failure.as_ref().unwrap().len() <= 4096);
    task.error = failure.unwrap();
    task.status = "failed".into();
    let mut result = json!({"task":task});
    redact_display(&mut result);
    assert!(!result.to_string().contains("PRIVATE_SENTINEL"));
    assert_eq!(result["task"]["status"], "failed");
}
#[test]
fn native_codex_tool_policy_denies_dummy_credentials_and_aliases() {
    let Some(codex) = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|p| p.join("codex"))
        .find(|p| p.is_file())
    else {
        return;
    };
    let codex = fs::canonicalize(codex).unwrap();
    let fixture = tempfile::tempdir().unwrap();
    let auth = fixture.path().join("fake-auth");
    fs::write(&auth, "NOT_A_REAL_CREDENTIAL").unwrap();
    let script="import os\npaths=['/auth/auth.json','/provider/auth.json','/proc/self/root/auth/auth.json','/proc/1/root/auth/auth.json','/work/auth-link']\nfor p in paths:\n try:\n  open(p).read(1); raise AssertionError('credential readable: '+p)\n except PermissionError: pass\nopen('/work/probe.txt','w').write('ok')\nprint('all credential reads denied; project writes work')";
    let mut cmd = base_sandbox();
    cmd.args([
        "--tmpfs",
        "/work",
        "--tmpfs",
        "/provider",
        "--symlink",
        "/auth/auth.json",
        "/work/auth-link",
    ])
    .arg("--ro-bind")
    .arg(&codex)
    .arg("/program/codex")
    .arg("--ro-bind")
    .arg(&auth)
    .arg("/auth/auth.json")
    .arg("--ro-bind")
    .arg(&auth)
    .arg("/provider/auth.json")
    .args(["--chdir", "/work", "/program/codex", "sandbox"])
    .args(permission_args("bug"))
    .args(["--", "/usr/bin/python", "-c", script]);
    let result = crate::install::runner::capture(cmd, Duration::from_secs(15), 4096, true).unwrap();
    assert_eq!(
        result.code,
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("all credential reads denied"));
}
#[test]
fn long_path_baselines_are_refused_before_claiming_a_slot() {
    let root = tempfile::tempdir().unwrap();
    let old = task_fixture();
    private_dir(&root.path().join(&old.id)).unwrap();
    save(root.path(), &old).unwrap();
    atomic(&root.path().join("active"), old.id.as_bytes()).unwrap();
    let long = "a".repeat(2000);
    let huge = (0..2050)
        .map(|n| {
            (
                format!("{long}/{n:05}"),
                Entry {
                    hash: "f".repeat(64),
                    mode: 0o644,
                },
            )
        })
        .collect();
    assert!(check_baseline_budget(&huge).is_err());
    assert_eq!(
        fs::read_to_string(root.path().join("active")).unwrap(),
        old.id
    );
    assert_eq!(load(root.path(), &old.id).unwrap().prompt, old.prompt);
    let mut oversized = old.clone();
    oversized.baseline = huge;
    assert!(save(root.path(), &oversized).is_err());
    assert_eq!(load(root.path(), &old.id).unwrap().prompt, old.prompt);
}

fn scout_fixture(root: &Path) -> Task {
    let mut t = task_fixture();
    t.kind = "scout".into();
    t.source_kind = "scout".into();
    t.reward_xp = 0;
    t.reward_need.clear();
    private_dir(&root.join(&t.id)).unwrap();
    private_dir(&root.join(&t.id).join("base")).unwrap();
    fs::write(root.join(&t.id).join("base/main.py"), "return a - b\n").unwrap();
    t.baseline = walk(&root.join(&t.id).join("base")).unwrap();
    copy_tree(
        &root.join(&t.id).join("base"),
        &root.join(&t.id).join("project"),
        &t.baseline,
    )
    .unwrap();
    t.last_message = json!({"finding":{"title":"Addition subtracts","path":"main.py","line":1,
        "evidence":"The addition function returns a - b.","check":"Assert add(2, 3) == 5.","verification":"provider_reported"}}).to_string();
    t
}
fn unused_paths(root: &Path) -> StatePaths {
    StatePaths::resolve(|key| match key {
        "HOME" => Some(root.join("home").into()),
        "XDG_STATE_HOME" => Some(root.join("state").into()),
        _ => None,
    })
    .unwrap()
}
#[test]
fn scout_uses_last_structured_message_and_never_accepts_or_exports_private_finding() {
    let root = tempfile::tempdir().unwrap();
    let mut t = scout_fixture(root.path());
    t.output = "Earlier plain-text commentary is not the final result".into();
    seal_review(root.path(), &mut t).unwrap();
    assert_eq!(t.status, "scouted");
    assert!(t.active());
    assert_eq!(t.reward_xp, 0);
    assert!(t.changes.is_empty());
    assert!(accept(&unused_paths(root.path()), root.path(), &mut t, &json!({})).is_err());
    let mut display = json!({"task":t});
    display["task"]["last_message"] = json!("PRIVATE-SCOUT-MESSAGE");
    display["task"]["source_task_id"] = json!("PRIVATE-SOURCE-BINDING");
    redact_display(&mut display);
    let text = display.to_string();
    assert!(!text.contains("Addition subtracts"));
    assert!(!text.contains("main.py"));
    assert!(!text.contains("PRIVATE-"));
    assert!(readonly("scout"));
    assert!(permission_args("scout").contains(&"permissions.codepet.extends=\":read-only\""));
    t.last_message = "{\"finding\":null}".into();
    seal_scout(root.path(), &mut t).unwrap();
    assert!(!t.active());
    assert!(t.finding.is_none());
}
#[test]
fn malformed_or_out_of_scope_scout_results_are_not_findings() {
    let root = tempfile::tempdir().unwrap();
    let mut t = scout_fixture(root.path());
    for invalid in ["{}", "not JSON", "{\"finding\":null,\"extra\":true}"] {
        t.last_message = invalid.into();
        assert!(seal_scout(root.path(), &mut t).is_err());
        assert!(t.finding.is_none());
    }
    let base = json!({"title":"suspected","path":"main.py","line":1,"evidence":"some evidence","check":"one check","verification":"provider_reported"});
    for (key, value) in [
        ("path", json!("../outside")),
        ("path", json!("missing.py")),
        ("line", json!(2)),
        ("verification", json!("verified")),
        ("title", json!("a".repeat(161))),
    ] {
        let mut finding = base.clone();
        finding[key] = value;
        t.last_message = json!({"finding":finding}).to_string();
        assert!(seal_scout(root.path(), &mut t).is_err());
        assert!(t.finding.is_none());
    }
    fs::write(root.path().join(&t.id).join("project/main.py"), "changed\n").unwrap();
    assert!(seal_review(root.path(), &mut t).is_err());
}
#[test]
fn historical_task_defaults_and_pinned_client_survive_selection_changes() {
    let root = tempfile::tempdir().unwrap();
    let original = task_fixture();
    let legacy = serde_json::to_value(&original).unwrap();
    assert!(legacy.get("client_id").is_none());
    let mut restored: Task = serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(restored.client_id, ClientId::Codex);
    assert_eq!(serde_json::to_value(&restored).unwrap(), legacy);
    restored.client_id = ClientId::ClaudeCode;
    restored.adapter_version = ClientId::ClaudeCode.version().into();
    restored.auth_route = AuthRoute::ClaudeSubscription;
    let pinned = serde_json::to_vec(&restored).unwrap();
    json_write(
        &root.path().join("client-settings.json"),
        &ClientSettings {
            client_id: ClientId::OpenCode,
        },
    )
    .unwrap();
    assert_eq!(selected_client(root.path()).unwrap(), ClientId::OpenCode);
    let reopened: Task = serde_json::from_slice(&pinned).unwrap();
    assert_eq!(reopened.client_id, ClientId::ClaudeCode);
    assert_eq!(reopened.auth_route, AuthRoute::ClaudeSubscription);
    assert_eq!(reopened.adapter_version, ClientId::ClaudeCode.version());
}
#[test]
fn unsafe_or_unknown_client_settings_never_fall_back() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(selected_client(root.path()).unwrap(), ClientId::Codex);
    let settings = root.path().join("client-settings.json");
    for bytes in [
        b"{}".as_slice(),
        b"{\"client_id\":\"t3\"}",
        b"{\"client_id\":\"codex\",\"other\":true}",
        &[b'x'; 1025],
    ] {
        fs::write(&settings, bytes).unwrap();
        assert!(selected_client(root.path()).is_err());
    }
    fs::remove_file(&settings).unwrap();
    std::os::unix::fs::symlink("absent", &settings).unwrap();
    assert!(selected_client(root.path()).is_err());
}
#[test]
fn fix_replay_binds_repo_and_finding_after_active_task_changes() {
    let root = tempfile::tempdir().unwrap();
    let mut scouting = scout_fixture(root.path());
    seal_scout(root.path(), &mut scouting).unwrap();
    save(root.path(), &scouting).unwrap();
    let request = "fix-request";
    let mut repair = task_fixture();
    repair.id = sha256_hex(request.as_bytes());
    repair.source_kind = "scout_fix".into();
    repair.source_task_id = Some(scouting.id.clone());
    let params = json!({"client_id":"codex","task_id":scouting.id,"review_digest":scouting.review_digest,"request_id":request});
    let paths = unused_paths(root.path());
    assert_eq!(
        fix(&paths, root.path(), &params, Some(&repair)).unwrap().id,
        repair.id
    );
    repair.client_id = ClientId::ClaudeCode;
    assert!(fix(&paths, root.path(), &params, Some(&repair)).is_err());
    repair.client_id = ClientId::Codex;
    scouting.project = "/another-project".into();
    save(root.path(), &scouting).unwrap();
    assert!(fix(&paths, root.path(), &params, Some(&repair)).is_err());
    assert!(!paths.state_dir.exists());
}
#[test]
fn automatic_source_scope_omits_large_art_but_export_still_refuses_oversized_output() {
    let root = tempfile::tempdir().unwrap();
    let original = root.path().join("original");
    private_dir(&original).unwrap();
    fs::write(original.join("main.py"), "return a - b\n").unwrap();
    let art = vec![0x89; MAX_FILE + 1];
    fs::write(original.join("portrait.PNG"), &art).unwrap();
    assert!(
        walk(&original).is_err(),
        "manual scope keeps its existing file limit"
    );
    let baseline = walk_scope(&original, true).unwrap();
    assert_eq!(baseline.keys().cloned().collect::<Vec<_>>(), ["main.py"]);
    let base = root.path().join("base");
    let project = root.path().join("project");
    copy_tree(&original, &base, &baseline).unwrap();
    copy_tree(&base, &project, &baseline).unwrap();
    fs::write(project.join("main.py"), "return a + b\n").unwrap();
    let changes = collect_changes(&base, &project, &baseline, "bug", 0).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].path, "main.py");
    assert_eq!(fs::read(original.join("portrait.PNG")).unwrap(), art);
    let replacement = Change {
        path: "portrait.PNG".into(),
        before: None,
        after: Some("replacement".into()),
    };
    assert!(check_destination(&original, &replacement, false).is_err());
    fs::write(project.join("unexpected.png"), &art).unwrap();
    assert!(collect_changes(&base, &project, &baseline, "bug", 0).is_err());
    assert_eq!(fs::read(original.join("portrait.PNG")).unwrap(), art);
}
