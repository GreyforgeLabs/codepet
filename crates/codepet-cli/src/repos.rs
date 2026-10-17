//! On-demand repository metadata discovery. No Git process, source read or persistence.
use rustix::fd::OwnedFd;
use rustix::fs::{self as rfs, Dir, FileType, Mode, OFlags, ResolveFlags};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeSet, VecDeque};
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

const ROOTS: &[&str] = &[
    "Projects",
    "Work/workspaces",
    "Code",
    "code",
    "src",
    "repos",
    "Developer",
];
const MAX_PATH: usize = 4096;

#[derive(Clone, Copy)]
struct Limits {
    depth: usize,
    entries: usize,
    results: usize,
    bytes: usize,
    time: Duration,
}
const LIMITS: Limits = Limits {
    depth: 4,
    entries: 12_000,
    results: 200,
    bytes: 128 * 1024,
    time: Duration::from_secs(2),
};

#[derive(Debug, Serialize)]
pub(crate) struct Repository {
    pub name: String,
    pub path: String,
    kind: &'static str,
    #[serde(skip)]
    pub recent_at_ms: u64,
}
#[derive(Debug, Default, Serialize)]
struct Discovery {
    repos: Vec<Repository>,
    scanned_roots: Vec<String>,
    skipped_directories: usize,
    incomplete: bool,
    limit_reason: &'static str,
}
impl Discovery {
    fn limited(&mut self, reason: &'static str) {
        self.incomplete = true;
        self.limit_reason = reason;
    }
}

fn directory(home: &OwnedFd, path: &Path) -> rustix::io::Result<OwnedFd> {
    rfs::openat2(
        home,
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
        ResolveFlags::BENEATH | ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_XDEV,
    )
}

fn marker_kind(directory: &OwnedFd) -> Option<&'static str> {
    let flags = ResolveFlags::BENEATH | ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_XDEV;
    // O_PATH inspects the marker before opening it for reading, so FIFOs/devices are skipped.
    let marker = rfs::openat2(
        directory,
        ".git",
        OFlags::PATH | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
        flags,
    )
    .ok()?;
    let metadata = rfs::fstat(&marker).ok()?;
    match FileType::from_raw_mode(metadata.st_mode) {
        FileType::Directory => Some("repository"),
        FileType::RegularFile if metadata.st_size <= MAX_PATH as i64 && metadata.st_nlink == 1 => {
            let marker = rfs::openat2(
                directory,
                ".git",
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
                flags,
            )
            .ok()?;
            let opened = rfs::fstat(&marker).ok()?;
            if opened.st_ino != metadata.st_ino || opened.st_dev != metadata.st_dev {
                return None;
            }
            let mut text = String::new();
            File::from(marker)
                .take(MAX_PATH as u64 + 1)
                .read_to_string(&mut text)
                .ok()?;
            if text.len() > MAX_PATH {
                return None;
            }
            let target = text
                .trim_end_matches(['\r', '\n'])
                .strip_prefix("gitdir: ")?;
            if target.is_empty() || target.chars().any(char::is_control) {
                return None;
            }
            // A gitfile also represents submodules. Do not follow its target or imply eligibility.
            Some("gitfile")
        }
        _ => None,
    }
}

fn recent_metadata(directory: &OwnedFd, modified_seconds: i64) -> u64 {
    let mut recent = modified_seconds.max(0) as u64;
    for path in [".git/index", ".git/logs/HEAD"] {
        if let Ok(fd) = rfs::openat2(
            directory,
            path,
            OFlags::PATH | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
            ResolveFlags::BENEATH | ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_XDEV,
        ) {
            if let Ok(stat) = rfs::fstat(&fd) {
                if FileType::from_raw_mode(stat.st_mode) == FileType::RegularFile {
                    recent = recent.max(stat.st_mtime.max(0) as u64);
                }
            }
        }
    }
    recent.saturating_mul(1000)
}

fn excluded(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name,
            "node_modules"
                | "target"
                | "dist"
                | "build"
                | "vendor"
                | "venv"
                | "__pycache__"
                | "cache"
                | "caches"
                | "proc"
                | "sys"
                | "dev"
                | "run"
                | "tmp"
                | "etc"
                | "mnt"
                | "media"
                | "lost+found"
        )
}

fn scan(home_path: &Path, limits: Limits) -> Result<Discovery, String> {
    if !home_path.is_absolute()
        || home_path == Path::new("/")
        || home_path
            .components()
            .any(|c| !matches!(c, Component::RootDir | Component::Normal(_)))
        || home_path
            .to_str()
            .is_none_or(|p| p.len() > MAX_PATH - 32 || p.chars().any(char::is_control))
    {
        return Err("Repository discovery needs a normal absolute home directory".into());
    }
    let started = Instant::now();
    let home = rfs::openat2(
        rfs::CWD,
        home_path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
        ResolveFlags::NO_SYMLINKS,
    )
    .map_err(|_| "Repository discovery could not safely open your home directory")?;
    let mut result = Discovery::default();
    let mut queue = VecDeque::new();
    for root in ROOTS {
        let relative = PathBuf::from(root);
        if directory(&home, &relative).is_ok() {
            result
                .scanned_roots
                .push(home_path.join(&relative).to_str().unwrap().to_owned());
            queue.push_back((relative, 0));
        }
    }
    let mut seen = BTreeSet::new();
    let mut entries = 0;
    'scan: while let Some((relative, depth)) = queue.pop_front() {
        if started.elapsed() >= limits.time {
            result.limited("time");
            break;
        }
        let Ok(fd) = directory(&home, &relative) else {
            result.skipped_directories += 1;
            continue;
        };
        let stat = rfs::fstat(&fd).map_err(|e| e.to_string())?;
        if !seen.insert((stat.st_dev, stat.st_ino)) {
            continue;
        }
        if let Some(kind) = marker_kind(&fd) {
            if result.repos.len() == limits.results {
                result.limited("results");
                break;
            }
            result.repos.push(Repository {
                name: relative.file_name().unwrap().to_str().unwrap().to_owned(),
                path: home_path.join(&relative).to_str().unwrap().to_owned(),
                kind,
                recent_at_ms: recent_metadata(&fd, stat.st_mtime),
            });
            if serde_json::to_vec(&result)
                .map_err(|e| e.to_string())?
                .len()
                + 64
                > limits.bytes
            {
                result.repos.pop();
                result.limited("response_size");
                break;
            }
            // A discovered checkout is a boundary, not a reason to inspect its source tree.
            continue;
        }
        if depth == limits.depth {
            continue;
        }
        let Ok(directory) = Dir::read_from(&fd) else {
            result.skipped_directories += 1;
            continue;
        };
        let mut children = Vec::new();
        for entry in directory {
            if started.elapsed() >= limits.time {
                result.limited("time");
                break 'scan;
            }
            if entries == limits.entries {
                result.limited("entries");
                break 'scan;
            }
            entries += 1;
            let Ok(entry) = entry else {
                result.skipped_directories += 1;
                continue;
            };
            let Ok(name) = entry.file_name().to_str() else {
                continue;
            };
            if excluded(name)
                || name.chars().any(char::is_control)
                || !matches!(entry.file_type(), FileType::Directory | FileType::Unknown)
            {
                continue;
            }
            let child = relative.join(name);
            if home_path.join(&child).as_os_str().len() <= MAX_PATH {
                children.push(child);
            }
        }
        children.sort();
        queue.extend(children.into_iter().map(|path| (path, depth + 1)));
    }
    result
        .repos
        .sort_by_cached_key(|repo| (repo.name.to_lowercase(), repo.path.clone()));
    if serde_json::to_vec(&result)
        .map_err(|e| e.to_string())?
        .len()
        > limits.bytes
    {
        return Err("Repository discovery response exceeds its size limit".into());
    }
    Ok(result)
}

pub fn discover() -> Result<Value, String> {
    let home = std::env::var_os("HOME").ok_or("No home directory is configured")?;
    serde_json::to_value(scan(Path::new(&home), LIMITS)?).map_err(|e| e.to_string())
}

pub(crate) fn candidates() -> Result<Vec<Repository>, String> {
    let home = std::env::var_os("HOME").ok_or("No home directory is configured")?;
    Ok(scan(Path::new(&home), LIMITS)?.repos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;

    fn repository(home: &Path, relative: &str) {
        fs::create_dir_all(home.join(relative).join(".git")).unwrap();
    }

    #[test]
    fn finds_sorted_checkouts_and_gitfiles_without_following_their_targets() {
        let home = tempfile::tempdir().unwrap();
        repository(home.path(), "Projects/Zebra");
        repository(home.path(), "Projects/group/alpha");
        repository(home.path(), "Work/workspaces/alpha");
        fs::create_dir_all(home.path().join("src/linked")).unwrap();
        fs::write(
            home.path().join("src/linked/.git"),
            "gitdir: /unavailable/private/gitdir\n",
        )
        .unwrap();
        let found = scan(home.path(), LIMITS).unwrap();
        assert_eq!(
            found
                .repos
                .iter()
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>(),
            ["alpha", "alpha", "linked", "Zebra"]
        );
        assert_eq!(found.repos[2].kind, "gitfile");
        assert!(!found.incomplete);
        assert_eq!(found.scanned_roots.len(), 3);
    }

    #[test]
    fn skips_links_caches_deep_paths_and_contents_of_discovered_repositories() {
        let home = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        repository(home.path(), "Projects/real");
        repository(home.path(), "Projects/real/nested");
        repository(home.path(), "Projects/node_modules/ignored");
        repository(home.path(), "Projects/.hidden/ignored");
        repository(home.path(), "Projects/a/b/c/d/too-deep");
        repository(other.path(), "outside");
        symlink(other.path(), home.path().join("Projects/link")).unwrap();
        symlink(home.path().join("Projects"), home.path().join("Code")).unwrap();
        fs::create_dir_all(home.path().join("Projects/bad-marker")).unwrap();
        symlink(other.path(), home.path().join("Projects/bad-marker/.git")).unwrap();
        fs::create_dir_all(home.path().join("Projects/invalid-gitfile")).unwrap();
        fs::write(
            home.path().join("Projects/invalid-gitfile/.git"),
            "not a gitfile",
        )
        .unwrap();
        let found = scan(home.path(), LIMITS).unwrap();
        assert_eq!(found.repos.len(), 1);
        assert_eq!(found.repos[0].name, "real");
        assert_eq!(found.scanned_roots.len(), 1);
    }

    #[test]
    fn reports_bounded_partial_results_and_refuses_unsafe_home_paths() {
        let home = tempfile::tempdir().unwrap();
        for name in ["a", "b", "c"] {
            repository(home.path(), &format!("Projects/{name}"));
        }
        for (limits, reason) in [
            (
                Limits {
                    results: 1,
                    ..LIMITS
                },
                "results",
            ),
            (
                Limits {
                    entries: 1,
                    ..LIMITS
                },
                "entries",
            ),
            (
                Limits {
                    time: Duration::ZERO,
                    ..LIMITS
                },
                "time",
            ),
            (
                Limits {
                    bytes: 300,
                    ..LIMITS
                },
                "response_size",
            ),
        ] {
            let found = scan(home.path(), limits).unwrap();
            assert!(found.incomplete);
            assert_eq!(found.limit_reason, reason);
            assert!(serde_json::to_vec(&found).unwrap().len() <= limits.bytes);
        }
        assert!(scan(Path::new("/"), LIMITS).is_err());
        assert!(scan(Path::new("relative"), LIMITS).is_err());
    }
}
