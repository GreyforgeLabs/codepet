//! Bounded project traversal and private file operations.
use super::*;

pub(super) fn private_dir(p: &Path) -> Result<()> {
    if !p.exists() {
        fs::DirBuilder::new().mode(0o700).create(p).map_err(io)?;
    }
    let m = fs::symlink_metadata(p).map_err(io)?;
    if !m.is_dir()
        || m.file_type().is_symlink()
        || m.mode() & 0o077 != 0
        || m.uid() != rustix::process::geteuid().as_raw()
    {
        return Err("Work storage must be a private directory owned by you".into());
    }
    Ok(())
}
pub(super) fn checked_file(p: &Path) -> Result<File> {
    let f = OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(p)
        .map_err(io)?;
    let m = f.metadata().map_err(io)?;
    if !m.is_file() {
        return Err("Expected a regular file".into());
    }
    Ok(f)
}
pub(super) fn read_limit(p: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    checked_file(p)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() > limit {
        return Err("A file exceeds this job's size limit".into());
    }
    Ok(bytes)
}
pub(super) fn read_beneath(root: &Path, relative: &Path, limit: usize) -> Result<Vec<u8>> {
    let root = rustix::fs::openat2(
        rustix::fs::CWD,
        root,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::NOFOLLOW,
        rustix::fs::Mode::empty(),
        rustix::fs::ResolveFlags::NO_SYMLINKS,
    )
    .map_err(io)?;
    let fd = rustix::fs::openat2(
        &root,
        relative,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK,
        rustix::fs::Mode::empty(),
        rustix::fs::ResolveFlags::BENEATH
            | rustix::fs::ResolveFlags::NO_SYMLINKS
            | rustix::fs::ResolveFlags::NO_XDEV,
    )
    .map_err(io)?;
    let file = File::from(fd);
    if !file.metadata().map_err(io)?.is_file() {
        return Err("Expected a regular project file".into());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() > limit {
        return Err("A file exceeds this job's size limit".into());
    }
    Ok(bytes)
}
pub(super) fn atomic(p: &Path, bytes: &[u8]) -> Result<()> {
    let temp = p.with_extension(format!("new-{}-{}", std::process::id(), now()));
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .map_err(io)?;
    let result = (|| {
        f.write_all(bytes).map_err(io)?;
        f.sync_all().map_err(io)?;
        fs::rename(&temp, p).map_err(io)?;
        File::open(p.parent().ok_or("Missing parent")?)
            .map_err(io)?
            .sync_all()
            .map_err(io)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
pub(super) fn json_write(p: &Path, v: &impl Serialize) -> Result<()> {
    atomic(p, &serde_json::to_vec(v).map_err(io)?)
}
pub(super) fn excluded(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "node_modules"
            | "target"
            | "dist"
            | "build"
            | ".venv"
            | "venv"
            | ".codex"
            | ".claude"
            | ".opencode"
            | "opencode.json"
            | "opencode.jsonc"
            | ".ssh"
            | ".aws"
            | ".gnupg"
            | ".npmrc"
            | ".pypirc"
            | "credentials"
            | "auth.json"
            | "__pycache__"
            | ".pytest_cache"
            | ".mypy_cache"
            | ".ruff_cache"
    ) || name == ".env"
        || name.starts_with(".env.")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name.starts_with("id_rsa")
        || name.starts_with("id_ed25519")
}
pub(super) fn relative(s: &str) -> Result<PathBuf> {
    let p = PathBuf::from(s);
    if s.is_empty()
        || s.len() > 4096
        || s.chars().any(char::is_control)
        || p.components().any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("Unsupported project filename".into());
    }
    Ok(p)
}
pub(super) fn walk(root: &Path) -> Result<BTreeMap<String, Entry>> {
    walk_scope(root, false)
}
pub(super) fn media_asset(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| {
            matches!(
                s.to_ascii_lowercase().as_str(),
                "png"
                    | "jpg"
                    | "jpeg"
                    | "gif"
                    | "webp"
                    | "avif"
                    | "bmp"
                    | "ico"
                    | "icns"
                    | "tif"
                    | "tiff"
                    | "mp3"
                    | "wav"
                    | "ogg"
                    | "flac"
                    | "mp4"
                    | "webm"
                    | "mkv"
                    | "mov"
                    | "avi"
                    | "pdf"
                    | "zip"
                    | "gz"
                    | "bz2"
                    | "xz"
                    | "7z"
                    | "tar"
                    | "tgz"
                    | "rar"
                    | "zst"
                    | "woff"
                    | "woff2"
                    | "ttf"
                    | "otf"
            )
        })
}
pub(super) struct ScopeBudget {
    pub(super) bytes: usize,
    pub(super) entries: usize,
    pub(super) started: Instant,
    pub(super) limit: Duration,
    pub(super) omit_media: bool,
}
impl ScopeBudget {
    pub(super) fn new(omit_media: bool, limit: Duration) -> Self {
        Self {
            bytes: 0,
            entries: 0,
            started: Instant::now(),
            limit,
            omit_media,
        }
    }
    pub(super) fn exhausted(&self) -> bool {
        self.bytes >= MAX_WORKSPACE
            || self.entries >= 40_000
            || self.started.elapsed() >= self.limit
    }
}
pub(super) fn walk_scope(root: &Path, omit_media: bool) -> Result<BTreeMap<String, Entry>> {
    walk_budgeted(
        root,
        &mut ScopeBudget::new(omit_media, Duration::from_secs(30)),
    )
}
pub(super) fn walk_budgeted(
    root: &Path,
    budget: &mut ScopeBudget,
) -> Result<BTreeMap<String, Entry>> {
    fn visit(
        root: &Path,
        root_fd: &rustix::fd::OwnedFd,
        dir: &Path,
        out: &mut BTreeMap<String, Entry>,
        budget: &mut ScopeBudget,
    ) -> Result<()> {
        if dir.components().count() > 32 || budget.started.elapsed() >= budget.limit {
            return Err("Choose a shallower or smaller project scope".into());
        }
        let fd = rustix::fs::openat2(
            root_fd,
            dir,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::NOFOLLOW,
            rustix::fs::Mode::empty(),
            rustix::fs::ResolveFlags::BENEATH
                | rustix::fs::ResolveFlags::NO_SYMLINKS
                | rustix::fs::ResolveFlags::NO_XDEV,
        )
        .map_err(io)?;
        for entry in rustix::fs::Dir::read_from(&fd).map_err(io)? {
            budget.entries += 1;
            if budget.entries > 40_000 || budget.started.elapsed() >= budget.limit {
                return Err(
                    "Choose a smaller project scope (directory traversal limit reached)".into(),
                );
            }
            let entry = entry.map_err(io)?;
            let name = entry
                .file_name()
                .to_str()
                .map_err(|_| "Project filenames must be UTF-8")?;
            if name == "."
                || name == ".."
                || excluded(name)
                || (budget.omit_media && media_asset(name))
            {
                continue;
            }
            let path = if dir == Path::new(".") {
                PathBuf::from(name)
            } else {
                dir.join(name)
            };
            let rel = path.to_str().ok_or("Invalid filename")?.to_owned();
            relative(&rel)?;
            let entry_fd = rustix::fs::openat2(
                &fd,
                name,
                rustix::fs::OFlags::PATH | rustix::fs::OFlags::NOFOLLOW,
                rustix::fs::Mode::empty(),
                rustix::fs::ResolveFlags::BENEATH
                    | rustix::fs::ResolveFlags::NO_SYMLINKS
                    | rustix::fs::ResolveFlags::NO_XDEV,
            )
            .map_err(io)?;
            let m = rustix::fs::fstat(&entry_fd).map_err(io)?;
            let kind = rustix::fs::FileType::from_raw_mode(m.st_mode);
            if !matches!(
                kind,
                rustix::fs::FileType::RegularFile | rustix::fs::FileType::Directory
            ) {
                return Err(format!(
                    "Choose a scope without links or special files: {rel}"
                ));
            }
            if kind == rustix::fs::FileType::Directory {
                visit(root, root_fd, &path, out, budget)?;
            } else {
                if out.len() >= MAX_FILES {
                    return Err("Choose a smaller project folder (10,000 files maximum)".into());
                }
                if m.st_size < 0 || m.st_size as u64 > MAX_FILE as u64 {
                    return Err(format!("File exceeds the 2 MiB input limit: {rel}"));
                }
                let size = m.st_size as usize;
                if size > MAX_WORKSPACE.saturating_sub(budget.bytes) {
                    return Err("Choose a smaller project folder (64 MiB maximum)".into());
                }
                // Charge the bounded read even if a racing edit makes it fail.
                budget.bytes += size;
                let data = read_beneath(root, Path::new(&rel), size)?;
                if budget.bytes > MAX_WORKSPACE {
                    return Err("Choose a smaller project folder (64 MiB maximum)".into());
                }
                out.insert(
                    rel,
                    Entry {
                        hash: sha256_hex(&data),
                        mode: m.st_mode & 0o777,
                    },
                );
            }
        }
        Ok(())
    }
    let mut entries = BTreeMap::new();
    let root_fd = rustix::fs::openat2(
        rustix::fs::CWD,
        root,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::NOFOLLOW,
        rustix::fs::Mode::empty(),
        rustix::fs::ResolveFlags::NO_SYMLINKS,
    )
    .map_err(io)?;
    visit(root, &root_fd, Path::new("."), &mut entries, budget)?;
    Ok(entries)
}
pub(super) fn copy_tree(from: &Path, to: &Path, entries: &BTreeMap<String, Entry>) -> Result<()> {
    private_dir(to)?;
    for (name, e) in entries {
        let rel = relative(name)?;
        let bytes = read_beneath(from, &rel, MAX_FILE)?;
        if sha256_hex(&bytes) != e.hash {
            return Err(
                "The project changed while making the task copy; retry when editing has stopped"
                    .into(),
            );
        }
        let target = to.join(rel);
        fs::create_dir_all(target.parent().ok_or("Missing folder")?).map_err(io)?;
        atomic(&target, &bytes)?;
        fs::set_permissions(target, fs::Permissions::from_mode(e.mode & 0o700)).map_err(io)?;
    }
    Ok(())
}
