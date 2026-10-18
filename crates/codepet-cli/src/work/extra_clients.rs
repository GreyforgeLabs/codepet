//! Candidate harnesses stay disabled until their tool/auth boundary is qualified.
use super::{ClientId, Resolved, Result};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

// Inspected versions, not a claim that these execution adapters are supported.
pub(super) fn version(id: ClientId) -> &'static str {
    match id {
        ClientId::Grok => "1.0.60",
        ClientId::Pi => "0.87.1",
        ClientId::Muse => "1.4.7-R6101.1",
        ClientId::Hermes => "0.21.3",
        _ => "unqualified",
    }
}

pub(super) fn resolve(id: ClientId) -> Result<Resolved> {
    Err(availability(id).1)
}

pub(super) fn availability(id: ClientId) -> (&'static str, String) {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let paths = std::env::var_os("PATH").unwrap_or_default();
    candidate_status(id, detected(id, home.as_deref(), &paths))
}

fn executable_present(path: &Path) -> bool {
    std::fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

fn detected(id: ClientId, home: Option<&Path>, paths: &std::ffi::OsStr) -> bool {
    let command = match id {
        ClientId::Muse => "muse",
        ClientId::Grok => "grok",
        ClientId::Hermes => "hermes",
        ClientId::Pi => "pi",
        _ => return false,
    };
    // Presence checks never execute wrappers: several clients update themselves there.
    if std::env::split_paths(paths).any(|directory| executable_present(&directory.join(command))) {
        return true;
    }
    home.is_some_and(|home| {
        let relative = match id {
            ClientId::Grok => ".grok/bin/grok".into(),
            ClientId::Hermes => ".hermes/hermes-agent/hermes".into(),
            ClientId::Pi => format!(".local/share/mise/installs/pi/{}/pi/pi", version(id)),
            ClientId::Muse => ".local/share/mise/installs/http-muse/latest/muse".into(),
            _ => return false,
        };
        executable_present(&home.join(relative))
    })
}

fn candidate_status(id: ClientId, installed: bool) -> (&'static str, String) {
    if !installed {
        return (
            "not-installed",
            format!(
                "{} is not detected. Its CodePet adapter is not available yet.",
                id.name()
            ),
        );
    }
    let reason = match id {
        ClientId::Muse => "Muse is detected. Its isolated CodePet adapter is not available yet.",
        ClientId::Grok => "Grok is detected. Its CodePet adapter still needs credential isolation.",
        ClientId::Hermes => "Hermes is detected. Its CodePet adapter still needs isolated tools and runtime support.",
        ClientId::Pi => "Pi is detected. Its CodePet adapter still needs credential isolation.",
        _ => "This coding client does not have a CodePet adapter.",
    };
    // Disabled candidates do not read authentication stores or suggest a provider route.
    ("adapter-not-ready", reason.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detecting_a_candidate_never_makes_it_runnable() {
        for id in [
            ClientId::Muse,
            ClientId::Grok,
            ClientId::Hermes,
            ClientId::Pi,
        ] {
            assert_eq!(candidate_status(id, false).0, "not-installed");
            assert_eq!(candidate_status(id, true).0, "adapter-not-ready");
            assert!(resolve(id).is_err());
        }
    }

    #[test]
    fn presence_checks_never_execute_update_wrappers() {
        let fixture = tempfile::tempdir().unwrap();
        let wrapper = fixture.path().join("muse");
        std::fs::write(&wrapper, "#!/bin/sh\nprintf invoked > \"$0.called\"\n").unwrap();
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(detected(ClientId::Muse, None, fixture.path().as_os_str()));
        assert!(!fixture.path().join("muse.called").exists());
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(!detected(ClientId::Muse, None, fixture.path().as_os_str()));
        assert!(!detected(ClientId::Pi, None, fixture.path().as_os_str()));
    }
}
