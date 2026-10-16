//! The frozen POSIX shim every client artifact runs (`docs/v0.3/contracts/shim.sh.template`,
//! `commands.json` `shim`). Connect substitutes only the two absolute paths; the content
//! never depends on the CodePet version. WP3's clients job installs it with
//! [`write_atomically`]; the hook tests and `scripts/e2e-hooks.sh` render the same bytes.
#![cfg_attr(not(test), allow(dead_code))]
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

/// The shim template (placeholders `@CODEPET_STATE_DIR@`, `@CODEPET_BIN@`).
pub const TEMPLATE: &str = include_str!("../../../../docs/v0.3/contracts/shim.sh.template");
/// Written in place of the shim when an uninstall cannot disconnect every client.
pub const TOMBSTONE: &str = include_str!("../../../../docs/v0.3/contracts/shim-tombstone.sh");
const STATE_PLACEHOLDER: &str = "@CODEPET_STATE_DIR@";
const BIN_PLACEHOLDER: &str = "@CODEPET_BIN@";
/// The longest path either placeholder may take.
pub const MAX_PATH_BYTES: usize = 4096;

/// A path the shim cannot carry safely inside its single quotes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Blocked;

fn shim_path_ok(p: &Path) -> Option<&str> {
    let s = p.to_str()?;
    (p.is_absolute()
        && !s.is_empty()
        && s.len() <= MAX_PATH_BYTES
        && !s.contains(['\'', '\n', '\0']))
    .then_some(s)
}

/// The shim's bytes for `state_dir` and `bin` (`commands.json` `path_rule`: absolute, no
/// `'`, newline or NUL, at most 4096 bytes; otherwise Connect fails with `blocked`).
pub fn render(state_dir: &Path, bin: &Path) -> Result<String, Blocked> {
    let s = shim_path_ok(state_dir).ok_or(Blocked)?;
    let b = shim_path_ok(bin).ok_or(Blocked)?;
    Ok(TEMPLATE
        .replacen(STATE_PLACEHOLDER, s, 1)
        .replacen(BIN_PLACEHOLDER, b, 1))
}

/// Writes `content` to `path` as mode 0755 through a temporary file in the same
/// directory (exclusive create, fsync, rename). The directory must exist.
pub fn write_atomically(path: &Path, content: &str) -> std::io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::other("the shim path has no directory"))?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| std::io::Error::other("the shim path has no file name"))?;
    let tmp = dir.join(format!(".{name}.partial-{}", std::process::id()));
    let result = (|| {
        // O_EXCL never follows a planted symlink; the mode is set explicitly (umask).
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o700)
            .open(&tmp)?;
        f.write_all(content.as_bytes())?;
        f.set_permissions(fs::Permissions::from_mode(0o755))?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn renders_the_contract_reference_bytes() {
        // commands.json reference_paths and reference_sha256.
        let shim = render(
            Path::new("/home/u/.local/state/codepet"),
            Path::new("/home/u/.local/bin/codepet"),
        )
        .unwrap();
        let commands: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../docs/v0.3/contracts/commands.json"
        ))
        .unwrap();
        // payload_digest is the first 16 hex of SHA-256.
        let digest = codepet_storage::payload_digest(shim.as_bytes());
        assert!(commands["shim"]["reference_sha256"]
            .as_str()
            .unwrap()
            .starts_with(&digest));
        assert!(shim.starts_with("#!/bin/sh\n# managed by codepet"));
        assert!(TOMBSTONE.contains("# managed by codepet: tombstone"));
    }

    #[test]
    fn refuses_paths_the_quotes_cannot_hold() {
        let ok = PathBuf::from("/s");
        for bad in ["rel/path", "/a'b", "/a\nb"] {
            assert_eq!(render(Path::new(bad), &ok), Err(Blocked), "{bad:?}");
            assert_eq!(render(&ok, Path::new(bad)), Err(Blocked), "{bad:?}");
        }
        let long = format!("/{}", "a".repeat(MAX_PATH_BYTES));
        assert_eq!(render(Path::new(&long), &ok), Err(Blocked));
    }

    #[test]
    fn writes_atomically_with_mode_0755() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("codepet-hook");
        write_atomically(&p, "#!/bin/sh\nexit 0\n").unwrap();
        write_atomically(&p, TOMBSTONE).unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), TOMBSTONE);
        assert_eq!(
            fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert_eq!(fs::read_dir(t.path()).unwrap().count(), 1, "no temp left");
    }
}
