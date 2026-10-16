//! Sanitized live metadata projections, not reconstructed raw client payloads.
//! Existing synthetic fixtures retain the complete schema/edge-case coverage.
mod hookkit;

use hookkit::{assert_silent_ok, Hk, CLIENTS};
use serde_json::Value;
use std::fs;
use std::path::Path;

fn evidence(client: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/live-2026-10-16")
        .join(format!("{client}.json"));
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn check_identity_projection(value: &Value) {
    match value {
        Value::String(s) => assert!(!s.starts_with("id-"), "original capture hash retained"),
        Value::Array(a) => a.iter().for_each(check_identity_projection),
        Value::Object(o) => {
            for (key, v) in o {
                assert!(
                    !matches!(
                        key.as_str(),
                        "prompt"
                            | "last_assistant_message"
                            | "tool_input"
                            | "tool_response"
                            | "conversation_history"
                            | "transcript_path"
                            | "cwd"
                    ),
                    "content field {key} retained"
                );
                if matches!(
                    key.as_str(),
                    "session_id"
                        | "sessionId"
                        | "sessionID"
                        | "turn_id"
                        | "promptId"
                        | "tool_use_id"
                        | "session"
                        | "parent"
                        | "part"
                ) && !v.is_null()
                {
                    assert!(v.as_str().unwrap().starts_with("qa-"), "{key}: {v}");
                }
                check_identity_projection(v);
            }
        }
        _ => {}
    }
}

#[test]
fn live_evidence_keeps_projection_limits_and_only_synthetic_ids() {
    for client in CLIENTS {
        let e = evidence(client);
        assert_eq!(e["format"], "codepet-live-projection-v1");
        assert_eq!(e["client"], client);
        assert_eq!(
            e["outcome"]["model_turn_completed"],
            client != "claude-code"
        );
        assert!(e["unverified"]
            .as_array()
            .unwrap()
            .contains(&Value::from("native_total_hook_latency")));
        check_identity_projection(&e);
        for capture in e["hook_captures"].as_array().unwrap() {
            assert_eq!(capture["input_over_limit"], false);
            assert_eq!(capture["shim_exit"], 0);
            assert!(capture["metadata"].get("codepet_v").is_none());
            assert!(capture["metadata"].get("tools").is_none());
        }
    }
    let claude = evidence("claude-code");
    assert_eq!(claude["outcome"]["outcome"], "weekly_limit");
    assert!(claude["hook_captures"].as_array().unwrap().iter().all(|c| {
        c["environment"]["claude_entrypoint"] == "cli" && c["metadata"]["hook_event_name"] != "Stop"
    }));
}

#[test]
fn native_hook_metadata_projections_reach_the_expected_normalization_branches() {
    // These expectations are CodePet's contract, not extra captured payload fields.
    let cases: [(&str, &[&str]); 3] = [
        (
            "claude-code",
            &["session_start", "turn_start", "turn_failed", "session_end"],
        ),
        (
            "codex",
            &["turn_start", "activity", "turn_done", "session_end"],
        ),
        (
            "grok-cli",
            &[
                "session_start",
                "session_end",
                "session_end",
                "session_start",
                "turn_start",
                "activity",
                "turn_done",
                "idle_prompt",
                "session_end",
                "session_end",
            ],
        ),
    ];
    for (client, expected) in cases {
        let e = evidence(client);
        let captures = e["hook_captures"].as_array().unwrap();
        assert_eq!(captures.len(), expected.len());
        for (capture, kind) in captures.iter().zip(expected) {
            let hk = Hk::new();
            hk.block_folds();
            let metadata = &capture["metadata"];
            let bytes = serde_json::to_vec(metadata).unwrap();
            let env = if client == "claude-code" {
                vec![("CLAUDE_CODE_ENTRYPOINT", "cli")]
            } else {
                vec![]
            };
            assert_silent_ok(&hk.hook(client, &bytes, &env), client);
            let got = hk.records();
            assert_eq!(got.len(), 1, "{client}: {metadata}");
            assert_eq!(got[0]["kind"], *kind);
            assert_eq!(got[0]["session"], metadata["session_id"]);
            assert!(got[0].get("tool_name").is_none());
            if client == "claude-code" {
                // prompt_id was not in the observer allowlist: never invent one.
                assert!(got[0]["turn"].is_null());
                if *kind == "turn_failed" {
                    assert_eq!(got[0]["note"], "rate_limit");
                }
            }
            if client == "codex" && *kind == "activity" {
                assert_eq!(metadata["tool_name"], "Bash");
                assert_eq!(got[0]["tool"], metadata["tool_use_id"]);
                assert_eq!(got[0]["turn"], metadata["turn_id"]);
            }
            if client == "grok-cli" && *kind == "turn_done" {
                assert_eq!(metadata["reason"], "end_turn");
                assert_eq!(got[0]["turn"], metadata["promptId"]);
            }
        }
    }
}

#[test]
fn incomplete_plugin_projections_are_not_misrepresented_as_raw_wire_fixtures() {
    for client in ["hermes", "opencode"] {
        let e = evidence(client);
        let hk = Hk::new();
        hk.block_folds();
        for capture in e["hook_captures"].as_array().unwrap() {
            // No codepet_v was retained. Do not add version=1 or a guessed tool count.
            let bytes = serde_json::to_vec(&capture["metadata"]).unwrap();
            assert_silent_ok(&hk.hook(client, &bytes, &[]), client);
        }
        assert!(
            hk.records().is_empty(),
            "{client}: incomplete projection accepted"
        );
    }
}
