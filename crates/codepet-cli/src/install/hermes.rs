//! Hermes Agent (spec §5.6): a plugin dir `~/.hermes/plugins/codepet/` (`plugin.yaml` +
//! `__init__.py`), built in `<data_dir>/staging` and renamed in (Hermes loads any dir with
//! a `plugin.yaml`), then enabled with `hermes plugins enable codepet`. Hermes's CLI
//! rewrites `~/.hermes/config.yaml` itself (`plugins.enabled`/`plugins.disabled` as sorted
//! lists, normalised YAML); CodePet never writes that file and checks the named sections
//! before and after. Plugins rather than shell hooks, because shell hooks need a
//! first-use TTY consent. The version comes from package metadata, never
//! `hermes --version`.
use super::yamlish::{self, Y};
use super::{runner, Artifact, Connected, Ctx, Failed, Reason, MARKER};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const PLUGIN_YAML: &str = include_str!("../../../../integrations/hermes/plugin.yaml");
pub const INIT_PY: &str = include_str!("../../../../integrations/hermes/__init__.py.in");
pub const NAME: &str = "codepet";
/// The `config.yaml` sections the key check compares (hooks, permissions, plugins).
const CHECKED: [&str; 6] = [
    "hooks",
    "hooks_auto_accept",
    "approvals",
    "command_allowlist",
    "permissions",
    "plugins",
];

pub fn plugin_dir(ctx: &Ctx) -> PathBuf {
    ctx.env.hermes_dir.join("plugins").join(NAME)
}

pub fn files(ctx: &Ctx) -> Result<Vec<super::TreeFile>, Failed> {
    Ok(vec![
        ("plugin.yaml", PLUGIN_YAML.as_bytes().to_vec(), 0o644),
        ("__init__.py", ctx.render(INIT_PY)?.into_bytes(), 0o644),
    ])
}

fn ours(dir: &Path) -> bool {
    fs::read_to_string(dir.join("plugin.yaml")).is_ok_and(|t| t.contains(MARKER))
}

pub fn status(ctx: &Ctx) -> Artifact {
    let dir = plugin_dir(ctx);
    if !ours(&dir) {
        return Artifact::Missing;
    }
    let current = files(ctx).is_ok_and(|f| {
        f.iter()
            .all(|(rel, bytes, _)| fs::read(dir.join(rel)).is_ok_and(|b| b == *bytes))
    });
    if current {
        Artifact::Current
    } else {
        Artifact::Stale
    }
}

/// `config.yaml` parsed (absent = empty); `Err` when unreadable.
fn read_config(ctx: &Ctx) -> Result<Y, ()> {
    match fs::read_to_string(ctx.env.hermes_dir.join("config.yaml")) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Y::Map(Vec::new())),
        Err(_) => Err(()),
        Ok(text) if text.len() > 4 * 1024 * 1024 => Err(()),
        Ok(text) => yamlish::parse(&text).map_err(|_| ()),
    }
}

/// Artifact ownership survives disabling; connection status also requires enablement.
pub fn enabled(ctx: &Ctx) -> bool {
    read_config(ctx).is_ok_and(|c| enabled_in(&c))
}

fn set_of(y: Option<&Y>) -> Option<Vec<String>> {
    match y {
        None => Some(Vec::new()),
        Some(v) => v.str_set(),
    }
}

fn plugins_entry<'a>(c: &'a Y, key: &str) -> Option<&'a Y> {
    c.get("plugins").and_then(|p| p.get(key))
}

/// `codepet` is in `plugins.enabled` and not in `plugins.disabled`.
fn enabled_in(c: &Y) -> bool {
    set_of(plugins_entry(c, "enabled")).is_some_and(|s| s.iter().any(|n| n == NAME))
        && set_of(plugins_entry(c, "disabled")).is_some_and(|s| !s.iter().any(|n| n == NAME))
}

/// The key check (spec §5.2 step 4): the checked sections are unchanged, except that
/// `codepet` moved into (`enable`) or out of `plugins.enabled`, the reverse for
/// `plugins.disabled`, and a `plugins.entries.codepet` may appear.
pub fn config_unchanged(before: &Y, after: &Y, enable: bool) -> bool {
    let empty = Y::Map(Vec::new());
    for key in CHECKED {
        let (b, a) = (before.get(key), after.get(key));
        if key != "plugins" {
            if b != a {
                return false;
            }
            continue;
        }
        let b = b.unwrap_or(&empty);
        let a = a.unwrap_or(&empty);
        let (Y::Map(bm), Y::Map(am)) = (b, a) else {
            if b == a {
                continue;
            }
            return false;
        };
        let names: std::collections::BTreeSet<&String> =
            bm.iter().chain(am.iter()).map(|(k, _)| k).collect();
        for name in names {
            let (bv, av) = (b.get(name), a.get(name));
            let ok = match name.as_str() {
                "enabled" | "disabled" => {
                    let (Some(mut bs), Some(as_)) = (set_of(bv), set_of(av)) else {
                        return false;
                    };
                    let add = (name == "enabled") == enable;
                    bs.retain(|n| n != NAME);
                    if add {
                        bs.push(NAME.to_owned());
                    }
                    bs.sort();
                    bs == as_
                }
                "entries" => {
                    let strip = |v: Option<&Y>| match v {
                        Some(Y::Map(kv)) => {
                            Y::Map(kv.iter().filter(|(k, _)| k != NAME).cloned().collect())
                        }
                        Some(Y::Null) | None => Y::Map(Vec::new()),
                        Some(other) => other.clone(),
                    };
                    strip(bv) == strip(av)
                }
                _ => bv == av,
            };
            if !ok {
                return false;
            }
        }
    }
    true
}

fn hermes(ctx: &Ctx, bin: &Path, verb: &str) -> Result<runner::CliRun, Failed> {
    runner::run(&ctx.env, bin, &["plugins", verb, NAME], &[])
        .map_err(|_| Failed::new(Reason::Blocked, "could not start hermes"))
}

pub fn connect(ctx: &Ctx, bin: &Path) -> Result<Connected, Failed> {
    let before = read_config(ctx)
        .map_err(|()| Failed::new(Reason::Blocked, "config.yaml is not readable YAML"))?;
    let dir = plugin_dir(ctx);
    if fs::symlink_metadata(&dir).is_ok() && !ours(&dir) {
        return Err(Failed::new(
            Reason::Blocked,
            "a plugins/codepet that is not CodePet's",
        ));
    }
    if let Some(parent) = dir.parent() {
        super::sweep_temps(parent);
    }
    super::install_dir(&ctx.staging, &dir, &files(ctx)?)?;
    if enabled_in(&before) {
        return Ok(Connected {
            connected: true,
            needs_step: None,
        });
    }
    let run = hermes(ctx, bin, "enable")?;
    let after = read_config(ctx);
    if after
        .as_ref()
        .map_or(true, |a| !config_unchanged(&before, a, true))
    {
        return Err(Failed::new(
            Reason::SettingsChanged,
            "config.yaml changed beyond CodePet's keys",
        )
        .installed());
    }
    if run.timed_out || !run.ok() || !after.as_ref().is_ok_and(enabled_in) {
        return Err(
            Failed::new(Reason::Blocked, "hermes plugins enable did not succeed").installed(),
        );
    }
    Ok(Connected {
        connected: true,
        needs_step: None,
    })
}

/// Disables with Hermes's CLI (when enabled), then removes the plugin dir.
pub fn disconnect(ctx: &Ctx, bin: Option<&Path>) -> Result<(), Failed> {
    let dir = plugin_dir(ctx);
    let before = match read_config(ctx) {
        Ok(c) => c,
        // Without CodePet's plugin dir there is nothing of CodePet's to stop, and a
        // `config.yaml` the small reader cannot parse is no reason to report a failure.
        Err(()) if fs::symlink_metadata(&dir).is_err() => return Ok(()),
        Err(()) => {
            return Err(Failed::new(
                Reason::Blocked,
                "config.yaml is not readable YAML",
            ))
        }
    };
    if enabled_in(&before) {
        let bin =
            bin.ok_or_else(|| Failed::new(Reason::Blocked, "hermes is not installed").installed())?;
        let run = hermes(ctx, bin, "disable")?;
        let after = read_config(ctx);
        if after
            .as_ref()
            .map_or(true, |a| !config_unchanged(&before, a, false))
        {
            return Err(Failed::new(
                Reason::SettingsChanged,
                "config.yaml changed beyond CodePet's keys",
            )
            .installed());
        }
        if run.timed_out || !run.ok() || after.as_ref().is_ok_and(enabled_in) {
            return Err(
                Failed::new(Reason::Blocked, "hermes plugins disable did not succeed").installed(),
            );
        }
    }
    if fs::symlink_metadata(&dir).is_ok() {
        if !ours(&dir) {
            return Err(Failed::new(
                Reason::Blocked,
                "plugins/codepet is not CodePet's",
            ));
        }
        fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

pub fn refresh(ctx: &Ctx) -> io::Result<bool> {
    let dir = plugin_dir(ctx);
    if !ours(&dir) {
        return Ok(false);
    }
    let Ok(files) = files(ctx) else {
        return Ok(false);
    };
    super::install_dir(&ctx.staging, &dir, &files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn y(t: &str) -> Y {
        yamlish::parse(t).unwrap()
    }

    /// The real Hermes 0.21.3 CLI's rewrite (tools/integrations/hermes-offline-gate.sh,
    /// `tests/fixtures/hermes-cli-gate-0.21.3.txt`): comments dropped, lists re-indented and
    /// sorted, `disabled: []`, `_config_version` and trailing comment blocks added.
    #[test]
    fn key_check_accepts_the_real_cli_rewrite() {
        let before = y("# throwaway\nmodel:\n  default: m1\nplugins:\n  enabled:\n  - other\nhooks:\n  pre_tool_call:\n  - command: echo hi\n");
        let tail = "_config_version: 45\n\n# ── Security ───\n# Secret redaction is ON by default.\n#\n# security:\n#   redact_secrets: true\n";
        let enabled = y(&format!("model:\n  default: m1\nplugins:\n  enabled:\n    - codepet\n    - other\n  disabled: []\nhooks:\n  pre_tool_call:\n    - command: echo hi\n{tail}"));
        let disabled = y(&format!("model:\n  default: m1\nplugins:\n  enabled:\n    - other\n  disabled:\n    - codepet\nhooks:\n  pre_tool_call:\n    - command: echo hi\n{tail}"));
        assert!(config_unchanged(&before, &enabled, true));
        assert!(enabled_in(&enabled) && !enabled_in(&disabled));
        assert!(config_unchanged(&enabled, &disabled, false));
    }

    #[test]
    fn key_check_allows_only_the_codepet_move() {
        let before = y("model: x\nplugins:\n  enabled: [herdr-agent-state]\n  disabled: [codepet, old]\nhooks:\n  a: 1\n");
        let enabled = y("hooks:\n  a: 1\nmodel: x\nplugins:\n  disabled:\n  - old\n  enabled:\n  - codepet\n  - herdr-agent-state\n");
        assert!(config_unchanged(&before, &enabled, true));
        assert!(enabled_in(&enabled) && !enabled_in(&before));
        let disabled = y("hooks:\n  a: 1\nplugins:\n  disabled: [codepet, old]\n  enabled: [herdr-agent-state]\n");
        assert!(config_unchanged(&enabled, &disabled, false));
        // From nothing: the CLI creates the plugins section.
        assert!(config_unchanged(
            &Y::Map(vec![]),
            &y("plugins:\n  enabled: [codepet]\n  disabled: []\n"),
            true
        ));
        for bad in [
            "plugins:\n  enabled: [codepet, herdr-agent-state, other]\n  disabled: [old]\nhooks:\n  a: 1\n",
            "plugins:\n  enabled: [codepet, herdr-agent-state]\n  disabled: [old]\nhooks:\n  a: 2\n",
            "plugins:\n  enabled: [codepet, herdr-agent-state]\n  disabled: [old]\nhooks:\n  a: 1\nhooks_auto_accept: true\n",
            "plugins:\n  enabled: [codepet, herdr-agent-state]\n  disabled: []\nhooks:\n  a: 1\n",
            "plugins:\n  enabled: [codepet, herdr-agent-state]\n  disabled: [old]\n  scan_on_install: false\nhooks:\n  a: 1\n",
        ] {
            assert!(!config_unchanged(&before, &y(bad), true), "{bad}");
        }
    }
}
