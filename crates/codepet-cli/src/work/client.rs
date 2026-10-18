//! Versioned owned-work clients. Observer connections never select an executor.
use super::{io, read_limit, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "extra_clients.rs"]
mod extra_clients;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ClientId {
    #[default]
    Codex,
    ClaudeCode,
    #[serde(rename = "opencode")]
    OpenCode,
    Muse,
    Grok,
    Hermes,
    Pi,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum AuthRoute {
    #[default]
    CodexChatgpt,
    ClaudeSubscription,
    OpenCodeBigPickleFree,
}
pub(super) struct Resolved {
    pub binary: PathBuf,
    pub credentials: Vec<u8>,
    pub route: AuthRoute,
}
impl ClientId {
    pub fn name(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::ClaudeCode => "Claude Code",
            Self::OpenCode => "OpenCode",
            Self::Muse => "Muse",
            Self::Grok => "Grok",
            Self::Hermes => "Hermes",
            Self::Pi => "Pi",
        }
    }
    pub fn version(self) -> &'static str {
        match self {
            Self::Codex => "0.163.0",
            Self::ClaudeCode => "2.1.294",
            Self::OpenCode => "1.19.0",
            _ => extra_clients::version(self),
        }
    }
}
pub(super) const CLIENTS: [ClientId; 7] = [
    ClientId::Codex,
    ClientId::ClaudeCode,
    ClientId::OpenCode,
    ClientId::Muse,
    ClientId::Grok,
    ClientId::Hermes,
    ClientId::Pi,
];

// A shell launcher may update software or load user hooks. Only a native executable
// whose version is subsequently checked inside containment can run an owned task.
pub(super) fn executable(name: &str, version: &str) -> Result<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(
            PathBuf::from(home)
                .join(".local/share/mise/installs")
                .join(name)
                .join(version)
                .join(name),
        );
    }
    candidates.extend(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).map(|p| p.join(name)),
    );
    for path in candidates {
        let Ok(path) = std::fs::canonicalize(path) else {
            continue;
        };
        let Ok(mut file) = super::checked_file(&path) else {
            continue;
        };
        use std::io::Read;
        let mut magic = [0; 4];
        if file.read_exact(&mut magic).is_ok() && magic == *b"\x7fELF" {
            return Ok(path);
        }
    }
    let display = match name {
        "codex" => "Codex",
        "claude" => "Claude Code",
        "opencode" => "OpenCode",
        _ => name,
    };
    Err(format!(
        "Install {display} {version} native client first; automatic update launchers are not used"
    ))
}
pub(super) fn home_file(directory: &str, filename: &str) -> Result<PathBuf> {
    Ok(
        PathBuf::from(std::env::var_os("HOME").ok_or("No home directory")?)
            .join(directory)
            .join(filename),
    )
}
pub(super) fn resolve(id: ClientId) -> Result<Resolved> {
    if !Path::new("/usr/bin/bwrap").is_file() || !Path::new("/usr/bin/prlimit").is_file() {
        return Err("Bubblewrap and prlimit are needed to keep jobs contained".into());
    }
    match id {
        ClientId::Codex => {
            let binary = executable("codex", id.version())?;
            let auth = std::env::var_os("CODEX_HOME")
                .map(PathBuf::from)
                .map(|p| p.join("auth.json"))
                .unwrap_or(home_file(".codex", "auth.json")?);
            let v: Value = serde_json::from_slice(&read_limit(&auth, 128 * 1024)?)
                .map_err(|_| "Codex sign-in could not be read")?;
            Ok(Resolved {
                binary,
                credentials: codex_credentials(&v)?,
                route: AuthRoute::CodexChatgpt,
            })
        }
        ClientId::ClaudeCode => {
            let binary = executable("claude", id.version())?;
            let path = std::env::var_os("CLAUDE_CONFIG_DIR")
                .map(PathBuf::from)
                .map(|p| p.join(".credentials.json"))
                .unwrap_or(home_file(".claude", ".credentials.json")?);
            let v: Value = serde_json::from_slice(&read_limit(&path, 128 * 1024)?)
                .map_err(|_| "Claude subscription sign-in could not be read")?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(io)?
                .as_millis() as u64;
            Ok(Resolved {
                binary,
                credentials: claude_credentials(&v["claudeAiOauth"], now)?,
                route: AuthRoute::ClaudeSubscription,
            })
        }
        ClientId::OpenCode => Ok(Resolved {
            binary: executable("opencode", id.version())?,
            credentials: Vec::new(),
            route: AuthRoute::OpenCodeBigPickleFree,
        }),
        _ => extra_clients::resolve(id),
    }
}
fn codex_credentials(auth: &Value) -> Result<Vec<u8>> {
    if auth["auth_mode"] != "chatgpt" || !auth["tokens"].is_object() {
        return Err("Sign into Codex with your ChatGPT subscription; API-key billing is not used by CodePet".into());
    }
    let mut clean = json!({"auth_mode":"chatgpt","tokens":auth["tokens"]});
    // Preserve native refresh scheduling without copying unrelated root fields.
    if let Some(value) = auth.get("last_refresh").filter(|v| !v.is_null()) {
        if !value.is_string() {
            return Err("Codex sign-in refresh metadata could not be read".into());
        }
        clean["last_refresh"] = value.clone();
    }
    serde_json::to_vec(&clean).map_err(io)
}

fn claude_credentials(oauth: &Value, now: u64) -> Result<Vec<u8>> {
    if !matches!(oauth["subscriptionType"].as_str(), Some("max" | "pro"))
        || oauth["accessToken"].as_str().is_none_or(|s| s.is_empty())
    {
        return Err("Sign into Claude Code with your Claude subscription first".into());
    }
    // Never give a job the refresh token: rotating a copied token could
    // invalidate the user's original subscription login.
    if oauth["expiresAt"]
        .as_u64()
        .is_none_or(|expiry| expiry <= now.saturating_add(25 * 60 * 1000))
    {
        return Err("Refresh your Claude Code subscription sign-in before starting; its access token must remain valid for this bounded job".into());
    }
    let mut clean = serde_json::Map::new();
    for key in [
        "accessToken",
        "expiresAt",
        "scopes",
        "subscriptionType",
        "rateLimitTier",
    ] {
        if let Some(value) = oauth.get(key) {
            clean.insert(key.into(), value.clone());
        }
    }
    serde_json::to_vec(&json!({"claudeAiOauth":clean})).map_err(io)
}
pub(super) fn auth_filename(id: ClientId) -> &'static str {
    match id {
        ClientId::Codex => "auth.json",
        ClientId::ClaudeCode => ".credentials.json",
        ClientId::OpenCode => "auth.json",
        _ => "unqualified",
    }
}
pub(super) fn command(id: ClientId, kind: &str) -> Command {
    let mut c = Command::new("/program/client");
    if id == ClientId::ClaudeCode {
        let tools = if super::readonly(kind) {
            "Read,Glob,Grep,WebSearch"
        } else {
            "Read,Glob,Grep,Edit,Write,WebSearch"
        };
        c.args([
            "--print",
            "--restricted",
            "--safe-mode",
            "--setting-sources",
            "",
            "--strict-mcp-config",
            "--mcp-config",
            "{\"mcpServers\":{}}",
            "--disable-slash-commands",
            "--no-chrome",
            "--no-session-persistence",
            "--permission-mode",
            "dontAsk",
            "--permission-prompts",
            "none",
            "--tools",
            tools,
            "--allowedTools",
            tools,
            "--output-format",
            "stream-json",
            "--verbose",
        ])
        .env("CLAUDE_CONFIG_DIR", "/provider")
        .env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1");
    }
    if id == ClientId::OpenCode {
        c.args([
            "run",
            "--pure",
            "--format",
            "json",
            "--model",
            "opencode/big-pickle",
            "--agent",
            "codepet",
        ])
        .env("XDG_CONFIG_HOME", "/provider/config")
        .env("XDG_DATA_HOME", "/provider/data")
        .env("XDG_CACHE_HOME", "/provider/cache")
        .env("XDG_STATE_HOME", "/provider/state")
        .env("OPENCODE_PURE", "1")
        .env("OPENCODE_DISABLE_PROJECT_CONFIG", "1")
        .env("OPENCODE_DISABLE_CLAUDE_CODE", "1")
        .env("OPENCODE_DISABLE_MODELS_FETCH", "1")
        .env("OPENCODE_CONFIG_CONTENT", opencode_config(kind).to_string());
    }
    c
}
fn opencode_config(kind: &str) -> Value {
    let mut permissions = json!({"*":"deny","read":"allow","glob":"allow","grep":"allow","bash":"allow","webfetch":"allow","websearch":"allow"});
    if !super::readonly(kind) {
        permissions["edit"] = json!("allow");
        permissions["write"] = json!("allow");
        permissions["apply_patch"] = json!("allow");
    }
    json!({"autoupdate":false,"share":"disabled","plugin":[],"mcp":{},"formatter":false,"lsp":false,"enabled_providers":["opencode"],"model":"opencode/big-pickle","small_model":"opencode/big-pickle","default_agent":"codepet","agent":{"codepet":{"mode":"primary","permission":permissions},"title":{"disable":true},"summary":{"disable":true}}})
}
pub(super) fn normalize_event(id: ClientId, event: &Value) -> Vec<Value> {
    if id == ClientId::Codex
        || event["type"]
            .as_str()
            .is_some_and(|s| s.starts_with("codepet."))
    {
        return vec![event.clone()];
    }
    if id == ClientId::ClaudeCode {
        return match event["type"].as_str() {
            Some("result")
                if event["is_error"] == false
                    && event["subtype"] == "success"
                    && event["result"]
                        .as_str()
                        .is_some_and(|s| !s.trim().is_empty()) =>
            {
                vec![
                    json!({"type":"item.completed","item":{"type":"agent_message","text":event["result"]}}),
                    json!({"type":"turn.completed"}),
                ]
            }
            Some("result") => {
                let reason = event["errors"]
                    .as_array()
                    .and_then(|v| v.iter().find_map(Value::as_str))
                    .or_else(|| event["result"].as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or("Claude could not complete this job");
                vec![json!({"type":"turn.failed","message":reason})]
            }
            Some("assistant") => vec![json!({"type":"item.started"})],
            _ => vec![],
        };
    }
    if id == ClientId::OpenCode {
        return match event["type"].as_str() {
            Some("text") if event["part"]["text"].is_string() => vec![
                json!({"type":"item.completed","item":{"type":"agent_message","text":event["part"]["text"]}}),
            ],
            Some("step_finish") if event["part"]["reason"] == "stop" => {
                vec![json!({"type":"turn.completed"})]
            }
            Some("error") => vec![
                json!({"type":"turn.failed","message":event["error"]["data"]["message"].as_str().unwrap_or("OpenCode could not complete this job")}),
            ],
            Some("tool_use") | Some("step_start") => vec![json!({"type":"item.started"})],
            _ => vec![],
        };
    }
    vec![]
}

pub(super) fn detail(id: ClientId) -> &'static str {
    match id {
        ClientId::Codex => "Isolated commands and reviewed code changes.",
        ClientId::ClaudeCode => "File edits only; command-based checks are not run.",
        ClientId::OpenCode => {
            "Free Big Pickle model; isolated commands; no account credentials or fallback."
        }
        _ => "Execution is unavailable until this client's isolation is qualified.",
    }
}
pub(super) fn availability(id: ClientId) -> Value {
    if !matches!(
        id,
        ClientId::Codex | ClientId::ClaudeCode | ClientId::OpenCode
    ) {
        let (status, reason) = extra_clients::availability(id);
        return json!({"id":id,"name":id.name(),"available":false,"status":status,"reason":reason,"detail":detail(id)});
    }
    let resolved = resolve(id);
    json!({"id":id,"name":id.name(),"available":resolved.is_ok(),"status":if resolved.is_ok(){"ready"}else{"setup-required"},"reason":resolved.err().unwrap_or_else(||"Client detected; isolated version and sign-in checked when work starts.".into()),"detail":detail(id)})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_client_ids_roundtrip_and_old_tasks_default_to_codex() {
        for (id, text) in CLIENTS.into_iter().zip([
            "codex",
            "claude-code",
            "opencode",
            "muse",
            "grok",
            "hermes",
            "pi",
        ]) {
            assert_eq!(serde_json::to_value(id).unwrap(), text);
            assert_eq!(serde_json::from_value::<ClientId>(json!(text)).unwrap(), id);
        }
        assert_eq!(ClientId::default(), ClientId::Codex);
    }
    #[test]
    fn codex_snapshot_preserves_optional_refresh_metadata_only() {
        let mut auth = json!({"auth_mode":"chatgpt","tokens":{"access_token":"dummy"},"last_refresh":"2026-10-18T00:00:00Z","OPENAI_API_KEY":"never-copy","other":"never-copy"});
        let clean: Value = serde_json::from_slice(&codex_credentials(&auth).unwrap()).unwrap();
        assert_eq!(
            clean,
            json!({"auth_mode":"chatgpt","tokens":{"access_token":"dummy"},"last_refresh":"2026-10-18T00:00:00Z"})
        );
        auth.as_object_mut().unwrap().remove("last_refresh");
        assert!(
            serde_json::from_slice::<Value>(&codex_credentials(&auth).unwrap())
                .unwrap()
                .get("last_refresh")
                .is_none()
        );
        auth["last_refresh"] = json!(42);
        assert!(codex_credentials(&auth).is_err());
        auth["auth_mode"] = json!("apikey");
        assert!(codex_credentials(&auth).is_err());
    }
    #[test]
    fn claude_snapshot_never_copies_refresh_or_other_provider_authority() {
        let mut oauth = json!({"accessToken":"dummy-access","refreshToken":"never-copy","expiresAt":2_000_000,"scopes":["user:inference"],"subscriptionType":"max","otherCredential":"never-copy"});
        let clean: Value = serde_json::from_slice(&claude_credentials(&oauth, 0).unwrap()).unwrap();
        assert_eq!(clean["claudeAiOauth"]["accessToken"], "dummy-access");
        assert!(clean["claudeAiOauth"].get("refreshToken").is_none());
        assert!(clean["claudeAiOauth"].get("otherCredential").is_none());
        assert!(claude_credentials(&oauth, 500_000).is_err());
        oauth["expiresAt"] = json!("not-an-expiry");
        assert!(claude_credentials(&oauth, 0).is_err());
        oauth["expiresAt"] = json!(2_000_000);
        oauth["subscriptionType"] = json!("api");
        assert!(claude_credentials(&oauth, 0).is_err());
    }
    #[test]
    fn native_claude_restricted_tools_keep_dummy_oauth_private() {
        native_gate(
            ClientId::ClaudeCode,
            "CODEPET_TEST_CLAUDE",
            "claude-work-gate.py",
        );
    }
    #[test]
    fn native_opencode_free_route_has_no_private_credentials() {
        native_gate(
            ClientId::OpenCode,
            "CODEPET_TEST_OPENCODE",
            "opencode-work-gate.py",
        );
    }
    fn native_gate(id: ClientId, variable: &str, script: &str) {
        let Some(binary) = std::env::var_os(variable) else {
            eprintln!("Set {variable} to qualify native tools");
            return;
        };
        let fixture = tempfile::tempdir().unwrap();
        let command = command(id, "bug");
        let args: Vec<_> = command.get_args().map(|v| v.to_str().unwrap()).collect();
        let env: std::collections::BTreeMap<_, _> = command
            .get_envs()
            .map(|(k, v)| (k.to_str().unwrap(), v.unwrap().to_str().unwrap()))
            .collect();
        let policy = fixture.path().join("policy.json");
        std::fs::write(
            &policy,
            serde_json::to_vec(&json!({"args":args,"env":env})).unwrap(),
        )
        .unwrap();
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tools/integrations")
            .join(script);
        let mut check = Command::new("/usr/bin/python3");
        check
            .arg(script)
            .arg("--binary")
            .arg(binary)
            .arg("--policy")
            .arg(policy)
            .arg("--output")
            .arg(fixture.path().join("gate"));
        if id == ClientId::OpenCode {
            // Native pure/config flags still import local plugin modules. The
            // real staging filter, not those flags, excludes user configuration.
            let source = fixture.path().join("source");
            std::fs::create_dir(&source).unwrap();
            std::fs::write(
                source.join("trap.js"),
                "require('fs').writeFileSync('/work/PLUGIN_RAN','bad')",
            )
            .unwrap();
            for parent in ["", "nested"] {
                let dir = source.join(parent);
                std::fs::create_dir_all(dir.join(".opencode/plugins")).unwrap();
                std::fs::write(dir.join("opencode.json"), r#"{"plugin":["file:///work/trap.js"],"mcp":{"trap":{"type":"local","command":["sh","-c","touch /work/MCP_RAN"]}}}"#).unwrap();
                std::fs::write(
                    dir.join("opencode.jsonc"),
                    "invalid configuration must be excluded",
                )
                .unwrap();
                std::fs::write(
                    dir.join(".opencode/plugins/trap.js"),
                    "require('fs').writeFileSync('/work/PLUGIN_RAN','bad')",
                )
                .unwrap();
            }
            let staged = fixture.path().join("staged");
            let baseline = super::super::walk(&source).unwrap();
            assert_eq!(baseline.keys().cloned().collect::<Vec<_>>(), ["trap.js"]);
            super::super::copy_tree(&source, &staged, &baseline).unwrap();
            check.arg("--staged").arg(staged);
        }
        let output = check.output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        eprintln!("{}", String::from_utf8_lossy(&output.stdout));
    }
    #[test]
    fn completion_is_client_specific_and_never_inferred_from_progress() {
        assert!(
            normalize_event(ClientId::ClaudeCode, &json!({"type":"assistant"}))
                .iter()
                .all(|e| e["type"] != "turn.completed")
        );
        assert!(normalize_event(
            ClientId::ClaudeCode,
            &json!({"type":"result","subtype":"success","is_error":false,"result":"report"})
        )
        .iter()
        .any(|e| e["type"] == "turn.completed"));
        assert_eq!(
            normalize_event(
                ClientId::ClaudeCode,
                &json!({"type":"result","subtype":"error_max_turns","is_error":true})
            )[0]["type"],
            "turn.failed"
        );
        assert_eq!(
            normalize_event(
                ClientId::ClaudeCode,
                &json!({"type":"result","is_error":true,"errors":["Subscription limit reached"]})
            )[0]["message"],
            "Subscription limit reached"
        );
        for result in [Value::Null, json!(42), json!("  ")] {
            assert_eq!(
                normalize_event(
                    ClientId::ClaudeCode,
                    &json!({"type":"result","subtype":"success","is_error":false,"result":result})
                )[0]["type"],
                "turn.failed"
            );
        }
        assert!(normalize_event(
            ClientId::OpenCode,
            &json!({"type":"step_finish","part":{"reason":"tool-calls"}})
        )
        .is_empty());
        assert_eq!(
            normalize_event(
                ClientId::OpenCode,
                &json!({"type":"step_finish","part":{"reason":"stop"}})
            )[0]["type"],
            "turn.completed"
        );
    }
}
