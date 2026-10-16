//! OpenCode (spec §5.5): one file, `~/.config/opencode/plugins/codepet.js`, headed
//! `// managed by codepet`, with no npm dependencies; `opencode.json` and `package.json` are
//! never touched. OpenCode loads `{plugin,plugins}/*.{ts,js}` including dotfiles, so the
//! temporary name is `.codepet.js.partial`. Uninstall deletes the file.
use super::{write_file, Artifact, Connected, Ctx, Failed, MARKER};
use std::fs;
use std::io;
use std::path::PathBuf;

pub const TEMPLATE: &str = include_str!("../../../../integrations/opencode/codepet.js.in");
const TMP: &str = ".codepet.js.partial";

pub fn file_path(ctx: &Ctx) -> PathBuf {
    ctx.env.opencode_dir.join("plugins/codepet.js")
}

fn ours(text: &str) -> bool {
    text.lines()
        .next()
        .is_some_and(|l| l.starts_with("//") && l.contains(MARKER))
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
    let dir = path.parent().expect("plugins dir");
    super::ensure_dir(dir, 0o755)?;
    super::sweep_temps(dir);
    if let Ok(old) = fs::read_to_string(&path) {
        if old != body && !ours(&old) {
            return Err(Failed::new(
                super::Reason::Blocked,
                "a codepet.js that is not CodePet's",
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
                "codepet.js is not CodePet's",
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
