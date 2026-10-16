//! Grok CLI (spec §5.4): one file, `~/.grok/hooks/codepet.json`. Global hook files are
//! always trusted. Grok loads every `*.json` in that dir, so the temporary name is
//! `.codepet.json.partial`. Uninstall deletes the file. Grok runs `command` with `sh -c`
//! on Unix (its hook guide, "Unsupported Handler Fields"), so the stored form keeps the
//! shell guard. The file is CodePet's when it is named `codepet.json` and every handler
//! runs a `codepet-hook grok-cli` command (JSON has no comment marker).
use super::{jsonish, write_file, Artifact, Connected, Ctx, Failed};
use codepet_core::agent::ClientId;
use std::fs;
use std::io;
use std::path::PathBuf;

pub const TEMPLATE: &str = include_str!("../../../../integrations/grok-cli/codepet.json.in");
const TMP: &str = ".codepet.json.partial";

pub fn file_path(ctx: &Ctx) -> PathBuf {
    ctx.env.grok_dir.join("hooks/codepet.json")
}

/// Every handler command in a Grok/Claude-shaped hooks file, or `None` if unreadable.
pub fn commands(text: &str) -> Option<Vec<String>> {
    let tree = jsonish::parse(text).ok()?;
    let mut out = Vec::new();
    let jsonish::J::Obj(events) = tree.get("hooks")? else {
        return None;
    };
    for (_, groups) in events {
        for g in groups.as_arr()? {
            for h in g.get("hooks")?.as_arr()? {
                out.push(h.get("command")?.as_str()?.to_owned());
            }
        }
    }
    Some(out)
}

fn ours(text: &str) -> bool {
    commands(text).is_some_and(|c| {
        !c.is_empty()
            && c.iter()
                .all(|c| super::is_our_command(c, ClientId::GrokCli))
    })
}

pub fn status(ctx: &Ctx) -> Artifact {
    let Ok(text) = fs::read_to_string(file_path(ctx)) else {
        return Artifact::Missing;
    };
    match ctx.render(TEMPLATE) {
        Ok(want) if want == text => Artifact::Current,
        _ if ours(&text) => Artifact::Stale,
        _ => Artifact::Missing,
    }
}

pub fn connect(ctx: &Ctx) -> Result<Connected, Failed> {
    let body = ctx.render(TEMPLATE)?;
    let path = file_path(ctx);
    let dir = path.parent().expect("hooks dir");
    super::ensure_dir(dir, 0o755)?;
    super::sweep_temps(dir);
    if let Ok(old) = fs::read_to_string(&path) {
        if old != body && !ours(&old) {
            return Err(Failed::new(
                super::Reason::Blocked,
                "a codepet.json that is not CodePet's",
            ));
        }
    }
    write_file(&path, TMP, body.as_bytes(), 0o644)?;
    Ok(Connected {
        connected: true,
        needs_step: None,
    })
}

pub fn disconnect(ctx: &Ctx) -> Result<(), Failed> {
    let path = file_path(ctx);
    if let Ok(text) = fs::read_to_string(&path) {
        if !ours(&text) {
            return Err(Failed::new(
                super::Reason::Blocked,
                "codepet.json is not CodePet's",
            ));
        }
        super::remove_file(&path)?;
    }
    if let Some(dir) = path.parent() {
        super::sweep_temps(dir);
    }
    Ok(())
}

pub fn refresh(ctx: &Ctx) -> io::Result<bool> {
    let path = file_path(ctx);
    let Ok(text) = fs::read_to_string(&path) else {
        return Ok(false);
    };
    let Ok(want) = ctx.render(TEMPLATE) else {
        return Ok(false);
    };
    if text == want || !ours(&text) {
        return Ok(false);
    }
    write_file(&path, TMP, want.as_bytes(), 0o644)
}
