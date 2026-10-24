//! The pet's command line without a display: help, version, usage errors and `ctl` with
//! no running pet. Nothing here starts a pet or touches a Wayland session.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A fresh, empty HOME under Cargo's per-target test directory.
fn home(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("pet-cli-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn pet(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codepet-pet"))
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", home)
        .env("XDG_RUNTIME_DIR", home)
        .output()
        .unwrap()
}

#[test]
fn help_and_version_succeed_on_stdout() {
    let home = home("help");
    for flag in ["--help", "-h"] {
        let out = pet(&home, &[flag]);
        assert_eq!(out.status.code(), Some(0), "{flag}");
        let text = String::from_utf8(out.stdout).unwrap();
        assert!(text.starts_with("usage: codepet-pet"), "{text}");
        assert!(text.contains("setReducedMotion BOOL"), "{text}");
        assert!(out.stderr.is_empty());
    }
    let out = pet(&home, &["--version"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(out.stdout).unwrap().trim(),
        format!("codepet-pet {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn usage_errors_exit_2_and_ctl_without_a_pet_exits_1() {
    let home = home("usage");
    for args in [&[][..], &["bogus"], &["ctl"]] {
        let out = pet(&home, args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(String::from_utf8(out.stderr).unwrap().starts_with("usage:"));
    }
    let out = pet(&home, &["ctl", "status\nquit"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "a verb may not smuggle a second line"
    );
    let out = pet(&home, &["ctl", "ping"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(out.stderr).unwrap().trim(),
        "codepet-pet: not running"
    );
    assert!(!&home.join("codepet-pet.sock").exists());
}
