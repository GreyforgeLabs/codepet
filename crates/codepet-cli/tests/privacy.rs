//! Privacy canary (SPEC-v0.3 §5.10, invariant O3): every content field of every client's
//! payloads carries a unique `CANARY-...` string. After hook + fold, nothing that CodePet
//! keeps (database, WAL, inbox names and contents, `.overflow`, `fold.backoff`, `jobs/`,
//! `bar-state.json`) contains a canary, a raw session/turn/tool id, a path or a tool name.
mod hookkit;

use hookkit::{assert_silent_ok, expand, fixtures, Hk, CLIENTS};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn files_under(p: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(p) else {
        return;
    };
    for e in rd.flatten() {
        let path = e.path();
        if path.is_dir() {
            files_under(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn contains(hay: &[u8], needle: &str) -> bool {
    hay.windows(needle.len()).any(|w| w == needle.as_bytes())
}

#[test]
fn no_content_survives_hook_and_fold() {
    let hk = Hk::with_db();
    // Canaries also in places the fixtures do not cover: ids nested in content, and a
    // tool name that must only be recognised (AskUserQuestion) and dropped.
    let extra: Vec<(&str, String)> = vec![
        (
            "claude-code",
            r#"{"hook_event_name":"UserPromptSubmit","session_id":"raw-session-zz9","prompt_id":"raw-turn-zz9","prompt":"CANARY-p2-7f3a","cwd":"/home/u/CANARY-cwd2-7f3a"}"#.into(),
        ),
        (
            "claude-code",
            r#"{"hook_event_name":"PreToolUse","session_id":"raw-session-zz9","prompt_id":"raw-turn-zz9","tool_use_id":"raw-tool-zz9","tool_name":"CANARY-toolname-7f3a","tool_input":{"session_id":"CANARY-nested-7f3a"}}"#.into(),
        ),
        (
            "claude-code",
            r#"{"hook_event_name":"StopFailure","session_id":"raw-session-zz9","prompt_id":"raw-turn-zz9","error":"CANARY-errenum-7f3a","error_details":"CANARY-errtext-7f3a"}"#.into(),
        ),
        (
            "grok-cli",
            r#"{"hook_event_name":"PreToolUse","sessionId":"raw-gsession-zz9","promptId":"g1","toolName":"AskUserQuestion","toolUseId":"raw-gtool-zz9","toolInput":{"q":"CANARY-question2-7f3a"}}"#.into(),
        ),
    ];
    let cli = [("CLAUDE_CODE_ENTRYPOINT", "cli")];
    for client in CLIENTS {
        for (name, fx) in fixtures(client) {
            if fx["nonce"].is_string() {
                continue; // ready-check payloads carry no content
            }
            let stdin = expand(fx["stdin"].as_str().unwrap());
            assert_silent_ok(&hk.stored(client, &stdin, &cli), &name);
        }
    }
    for (client, payload) in &extra {
        assert_silent_ok(&hk.stored(client, payload.as_bytes(), &cli), "extra");
    }
    assert!(hk.wait_for(Duration::from_secs(5), || hk.inbox_names().is_empty()));
    assert!(hk.wait_for(Duration::from_secs(3), || hk.fold_processes() == 0));
    // Something was really folded and exported.
    assert!(
        hk.db()
            .int("SELECT COUNT(*) FROM agent_events WHERE outcome = 'folded'")
            > 40
    );
    assert!(hk.bar_state().is_some());
    // Make sure the WAL still holds what was written (no checkpoint is forced).
    let mut files = Vec::new();
    files_under(&hk.state, &mut files);
    let kept: Vec<&PathBuf> = files.iter().collect();
    assert!(kept.iter().any(|p| p.ends_with("codepet.sqlite3")));
    let forbidden = [
        "CANARY-",
        "raw-session-zz9",
        "raw-turn-zz9",
        "raw-tool-zz9",
        "raw-gsession-zz9",
        "raw-gtool-zz9",
        "5f0c2a1e-7b3d-4c9a-8e21-0d6b9a4f3c11",
        "01a13cfb-647c-7fd3-8500-3c7f8733825c",
        "toolu_01ABCdef",
        "call_abc",
        "AskUserQuestion",
        "/home/u",
        "Bash",
        "transcript",
    ];
    for path in &kept {
        let bytes = fs::read(path).unwrap();
        let name = path.to_string_lossy();
        for f in forbidden {
            assert!(!contains(&bytes, f), "{f:?} found in {name}");
            assert!(!name.contains(f), "{f:?} in a file name: {name}");
        }
    }
}
