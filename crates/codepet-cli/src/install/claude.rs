//! Claude Code (spec §5.3): a local plugin inside the `codepet-local` marketplace that
//! CodePet ships at `<data_dir>/integrations/claude-code/`, installed with Claude Code's
//! own CLI, which records `enabledPlugins` and `extraKnownMarketplaces` in its
//! `settings.json` (CodePet never writes that file; the settings key check compares it
//! before and after). Status reads only CodePet's entry in `plugins/installed_plugins.json`.
//!
//! Day-0 gate (offline, `CLAUDE_CONFIG_DIR` honoured, no model call; shapes in
//! `tests/fixtures/claude-cli/`): `plugin install codepet --marketplace <dir> --json`
//! answers `{"outcome":"ok",...}` (also when already installed); `plugin uninstall
//! codepet@codepet-local --json` and `plugin marketplace remove codepet-local --json`
//! answer `outcome: failed` with `failureCode` `not_installed` / `not_configured` when
//! there is nothing to remove, which counts as done. Uninstalling leaves
//! `extraKnownMarketplaces: {}` behind. The plugin loads in place from the marketplace dir.
use super::{runner, Artifact, Connected, Ctx, Failed, Reason};
use serde_json::{Map, Value};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const PLUGIN_ID: &str = "codepet@codepet-local";
pub const MARKETPLACE: &str = "codepet-local";
const MARKETPLACE_JSON: &str =
    include_str!("../../../../integrations/claude-code/.claude-plugin/marketplace.json");
const PLUGIN_JSON: &str =
    include_str!("../../../../integrations/claude-code/plugin/.claude-plugin/plugin.json.in");
pub const HOOKS_JSON: &str =
    include_str!("../../../../integrations/claude-code/plugin/hooks/hooks.json.in");
/// Claude Code's own extra variables under the CLI rules.
pub const CLI_ENV: [(&str, &str); 2] = [
    ("DISABLE_AUTOUPDATER", "1"),
    ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1"),
];

pub fn marketplace_dir(ctx: &Ctx) -> PathBuf {
    ctx.integrations.join("claude-code")
}

/// The marketplace tree: (relative path, bytes, mode).
pub fn files(ctx: &Ctx) -> Result<Vec<super::TreeFile>, Failed> {
    Ok(vec![
        (
            ".claude-plugin/marketplace.json",
            MARKETPLACE_JSON.as_bytes().to_vec(),
            0o644,
        ),
        (
            "plugin/.claude-plugin/plugin.json",
            PLUGIN_JSON
                .replace("@CODEPET_VERSION@", env!("CARGO_PKG_VERSION"))
                .into_bytes(),
            0o644,
        ),
        (
            "plugin/hooks/hooks.json",
            ctx.render(HOOKS_JSON)?.into_bytes(),
            0o644,
        ),
    ])
}

fn installed_plugins(ctx: &Ctx) -> Option<Value> {
    let p = ctx.env.claude_dir.join("plugins/installed_plugins.json");
    let m = fs::metadata(&p).ok()?;
    if m.len() > 4 * 1024 * 1024 {
        return None;
    }
    serde_json::from_slice(&fs::read(p).ok()?).ok()
}

/// CodePet's entry is in `installed_plugins.json` (the only key read there).
pub fn plugin_recorded(ctx: &Ctx) -> bool {
    installed_plugins(ctx)
        .and_then(|v| v.get("plugins").and_then(|p| p.get(PLUGIN_ID)).cloned())
        .is_some_and(|e| e.as_array().is_some_and(|a| !a.is_empty()))
}

fn tree_current(ctx: &Ctx) -> bool {
    let dir = marketplace_dir(ctx);
    files(ctx).is_ok_and(|fs_| {
        fs_.iter()
            .all(|(rel, bytes, _)| fs::read(dir.join(rel)).is_ok_and(|b| b == *bytes))
    })
}

pub fn status(ctx: &Ctx) -> Artifact {
    if !plugin_recorded(ctx) {
        return Artifact::Missing;
    }
    if tree_current(ctx) {
        Artifact::Current
    } else {
        Artifact::Stale
    }
}

/// `settings.json` as a JSON object (absent = empty); `Err` when unreadable.
fn read_settings(dir: &Path) -> Result<Map<String, Value>, ()> {
    match fs::read(dir.join("settings.json")) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Map::new()),
        Err(_) => Err(()),
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(Value::Object(m)) => Ok(m),
            _ => Err(()),
        },
    }
}

/// The settings key check (spec §5.2 step 4): every top-level key other than
/// `enabledPlugins` and `extraKnownMarketplaces` is unchanged (that covers `hooks` and
/// `permissions`), and inside those two only CodePet's own entry may appear, change or go.
/// An absent map equals an empty one (Claude leaves `extraKnownMarketplaces: {}` behind).
pub fn settings_unchanged(before: &Map<String, Value>, after: &Map<String, Value>) -> bool {
    let empty = Value::Object(Map::new());
    let own = |k: &str| match k {
        "enabledPlugins" => Some(PLUGIN_ID),
        "extraKnownMarketplaces" => Some(MARKETPLACE),
        _ => None,
    };
    let keys: std::collections::BTreeSet<&String> = before.keys().chain(after.keys()).collect();
    keys.into_iter().all(|k| match own(k) {
        None => before.get(k) == after.get(k),
        Some(ours) => {
            let (Some(b), Some(a)) = (
                before.get(k).unwrap_or(&empty).as_object(),
                after.get(k).unwrap_or(&empty).as_object(),
            ) else {
                return false;
            };
            let strip = |m: &Map<String, Value>| {
                let mut m = m.clone();
                m.remove(ours);
                m
            };
            strip(b) == strip(a)
        }
    })
}

fn settings_enable_ours(m: &Map<String, Value>) -> bool {
    m.get("enabledPlugins")
        .and_then(|e| e.get(PLUGIN_ID))
        .is_some_and(|v| v == &Value::Bool(true))
}

/// The CLI's one-line `--json` result: `Ok` for `outcome: ok` or one of `done_codes`.
fn cli_ok(run: &runner::CliRun, done_codes: &[&str]) -> bool {
    let Some(line) = run
        .stdout
        .lines()
        .rev()
        .find(|l| l.trim_start().starts_with('{'))
    else {
        return run.ok();
    };
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return run.ok();
    };
    match v.get("outcome").and_then(Value::as_str) {
        Some("ok") => true,
        Some("failed") => v
            .get("failureCode")
            .and_then(Value::as_str)
            .is_some_and(|c| done_codes.contains(&c)),
        _ => run.ok(),
    }
}

fn claude(ctx: &Ctx, bin: &Path, args: &[&str]) -> Result<runner::CliRun, Failed> {
    runner::run(&ctx.env, bin, args, &CLI_ENV)
        .map_err(|_| Failed::new(Reason::Blocked, "could not start claude"))
}

pub fn connect(ctx: &Ctx, bin: &Path) -> Result<Connected, Failed> {
    let dir = marketplace_dir(ctx);
    super::install_dir(&ctx.staging, &dir, &files(ctx)?)?;
    let before = read_settings(&ctx.env.claude_dir)
        .map_err(|()| Failed::new(Reason::Blocked, "settings.json is not readable JSON"))?;
    if plugin_recorded(ctx) && settings_enable_ours(&before) {
        return Ok(Connected {
            connected: true,
            needs_step: None,
        });
    }
    let dir_s = dir
        .to_str()
        .ok_or_else(|| Failed::new(Reason::Blocked, "marketplace path is not UTF-8"))?;
    let run = claude(
        ctx,
        bin,
        &[
            "plugin",
            "install",
            "codepet",
            "--marketplace",
            dir_s,
            "--json",
        ],
    )?;
    let after = read_settings(&ctx.env.claude_dir);
    let changed = after
        .as_ref()
        .map_or(true, |a| !settings_unchanged(&before, a));
    if changed {
        return Err(Failed::new(
            Reason::SettingsChanged,
            "settings.json changed beyond CodePet's keys",
        )
        .installed());
    }
    if run.timed_out || !cli_ok(&run, &[]) || !plugin_recorded(ctx) {
        return Err(
            Failed::new(Reason::Blocked, "claude plugin install did not succeed").installed(),
        );
    }
    Ok(Connected {
        connected: true,
        needs_step: None,
    })
}

/// Uninstalls with Claude Code's CLI, then removes the marketplace dir. Without a usable
/// `claude` the plugin stays registered; the `disconnected` flag already makes it inert.
pub fn disconnect(ctx: &Ctx, bin: Option<&Path>) -> Result<(), Failed> {
    let before = read_settings(&ctx.env.claude_dir)
        .map_err(|()| Failed::new(Reason::Blocked, "settings.json is not readable JSON"))?;
    let recorded = plugin_recorded(ctx)
        || before.contains_key("enabledPlugins")
            && before["enabledPlugins"].get(PLUGIN_ID).is_some()
        || before
            .get("extraKnownMarketplaces")
            .is_some_and(|m| m.get(MARKETPLACE).is_some());
    if recorded {
        let bin =
            bin.ok_or_else(|| Failed::new(Reason::Blocked, "claude is not installed").installed())?;
        let a = claude(ctx, bin, &["plugin", "uninstall", PLUGIN_ID, "--json"])?;
        let b = claude(
            ctx,
            bin,
            &["plugin", "marketplace", "remove", MARKETPLACE, "--json"],
        )?;
        let after = read_settings(&ctx.env.claude_dir);
        if after
            .as_ref()
            .map_or(true, |a| !settings_unchanged(&before, a))
        {
            return Err(Failed::new(
                Reason::SettingsChanged,
                "settings.json changed beyond CodePet's keys",
            )
            .installed());
        }
        if a.timed_out
            || b.timed_out
            || !cli_ok(&a, &["not_installed"])
            || !cli_ok(&b, &["not_configured"])
            || plugin_recorded(ctx)
        {
            return Err(
                Failed::new(Reason::Blocked, "claude plugin uninstall did not succeed").installed(),
            );
        }
    }
    match fs::remove_dir_all(marketplace_dir(ctx)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

pub fn refresh(ctx: &Ctx) -> io::Result<bool> {
    let dir = marketplace_dir(ctx);
    let ours = fs::read_to_string(dir.join(".claude-plugin/marketplace.json"))
        .is_ok_and(|t| t.contains(super::MARKER));
    if !ours {
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
    use serde_json::json;

    fn obj(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn settings_check_allows_only_codepets_entries() {
        let before = obj(
            json!({"hooks": {"Stop": [1]}, "permissions": {"allow": ["x"]},
            "enabledPlugins": {"a@b": true}, "theme": "dark"}),
        );
        let installed = obj(
            json!({"permissions": {"allow": ["x"]}, "hooks": {"Stop": [1]},
            "enabledPlugins": {"a@b": true, PLUGIN_ID: true},
            "extraKnownMarketplaces": {MARKETPLACE: {"source": {"source": "directory", "path": "/p"}}},
            "theme": "dark"}),
        );
        assert!(settings_unchanged(&before, &installed));
        // Uninstall leaves an empty extraKnownMarketplaces behind.
        let removed = obj(
            json!({"permissions": {"allow": ["x"]}, "hooks": {"Stop": [1]},
            "enabledPlugins": {"a@b": true}, "extraKnownMarketplaces": {}, "theme": "dark"}),
        );
        assert!(settings_unchanged(&installed, &removed));
        assert!(settings_unchanged(&before, &removed));
        for bad in [
            json!({"hooks": {}, "permissions": {"allow": ["x"]}, "enabledPlugins": {"a@b": true}, "theme": "dark"}),
            json!({"hooks": {"Stop": [1]}, "permissions": {"allow": ["x"]}, "enabledPlugins": {"a@b": false}, "theme": "dark"}),
            json!({"hooks": {"Stop": [1]}, "permissions": {"allow": ["x"]}, "enabledPlugins": {"a@b": true, "z@q": true}, "theme": "dark"}),
            json!({"hooks": {"Stop": [1]}, "permissions": {"allow": ["x", "y"]}, "enabledPlugins": {"a@b": true}, "theme": "dark"}),
            json!({"hooks": {"Stop": [1]}, "permissions": {"allow": ["x"]}, "enabledPlugins": {"a@b": true}, "theme": "dark", "new": 1}),
            json!({"hooks": {"Stop": [1]}, "permissions": {"allow": ["x"]}, "enabledPlugins": {"a@b": true}, "theme": "dark", "extraKnownMarketplaces": {"other": {}}}),
        ] {
            assert!(!settings_unchanged(&before, &obj(bad.clone())), "{bad}");
        }
    }
}
