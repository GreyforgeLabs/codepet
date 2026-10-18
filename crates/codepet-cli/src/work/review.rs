//! Sealed review, conflict checks and explicit journaled application.
use super::*;

pub(super) fn collect_changes(
    base: &Path,
    project: &Path,
    baseline: &BTreeMap<String, Entry>,
    kind: &str,
    initial_size: usize,
) -> Result<Vec<Change>> {
    let staged = walk(project)?;
    let mut names: Vec<_> = baseline.keys().chain(staged.keys()).cloned().collect();
    names.sort();
    names.dedup();
    let mut changes = Vec::new();
    let mut size = initial_size;
    for name in names {
        if baseline.get(&name).map(|e| &e.hash) == staged.get(&name).map(|e| &e.hash) {
            continue;
        }
        if readonly(kind) {
            return Err("Research unexpectedly changed its read-only project copy".into());
        }
        let text = |folder: &Path, exists: bool| -> Result<Option<String>> {
            if !exists {
                return Ok(None);
            }
            let b = read_limit(&folder.join(&name), MAX_REVIEW).map_err(|e| {
                format!("Cannot review {name}: {e}. Full before/after text and the report must fit within {} KiB; start a smaller task.", MAX_REVIEW / 1024)
            })?;
            if b.contains(&0) {
                return Err(format!("Binary changes need a different workflow: {name}"));
            }
            Ok(Some(String::from_utf8(b).map_err(|_| {
                format!("Non-text changes need a different workflow: {name}")
            })?))
        };
        let before = text(base, baseline.contains_key(&name))?;
        let after = text(project, staged.contains_key(&name))?;
        size += before.as_ref().map_or(0, String::len) + after.as_ref().map_or(0, String::len);
        if size > MAX_REVIEW {
            return Err(format!("Reviewing {name} exceeds the {} KiB total for full before/after text and the report; start a smaller task. No changed files were omitted.", MAX_REVIEW / 1024));
        }
        if changes.len() >= 100 {
            return Err(format!("Reviewing {name} exceeds the 100 changed-file limit; start a smaller task. No changed files were omitted."));
        }
        changes.push(Change {
            path: name,
            before,
            after,
        });
    }
    Ok(changes)
}

pub(super) fn check_encoded_review(value: &Value) -> Result<()> {
    if serde_json::to_vec(value).map_err(io)?.len() > MAX_ENCODED_REVIEW {
        return Err(format!("The encoded review exceeds {} KiB after JSON escaping; start a smaller task. No changed files were omitted.", MAX_ENCODED_REVIEW / 1024));
    }
    Ok(())
}

pub(super) fn workspace_event(
    base: &Path,
    project: &Path,
    baseline: &BTreeMap<String, Entry>,
    kind: &str,
) -> Result<Value> {
    let changes = collect_changes(base, project, baseline, kind, 0)?;
    let event = json!({"type":"codepet.workspace","changes":changes});
    check_encoded_review(&event)?;
    Ok(event)
}

pub(super) fn seal_review(root: &Path, t: &mut Task) -> Result<()> {
    let dir = root.join(&t.id);
    let changes = collect_changes(
        &dir.join("base"),
        &dir.join("project"),
        &t.baseline,
        &t.kind,
        t.output.len(),
    )?;
    if t.kind == "scout" {
        return seal_scout(root, t);
    }
    if !readonly(&t.kind) && changes.is_empty() {
        return Err(format!("{} returned no code changes. No reward is available; review its explanation and start a narrower task.", t.client_id.name()));
    }
    if t.output.trim().is_empty() {
        return Err(format!(
            "{} returned no result to review",
            t.client_id.name()
        ));
    }
    t.changes = changes;
    check_encoded_review(
        &json!({"changes":t.changes,"output":t.output,"prompt":t.prompt,"project":t.project}),
    )?;
    t.review_digest =
        sha256_hex(&serde_json::to_vec(&(t.kind.as_str(), &t.output, &t.changes)).map_err(io)?);
    t.status = "review".into();
    t.progress = format!(
        "Ready for your review. Checks reported by {} are not independently verified.",
        t.client_id.name()
    );
    Ok(())
}

pub(super) fn seal_scout(root: &Path, t: &mut Task) -> Result<()> {
    let json: Value = serde_json::from_str(&t.last_message)
        .map_err(|_| "The scout did not return a valid structured finding")?;
    if json.get("finding").is_none() {
        return Err("The scout result is missing its finding field".into());
    }
    let report: ScoutReport = serde_json::from_value(json).map_err(io)?;
    if let Some(f) = &report.finding {
        relative(&f.path)?;
        if !t.baseline.contains_key(&f.path)
            || f.line == 0
            || f.line > 1_000_000
            || f.title.trim().is_empty()
            || f.title.len() > 160
            || f.evidence.trim().is_empty()
            || f.evidence.len() > 2048
            || f.check.trim().is_empty()
            || f.check.len() > 1024
            || f.verification != "provider_reported"
            || f.title.chars().any(char::is_control)
            || [&f.evidence, &f.check]
                .iter()
                .any(|s| s.chars().any(|c| c.is_control() && c != '\n' && c != '\t'))
        {
            return Err("The scout finding is outside its scope or display limits".into());
        }
        let bytes = read_limit(&root.join(&t.id).join("base").join(&f.path), MAX_FILE)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| "The scout selected a non-text file")?;
        if f.line as usize > text.lines().count().max(1) {
            return Err("The scout selected a line outside the staged file".into());
        }
    }
    t.finding = report.finding;
    t.review_digest = scout_digest(t)?;
    t.output = if t.finding.is_some() {
        "A suspected bug is ready to inspect. Evidence and checks are provider-reported, not independently verified. Choosing Fix starts an isolated repair; your files still require explicit review and Apply.".into()
    } else {
        "No sufficiently supported bug was found in this pass. This does not prove the repository is bug-free.".into()
    };
    t.last_message.clear();
    t.status = "scouted".into();
    t.progress = if t.finding.is_some() {
        "Found a suspected bug"
    } else {
        "Finished looking; no finding"
    }
    .into();
    Ok(())
}

pub(super) fn accept(p: &StatePaths, root: &Path, t: &mut Task, v: &Value) -> Result<()> {
    if t.kind == "scout" {
        return Err("Scouting earns no XP; choose Fix or dismiss the finding".into());
    }
    if t.status == "accepted" {
        return Ok(());
    }
    if !matches!(t.status.as_str(), "review" | "applying") {
        return Err("This job is not ready for review".into());
    }
    if str_param(v, "review_digest")? != t.review_digest
        || v.get("acknowledge_unverified").and_then(Value::as_bool) != Some(true)
    {
        return Err("Read the result and changes, then acknowledge that reported checks are not independently verified".into());
    }
    let digest =
        sha256_hex(&serde_json::to_vec(&(t.kind.as_str(), &t.output, &t.changes)).map_err(io)?);
    if digest != t.review_digest {
        return Err("The saved review changed; nothing was applied".into());
    }
    let current = store(p)?.snapshot().map_err(io)?;
    if !current.enabled {
        return Err("Turn CodePet on before accepting its reward".into());
    }
    if current.life.as_ref().map(|l| l.life_id.as_str()) != Some(&t.life_id) {
        return Err(
            "This result belongs to a different pet; it cannot reward your current companion"
                .into(),
        );
    }
    let project = Path::new(&t.project);
    if fs::canonicalize(project).map_err(io)? != project {
        return Err("The project folder changed; nothing was applied".into());
    }
    // Preflight every destination before the first write, then recheck each immediately
    // before its atomic replacement. External writers must remain stopped during Apply.
    for change in &t.changes {
        check_destination(project, change, t.status == "applying")?;
    }
    t.status = "applying".into();
    t.progress = "Applying your reviewed changes…".into();
    save(root, t)?;
    let dir = root.join(&t.id);
    json_write(
        &dir.join("apply-journal.json"),
        &json!({"project":t.project,"review_digest":t.review_digest,"changes":t.changes,"recovery":"Original bytes are in base/. Retry Accept to finish only when each destination matches its original or reviewed content."}),
    )?;
    for change in &t.changes {
        let destination = check_destination(project, change, true)?;
        if let Some(after) = &change.after {
            ensure_parents(project, &relative(&change.path)?)?;
            atomic(&destination, after.as_bytes())?;
            let mode = t.baseline.get(&change.path).map_or(0o644, |e| e.mode);
            fs::set_permissions(&destination, fs::Permissions::from_mode(mode)).map_err(io)?;
        } else if destination.exists() {
            fs::remove_file(&destination).map_err(io)?;
            File::open(destination.parent().ok_or("Missing parent")?)
                .map_err(io)?
                .sync_all()
                .map_err(io)?;
        }
    }
    let mut s = store(p)?;
    s.accept_work(&t.id, &t.life_id, &t.kind, t.elapsed_ms)
        .map_err(io)?;
    crate::rpc::publish_bar_state(&s);
    t.status = "accepted".into();
    t.progress = "Job accepted. Your pet earned 20 XP and its care reward!".into();
    save(root, t)
}
pub(super) fn ensure_parents(root: &Path, relative: &Path) -> Result<()> {
    let mut p = root.to_path_buf();
    for part in relative.parent().ok_or("Invalid path")?.components() {
        p.push(part);
        if !p.exists() {
            fs::create_dir(&p).map_err(io)?;
        }
        let m = fs::symlink_metadata(&p).map_err(io)?;
        if !m.is_dir() || m.file_type().is_symlink() {
            return Err("A destination parent became a link or file; application stopped".into());
        }
    }
    Ok(())
}
pub(super) fn check_destination(root: &Path, c: &Change, recovery: bool) -> Result<PathBuf> {
    let rel = relative(&c.path)?;
    let mut p = root.to_path_buf();
    for part in rel.parent().ok_or("Invalid path")?.components() {
        p.push(part);
        if let Ok(m) = fs::symlink_metadata(&p) {
            if !m.is_dir() || m.file_type().is_symlink() {
                return Err(format!("Destination changed: {}", c.path));
            }
        }
    }
    let p = root.join(rel);
    let current = match fs::symlink_metadata(&p) {
        Ok(m) => {
            if !m.is_file() || m.file_type().is_symlink() || m.nlink() > 1 {
                return Err(format!("Unsafe destination: {}", c.path));
            }
            Some(String::from_utf8(read_limit(&p, MAX_REVIEW)?).map_err(io)?)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(io(e)),
    };
    if current != c.before && !(recovery && current == c.after) {
        return Err(format!("Conflict in {}. Your edits were preserved. Restore the original or reviewed version before retrying; backups are kept with this job.",c.path));
    }
    Ok(p)
}
