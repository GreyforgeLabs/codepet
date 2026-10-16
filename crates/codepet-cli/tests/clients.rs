//! WP3 end to end (SPEC-v0.3 §5.2-§5.7, §6.2, §9.2 "Installers"): `clients.status`,
//! `clients.connect`, `clients.disconnect`, `clients.test`, the detached clients job, the
//! ready check through the real stored commands, `codepet connect|disconnect|doctor`,
//! against a throwaway HOME with client stand-ins (`tests/clientkit`). No real client runs,
//! no real client config is read or written, and nothing touches the owner's state.
mod clientkit;
use clientkit::{python_has_yaml, snapshot, Home, COMMANDS};
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Stdio;

fn commands() -> Value {
    serde_json::from_str(COMMANDS).unwrap()
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap() + "\n"
}

#[test]
fn status_on_an_empty_home_lists_five_clients_and_runs_nothing() {
    let h = Home::new();
    let s = h.ok("clients.status", json!({}));
    let list = s["clients"].as_array().unwrap();
    let ids: Vec<&str> = list.iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(
        ids,
        ["claude-code", "codex", "opencode", "hermes", "grok-cli"]
    );
    let names: Vec<&str> = list.iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        ["Claude Code", "Codex", "OpenCode", "Hermes", "Grok"]
    );
    for c in list {
        assert_eq!(c["installed"], false);
        assert_eq!(c["connected"], false);
        assert_eq!(c["events_7d"], 0);
        assert_eq!(c["job"], Value::Null);
        let keys: Vec<&String> = c.as_object().unwrap().keys().collect();
        assert_eq!(keys.len(), 11, "{c}");
    }
    // The export now carries one record per client, shaped like bar.v2.schema.json's
    // client entry; the status entry adds name and events_7d (protocol.json).
    let bar = h.bar();
    assert_eq!(bar["clients"].as_object().unwrap().len(), 5);
    let schema: Value = serde_json::from_str(include_str!(
        "../../../docs/v0.3/contracts/bar.v2.schema.json"
    ))
    .unwrap();
    let mut want: Vec<&str> = schema["$defs"]["client"]["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    want.sort();
    for (id, entry) in bar["clients"].as_object().unwrap() {
        let mut got: Vec<&str> = entry
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        got.sort();
        assert_eq!(got, want, "{id}");
    }
    let protocol: Value =
        serde_json::from_str(include_str!("../../../docs/v0.3/contracts/protocol.json")).unwrap();
    let mut want: Vec<&str> = protocol["result_schemas"]["clients.status"]["$defs"]
        ["client_status"]["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    want.sort();
    for c in list {
        let mut got: Vec<&str> = c.as_object().unwrap().keys().map(String::as_str).collect();
        got.sort();
        assert_eq!(got, want);
    }
    assert!(!h.data_dir().exists(), "status writes no client artifact");
}

#[test]
fn status_detects_every_installed_client_from_files_only() {
    let h = Home::new();
    h.install_claude();
    h.install_codex("0.163.0");
    h.install_opencode();
    h.install_hermes();
    h.install_grok();
    let s = h.ok("clients.status", json!({}));
    for c in s["clients"].as_array().unwrap() {
        assert_eq!(c["installed"], true, "{c}");
    }
    let v = |id: &str| {
        s["clients"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == id)
            .unwrap()["version"]
            .clone()
    };
    assert_eq!(v("codex"), "0.163.0");
    assert_eq!(v("opencode"), "1.19.0");
    assert_eq!(v("hermes"), "0.21.3");
    assert_eq!(v("grok-cli"), "1.0.52");
    // Claude's version is not in a file here; status never runs `claude --version`.
    assert_eq!(v("claude-code"), Value::Null);
    assert!(h.calls("claude-calls.log").is_empty() && h.calls("hermes-calls.log").is_empty());
    assert_eq!(h.unexpected_calls(), "");
}

fn claude_settings() -> Value {
    json!({
        "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "slopbrake-hook stop", "timeout": 600}]}],
                  "SessionStart": [{"matcher": "startup", "hooks": [{"type": "command", "command": "bash herdr.sh session"}]}]},
        "permissions": {"allow": ["Bash(ls)"]},
        "enabledPlugins": {"vercel@claude-plugins-official": true},
        "theme": "dark"
    })
}

#[test]
fn claude_code_connect_ready_check_idempotence_and_disconnect() {
    let h = Home::new();
    h.install_claude();
    let settings = h.home().join(".claude/settings.json");
    fs::write(&settings, pretty(&claude_settings())).unwrap();
    let j = h.connect(&["claude-code"]);
    let r = h.result_of(&j, "claude-code");
    assert_eq!(r["outcome"], "connected", "{j}");
    assert_eq!(
        j["test"], "done",
        "the ready check went through the real stored command: {j}"
    );
    // The shim: the frozen template with this home's paths, 0755.
    let shim = fs::read_to_string(h.shim()).unwrap();
    assert!(shim.contains(&format!("S='{}'", h.state_dir().display())));
    assert!(shim.contains(&format!("B='{}'", clientkit::BIN)));
    assert_eq!(
        fs::metadata(h.shim()).unwrap().permissions().mode() & 0o777,
        0o755
    );
    // The marketplace: hooks.json is the frozen file with this shim path.
    let m = h.data_dir().join("integrations/claude-code");
    let hooks = fs::read_to_string(m.join("plugin/hooks/hooks.json")).unwrap();
    let tpl = include_str!("../../../integrations/claude-code/plugin/hooks/hooks.json.in");
    assert_eq!(
        hooks,
        tpl.replace("@CODEPET_SHIM@", h.shim().to_str().unwrap())
    );
    let want: Value = serde_json::from_str(
        &h.with_shim(&commands()["clients"]["claude-code"]["file"].to_string()),
    )
    .unwrap();
    assert_eq!(serde_json::from_str::<Value>(&hooks).unwrap(), want);
    let plugin: Value = serde_json::from_str(
        &fs::read_to_string(m.join("plugin/.claude-plugin/plugin.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(plugin["name"], "codepet");
    assert_eq!(plugin["version"], env!("CARGO_PKG_VERSION"));
    // Claude Code's own CLI recorded it; the foreign keys are unchanged.
    let after: Value = serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
    assert_eq!(after["enabledPlugins"]["codepet@codepet-local"], true);
    assert_eq!(
        after["extraKnownMarketplaces"]["codepet-local"]["source"]["path"],
        m.to_str().unwrap()
    );
    for k in ["hooks", "permissions", "theme"] {
        assert_eq!(after[k], claude_settings()[k], "{k}");
    }
    assert_eq!(
        after["enabledPlugins"]["vercel@claude-plugins-official"],
        true
    );
    // The CLI ran under the client CLI rules: a minimal environment.
    let calls = h.calls("claude-calls.log");
    let install = calls.iter().find(|c| c["argv"][1] == "install").unwrap();
    assert_eq!(
        install["argv"],
        json!([
            "plugin",
            "install",
            "codepet",
            "--marketplace",
            m.to_str().unwrap(),
            "--json"
        ])
    );
    for name in install["env"].as_array().unwrap() {
        let name = name.as_str().unwrap();
        assert!(
            [
                "PATH",
                "HOME",
                "DISABLE_AUTOUPDATER",
                "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
                "LC_CTYPE"
            ]
            .contains(&name),
            "{name}"
        );
    }
    // Export and status: connected, waiting for the first real event.
    let st = h.status_of("claude-code");
    assert_eq!(
        (
            st["connected"].clone(),
            st["first_seen_ms"].clone(),
            st["version"].clone()
        ),
        (json!(true), Value::Null, Value::Null)
    );
    let bar = h.bar();
    assert_eq!(bar["clients"]["claude-code"]["connected"], true);
    assert_eq!(bar["clients"]["claude-code"]["job"], Value::Null);
    assert_eq!(bar["agent"]["test"]["state"], "done");
    assert_eq!(bar["agent"]["test"]["client"], "claude-code");
    assert!(!h.state_dir().join("test.nonce").exists());

    // Connecting again is byte-identical and does not run the CLI again.
    let before = (snapshot(&h.data_dir()), fs::read(&settings).unwrap());
    let n_calls = h.calls("claude-calls.log").len();
    let j = h.connect(&["claude-code"]);
    assert_eq!(h.result_of(&j, "claude-code")["outcome"], "connected");
    assert_eq!(
        (snapshot(&h.data_dir()), fs::read(&settings).unwrap()),
        before
    );
    assert_eq!(
        h.calls("claude-calls.log").len(),
        n_calls + 1,
        "only `--version`"
    );

    // Disconnect: the flag first, then Claude Code's own uninstall; the rest is unchanged.
    let j = h.disconnect("claude-code");
    assert_eq!(
        h.result_of(&j, "claude-code")["outcome"],
        "disconnected",
        "{j}"
    );
    assert!(h
        .state_dir()
        .join("clients/claude-code.disconnected")
        .is_file());
    assert!(!m.exists());
    let gone: Value = serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
    assert_eq!(
        gone["enabledPlugins"],
        json!({"vercel@claude-plugins-official": true})
    );
    assert_eq!(gone["extraKnownMarketplaces"], json!({}));
    for k in ["hooks", "permissions", "theme"] {
        assert_eq!(gone[k], claude_settings()[k], "{k}");
    }
    assert_eq!(h.status_of("claude-code")["connected"], false);
    // Disconnecting again is a no-op that still succeeds.
    let j = h.disconnect("claude-code");
    assert_eq!(h.result_of(&j, "claude-code")["outcome"], "disconnected");
}

#[test]
fn claude_code_settings_changes_beyond_codepets_keys_fail_the_connect() {
    let h = Home::new();
    h.install_claude();
    fs::write(
        h.home().join(".claude/settings.json"),
        pretty(&claude_settings()),
    )
    .unwrap();
    fs::write(h.home().join("claude-tamper"), "").unwrap();
    let j = h.connect(&["claude-code"]);
    let r = h.result_of(&j, "claude-code");
    assert_eq!(
        (r["outcome"].clone(), r["reason"].clone()),
        (json!("failed"), json!("settings_changed")),
        "{j}"
    );
    let st = h.status_of("claude-code");
    assert_eq!(st["failed_reason"], "settings_changed");
    assert_eq!(
        h.bar()["clients"]["claude-code"]["failed_reason"],
        "settings_changed"
    );
    // A policy refusal is `blocked`.
    let h = Home::new();
    h.install_claude();
    fs::write(h.home().join("claude-fail"), "").unwrap();
    let j = h.connect(&["claude-code"]);
    assert_eq!(h.result_of(&j, "claude-code")["reason"], "blocked", "{j}");
    // Unreadable settings: nothing runs.
    let h = Home::new();
    h.install_claude();
    fs::write(h.home().join(".claude/settings.json"), "{ nope").unwrap();
    let j = h.connect(&["claude-code"]);
    assert_eq!(h.result_of(&j, "claude-code")["reason"], "blocked");
    assert!(!h
        .calls("claude-calls.log")
        .iter()
        .any(|c| c["argv"][1] == "install"));
    assert_eq!(
        fs::read_to_string(h.home().join(".claude/settings.json")).unwrap(),
        "{ nope"
    );
}

#[test]
fn grok_and_opencode_files_are_idempotent_and_leave_foreign_files_alone() {
    let h = Home::new();
    h.install_grok();
    h.install_opencode();
    let gdir = h.home().join(".grok/hooks");
    fs::create_dir_all(&gdir).unwrap();
    fs::write(
        gdir.join("herdr.json"),
        "{\"hooks\":{\"SessionStart\":[]}}\n",
    )
    .unwrap();
    fs::write(gdir.join(".codepet.json.partial"), "stale").unwrap();
    let odir = h.home().join(".config/opencode");
    fs::create_dir_all(odir.join("plugins")).unwrap();
    for (n, t) in [
        ("plugins/herdr-agent-state.js", "// herdr\n"),
        ("plugins/slopbrake.js", "// slop\n"),
        ("opencode.json", "{\"autoupdate\":false}\n"),
        ("package.json", "{}\n"),
    ] {
        fs::write(odir.join(n), t).unwrap();
    }
    fs::write(odir.join("plugins/.codepet.js.partial"), "stale").unwrap();
    let foreign_g = fs::read(gdir.join("herdr.json")).unwrap();
    let foreign_o: Vec<_> = [
        "plugins/herdr-agent-state.js",
        "plugins/slopbrake.js",
        "opencode.json",
        "package.json",
    ]
    .iter()
    .map(|n| fs::read(odir.join(n)).unwrap())
    .collect();
    let j = h.connect(&["opencode", "grok-cli"]);
    assert_eq!(h.result_of(&j, "grok-cli")["outcome"], "connected", "{j}");
    assert_eq!(h.result_of(&j, "opencode")["outcome"], "connected", "{j}");
    assert_eq!(
        j["test"], "done",
        "argv (OpenCode) and shell (Grok) ready checks: {j}"
    );
    let grok = fs::read_to_string(gdir.join("codepet.json")).unwrap();
    let tpl = include_str!("../../../integrations/grok-cli/codepet.json.in");
    assert_eq!(
        grok,
        tpl.replace("@CODEPET_SHIM@", h.shim().to_str().unwrap())
    );
    let want: Value =
        serde_json::from_str(&h.with_shim(&commands()["clients"]["grok-cli"]["file"].to_string()))
            .unwrap();
    assert_eq!(serde_json::from_str::<Value>(&grok).unwrap(), want);
    let js = fs::read_to_string(odir.join("plugins/codepet.js")).unwrap();
    assert!(js.starts_with("// managed by codepet"));
    assert!(js.contains(&format!("const SHIM = \"{}\";", h.shim().display())));
    // Temp names never match a loaded glob, and stale ones are swept.
    for d in [&gdir, &odir.join("plugins")] {
        for e in fs::read_dir(d).unwrap() {
            let n = e.unwrap().file_name().into_string().unwrap();
            assert!(!n.contains(".partial"), "{n}");
        }
    }
    let mtimes = (
        fs::metadata(gdir.join("codepet.json"))
            .unwrap()
            .modified()
            .unwrap(),
        fs::metadata(odir.join("plugins/codepet.js"))
            .unwrap()
            .modified()
            .unwrap(),
    );
    h.connect(&["opencode", "grok-cli"]);
    assert_eq!(
        (
            fs::metadata(gdir.join("codepet.json"))
                .unwrap()
                .modified()
                .unwrap(),
            fs::metadata(odir.join("plugins/codepet.js"))
                .unwrap()
                .modified()
                .unwrap()
        ),
        mtimes,
        "a second Connect rewrites nothing"
    );
    for id in ["grok-cli", "opencode"] {
        assert_eq!(
            h.result_of(&h.disconnect(id), id)["outcome"],
            "disconnected"
        );
    }
    assert!(!gdir.join("codepet.json").exists() && !odir.join("plugins/codepet.js").exists());
    assert_eq!(fs::read(gdir.join("herdr.json")).unwrap(), foreign_g);
    let after: Vec<_> = [
        "plugins/herdr-agent-state.js",
        "plugins/slopbrake.js",
        "opencode.json",
        "package.json",
    ]
    .iter()
    .map(|n| fs::read(odir.join(n)).unwrap())
    .collect();
    assert_eq!(after, foreign_o);
    assert_eq!(
        h.unexpected_calls(),
        "",
        "no Grok or OpenCode binary ever ran"
    );
}

#[test]
fn a_foreign_codepet_named_file_is_never_clobbered() {
    let h = Home::new();
    h.install_grok();
    let gdir = h.home().join(".grok/hooks");
    fs::create_dir_all(&gdir).unwrap();
    fs::write(
        gdir.join("codepet.json"),
        "{\"hooks\":{\"Stop\":[{\"hooks\":[{\"type\":\"command\",\"command\":\"mine\"}]}]}}",
    )
    .unwrap();
    let j = h.connect(&["grok-cli"]);
    assert_eq!(h.result_of(&j, "grok-cli")["reason"], "blocked");
    assert!(fs::read_to_string(gdir.join("codepet.json"))
        .unwrap()
        .contains("\"mine\""));
}

#[test]
fn hermes_plugin_dir_and_config_key_check() {
    if !python_has_yaml() {
        eprintln!("skipped: python3 has no yaml module for the Hermes stand-in");
        return;
    }
    let h = Home::new();
    h.install_hermes();
    let cfg = h.home().join(".hermes/config.yaml");
    fs::write(&cfg, "# my config\nmodel:\n  default: m1\nplugins:\n  enabled:\n  - herdr-agent-state\nhooks:\n  pre_tool_call:\n  - command: echo hi\n    timeout: 5\n").unwrap();
    let j = h.connect(&["hermes"]);
    assert_eq!(h.result_of(&j, "hermes")["outcome"], "connected", "{j}");
    assert_eq!(j["test"], "done", "{j}");
    let dir = h.home().join(".hermes/plugins/codepet");
    assert!(fs::read_to_string(dir.join("plugin.yaml"))
        .unwrap()
        .contains("managed by codepet"));
    let init = fs::read_to_string(dir.join("__init__.py")).unwrap();
    assert!(init.contains(&format!("_SHIM = \"{}\"", h.shim().display())));
    let text = fs::read_to_string(&cfg).unwrap();
    assert!(text.contains("codepet") && text.contains("echo hi"));
    let calls = h.calls("hermes-calls.log");
    assert_eq!(calls.len(), 1, "never `hermes --version`: {calls:?}");
    assert_eq!(calls[0]["argv"], json!(["plugins", "enable", "codepet"]));
    assert!(
        !h.home()
            .join(".local/share/codepet/staging")
            .read_dir()
            .unwrap()
            .any(|_| true),
        "staging is empty"
    );
    // Again: already enabled, so the CLI does not run.
    h.connect(&["hermes"]);
    assert_eq!(h.calls("hermes-calls.log").len(), 1);
    let j = h.disconnect("hermes");
    assert_eq!(h.result_of(&j, "hermes")["outcome"], "disconnected", "{j}");
    assert!(!dir.exists());
    assert_eq!(
        h.calls("hermes-calls.log")[1]["argv"],
        json!(["plugins", "disable", "codepet"])
    );
    // A CLI that changes a hooks/plugins key it should not: settings_changed.
    let h = Home::new();
    h.install_hermes();
    fs::write(h.home().join("hermes-tamper"), "").unwrap();
    let j = h.connect(&["hermes"]);
    assert_eq!(
        h.result_of(&j, "hermes")["reason"],
        "settings_changed",
        "{j}"
    );
}

const CODEX_HOOKS: &str = r#"{
  "hooks": {
    "SessionStart": [{"matcher": "startup", "hooks": [{"type": "command", "command": "bash herdr-agent-state.sh session", "timeout": 10}]}],
    "PreToolUse": [{"matcher": "^Bash$", "hooks": [{"type": "command", "command": "slopbrake-hook pre-tool-use"}]}],
    "PostToolUse": [{"hooks": [{"type": "command", "command": "slopbrake-hook post-tool-use"}]}],
    "Stop": [{"hooks": [{"type": "command", "command": "slopbrake-hook stop", "timeout": 600}]}]
  }
}
"#;

#[test]
fn codex_guided_trust_append_tombstone_and_foreign_warning() {
    let h = Home::new();
    h.install_codex("0.163.0");
    let hooks = h.home().join(".codex/hooks.json");
    fs::write(&hooks, CODEX_HOOKS).unwrap();
    // Connect all never includes Codex.
    let out = h.run(&["connect", "--all"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("Nothing to connect"));
    assert_eq!(fs::read_to_string(&hooks).unwrap(), CODEX_HOOKS);
    let j = h.connect(&["codex"]);
    let r = h.result_of(&j, "codex");
    assert_eq!(
        (r["outcome"].clone(), r["step"].clone()),
        (json!("needs_step"), json!("codex_trust")),
        "{j}"
    );
    assert_eq!(j["test"], "done", "{j}");
    let tree: Value = serde_json::from_str(&fs::read_to_string(&hooks).unwrap()).unwrap();
    assert_eq!(tree["hooks"]["Stop"].as_array().unwrap().len(), 2);
    assert_eq!(tree["hooks"]["PostToolUse"].as_array().unwrap().len(), 2);
    let ours = commands()["clients"]["codex"]["groups"]
        .as_array()
        .unwrap()
        .clone();
    for g in &ours {
        let ev = g["event"].as_str().unwrap();
        let want: Value = serde_json::from_str(&h.with_shim(&g["group"].to_string())).unwrap();
        assert_eq!(
            tree["hooks"][ev].as_array().unwrap().last().unwrap(),
            &want,
            "{ev}"
        );
    }
    assert_eq!(
        fs::read_to_string(h.home().join(".codex/hooks.json.codepet-orig")).unwrap(),
        CODEX_HOOKS
    );
    let st = h.status_of("codex");
    assert_eq!(
        (st["connected"].clone(), st["needs_step"].clone()),
        (json!(true), json!("codex_trust"))
    );
    // The user trusts all six lines in /hooks: status sees the hashes (read only).
    let p = hooks.display().to_string();
    let mut toml = String::new();
    let doc: Value = serde_json::from_str(&fs::read_to_string(&hooks).unwrap()).unwrap();
    for g in &ours {
        let ev = g["event"].as_str().unwrap();
        let i = doc["hooks"][ev].as_array().unwrap().len() - 1;
        // The frozen hashes hold for the reference paths only; recompute for this home.
        let handler = &doc["hooks"][ev][i]["hooks"][0];
        let mut h2 = serde_json::Map::new();
        h2.insert("async".into(), json!(handler.get("async").is_some()));
        h2.insert("command".into(), handler["command"].clone());
        h2.insert("timeout".into(), json!(2));
        h2.insert("type".into(), json!("command"));
        let fp = json!({"event_name": g["snake_event"], "hooks": [Value::Object(h2)]}).to_string();
        let hash = codepet_storage::sha256_hex(fp.as_bytes());
        toml += &format!(
            "[hooks.state.\"{p}:{}:{i}:0\"]\ntrusted_hash = \"sha256:{hash}\"\n",
            g["snake_event"].as_str().unwrap()
        );
    }
    let config = h.home().join(".codex/config.toml");
    fs::write(&config, &toml).unwrap();
    let st = h.status_of("codex");
    assert_eq!(
        st["needs_step"],
        Value::Null,
        "trust seen by the file-only status"
    );
    // Something appends a foreign Stop group after ours; Disconnect must tombstone ours.
    let mut doc: Value = serde_json::from_str(&fs::read_to_string(&hooks).unwrap()).unwrap();
    doc["hooks"]["Stop"]
        .as_array_mut()
        .unwrap()
        .push(json!({"hooks": [{"type": "command", "command": "later-hook"}]}));
    fs::write(&hooks, pretty(&doc)).unwrap();
    let doctor: Value = serde_json::from_slice(&h.run(&["doctor", "--json"]).stdout).unwrap();
    let cx = &doctor["clients"].as_array().unwrap()[1];
    assert_eq!(cx["codex"]["followed_by_other_hooks"], 1, "{doctor}");
    let plain = String::from_utf8(h.run(&["doctor"]).stdout).unwrap();
    assert!(plain.contains("have other hooks after them"), "{plain}");
    let j = h.disconnect("codex");
    let r = h.result_of(&j, "codex");
    assert_eq!(r["outcome"], "disconnected");
    assert!(
        r["detail"].as_str().unwrap().contains("left in place"),
        "{r}"
    );
    let after: Value = serde_json::from_str(&fs::read_to_string(&hooks).unwrap()).unwrap();
    let stop = after["hooks"]["Stop"].as_array().unwrap();
    assert_eq!(
        stop.len(),
        3,
        "the tombstone keeps the foreign group's index"
    );
    assert_eq!(stop[2]["hooks"][0]["command"], "later-hook");
    assert!(after["hooks"].get("UserPromptSubmit").is_none());
    assert_eq!(
        after["hooks"]["SessionStart"],
        serde_json::from_str::<Value>(CODEX_HOOKS).unwrap()["hooks"]["SessionStart"]
    );
    assert_eq!(
        fs::read_to_string(&config).unwrap(),
        toml,
        "config.toml is never written"
    );
    assert_eq!(h.status_of("codex")["connected"], false);
    assert_eq!(h.unexpected_calls(), "", "the codex binary never ran");
}

#[test]
fn codex_with_hooks_turned_off_writes_nothing_and_too_old_is_refused() {
    let h = Home::new();
    h.install_codex("0.163.0");
    fs::write(h.home().join(".codex/hooks.json"), CODEX_HOOKS).unwrap();
    fs::write(
        h.home().join(".codex/config.toml"),
        "[features]\nhooks = false\n",
    )
    .unwrap();
    let j = h.connect(&["codex"]);
    let r = h.result_of(&j, "codex");
    assert_eq!(
        (r["outcome"].clone(), r["step"].clone()),
        (json!("needs_step"), json!("codex_hooks_off")),
        "{j}"
    );
    assert_eq!(j["test"], Value::Null, "nothing to check");
    assert_eq!(
        fs::read_to_string(h.home().join(".codex/hooks.json")).unwrap(),
        CODEX_HOOKS
    );
    let bar = h.bar();
    assert_eq!(bar["clients"]["codex"]["connected"], false);
    assert_eq!(bar["clients"]["codex"]["needs_step"], "codex_hooks_off");
    let h = Home::new();
    h.install_codex("0.161.2");
    let j = h.connect(&["codex"]);
    assert_eq!(h.result_of(&j, "codex")["reason"], "too_old");
    assert!(!h.home().join(".codex/hooks.json").exists());
    let j = h.connect(&["opencode"]);
    assert_eq!(h.result_of(&j, "opencode")["reason"], "not_installed");
}

#[test]
fn off_test_state_and_receipts() {
    let mut h = Home::new();
    h.install_grok();
    h.ok("app.set_enabled", json!({"enabled": false}));
    assert_eq!(
        h.err_code("clients.connect", json!({"clients": ["grok-cli"]})),
        "INVALID_STATE"
    );
    assert_eq!(h.err_code("clients.test", json!({})), "INVALID_STATE");
    let out = h.run(&["connect", "grok-cli"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&out.stderr).trim(),
        "CodePet is off. Turn it on first: codepet on"
    );
    // Disconnect works while Off (uninstall needs it), and status too.
    let j = h.disconnect("grok-cli");
    assert_eq!(h.result_of(&j, "grok-cli")["outcome"], "disconnected");
    h.ok("clients.status", json!({}));
    h.ok("app.set_enabled", json!({"enabled": true}));
    // A replayed request id returns the same job and starts nothing new.
    let a = h.rpc_with(
        "same",
        "clients.connect",
        json!({"clients": ["grok-cli"]}),
        &[],
    );
    let b = h.rpc_with(
        "same",
        "clients.connect",
        json!({"clients": ["grok-cli"]}),
        &[],
    );
    assert_eq!(a["result"]["job_id"], b["result"]["job_id"]);
    h.wait_job(a["result"]["job_id"].as_str().unwrap());
    let jobs = fs::read_dir(h.state_dir().join("jobs"))
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_str()
                .unwrap()
                .ends_with(".json")
        })
        .count();
    assert_eq!(jobs, 2, "the disconnect and one connect");
    // The bar's test state home: TEST_STATE, nothing started.
    h.set_env("CODEPET_TEST_STATE", "1");
    for (m, p) in [
        ("clients.connect", json!({"clients": ["grok-cli"]})),
        ("clients.disconnect", json!({"client": "grok-cli"})),
        ("clients.test", json!({})),
    ] {
        assert_eq!(h.err_code(m, p), "TEST_STATE", "{m}");
    }
    h.ok("clients.status", json!({}));
}

#[test]
fn a_broken_shim_fails_the_ready_check_and_the_tombstone_is_silent() {
    let h = Home::new();
    h.install_grok();
    h.install_opencode();
    let j = h.connect(&["grok-cli", "opencode"]);
    assert_eq!(j["test"], "done");
    // Stored commands with a tombstoned or deleted shim: exit 0, no output (K10).
    let tomb = include_str!("../../../docs/v0.3/contracts/shim-tombstone.sh");
    for state in ["tombstone", "deleted"] {
        if state == "tombstone" {
            fs::write(h.shim(), tomb).unwrap();
        } else {
            fs::remove_file(h.shim()).unwrap();
        }
        let cmd = format!("{} grok-cli >/dev/null 2>&1 || true", h.shim().display());
        let out = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(&cmd)
            .env_clear()
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(
            (out.status.code(), out.stdout.len(), out.stderr.len()),
            (Some(0), 0, 0),
            "{state}"
        );
    }
    fs::write(h.shim(), tomb).unwrap();
    fs::set_permissions(h.shim(), fs::Permissions::from_mode(0o755)).unwrap();
    let out = std::process::Command::new(h.shim())
        .arg("opencode")
        .env_clear()
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(
        (out.status.code(), out.stdout.len(), out.stderr.len()),
        (Some(0), 0, 0)
    );
    // clients.test now finds nothing arriving.
    let r = h.ok("clients.test", json!({}));
    let j = h.wait_job(r["job_id"].as_str().unwrap());
    assert_eq!(j["test"], "failed", "{j}");
    let st = h.status_of("grok-cli");
    assert_eq!(st["failed_reason"], "ready_check");
    assert_eq!(h.bar()["agent"]["test"]["state"], "failed");
    // Connect repairs the shim and the ready check passes again.
    let j = h.connect(&["grok-cli"]);
    assert_eq!(j["test"], "done");
    assert_eq!(h.status_of("grok-cli")["failed_reason"], Value::Null);
}

#[test]
fn a_killed_job_resumes_on_state_get_and_rolls_back_after_three_attempts() {
    let h = Home::new();
    h.install_grok();
    let r = h.rpc_with(
        "k1",
        "clients.connect",
        json!({"clients": ["grok-cli"]}),
        &[("CODEPET_FAULT", "connect_after_artifact")],
    );
    let job = r["result"]["job_id"].as_str().unwrap().to_owned();
    // The job aborts right after writing the artifact.
    let started = std::time::Instant::now();
    loop {
        let j = h.journal(&job).unwrap();
        if j["attempts"] == 1
            && j["pid"]
                .as_u64()
                .is_some_and(|p| !std::path::Path::new(&format!("/proc/{p}")).exists())
        {
            break;
        }
        assert!(started.elapsed().as_secs() < 20, "{j}");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(h.home().join(".grok/hooks/codepet.json").exists());
    assert_eq!(
        h.bar()["clients"]["grok-cli"]["job"]["kind"],
        "connect",
        "still marked running"
    );
    // The next state.get resumes it (without the fault this time).
    h.ok("state.get", json!({}));
    let j = h.wait_job(&job);
    assert_eq!(j["attempts"], 2);
    assert_eq!(h.result_of(&j, "grok-cli")["outcome"], "connected");
    assert_eq!(h.bar()["clients"]["grok-cli"]["job"], Value::Null);

    // A job that keeps dying is rolled back: the file-owned artifact is removed again.
    let h = Home::new();
    h.install_opencode();
    let fault = [("CODEPET_FAULT", "connect_after_artifact")];
    let r = h.rpc_with(
        "k2",
        "clients.connect",
        json!({"clients": ["opencode"]}),
        &fault,
    );
    let job = r["result"]["job_id"].as_str().unwrap().to_owned();
    let started = std::time::Instant::now();
    loop {
        let j = h.journal(&job).unwrap();
        if j["done"] == true {
            assert_eq!(h.result_of(&j, "opencode")["reason"], "blocked");
            break;
        }
        assert!(started.elapsed().as_secs() < 40, "{j}");
        h.rpc_with(
            &format!("s{}", started.elapsed().as_millis()),
            "state.get",
            json!({}),
            &fault,
        );
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    assert!(!h
        .home()
        .join(".config/opencode/plugins/codepet.js")
        .exists());
    assert_eq!(h.bar()["clients"]["opencode"]["connected"], false);
}

#[test]
fn cli_wrappers_and_doctor() {
    let h = Home::new();
    h.install_grok();
    h.install_claude();
    let canary = "CANARY-settings-content-7f3a";
    fs::write(
        h.home().join(".claude/settings.json"),
        pretty(&json!({"permissions": {"allow": [canary]}})),
    )
    .unwrap();
    for bad in [
        &["connect"][..],
        &["connect", "crush"],
        &["disconnect"],
        &["disconnect", "a", "b"],
        &["doctor", "--x"],
    ] {
        assert_eq!(h.run(bad).status.code(), Some(2), "{bad:?}");
    }
    let out = h.run(&["connect", "--all"]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{text} {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains(
            "Claude Code: connected. Open a new Claude Code window (or type /reload-plugins)."
        ),
        "{text}"
    );
    assert!(
        text.contains("Grok: connected. Open a new Grok window."),
        "{text}"
    );
    assert!(text.contains("Ready check: passed."), "{text}");
    let d = h.run(&["doctor"]);
    let plain = String::from_utf8(d.stdout).unwrap();
    assert_eq!(d.status.code(), Some(0));
    assert!(plain.starts_with("CodePet is On\n"), "{plain}");
    assert!(
        plain.contains("Claude Code: connected, waiting for the first event"),
        "{plain}"
    );
    assert!(plain.contains("Codex: not installed"), "{plain}");
    assert!(plain.contains("Inbox: 0 waiting, 0 dropped"), "{plain}");
    assert!(
        plain.contains("(current), sha256 ") && plain.contains("paths match this install"),
        "{plain}"
    );
    let json_out: Value = serde_json::from_slice(&h.run(&["doctor", "--json"]).stdout).unwrap();
    assert_eq!(json_out["schema"], "codepet.doctor.v1");
    let sha = codepet_storage::sha256_hex(&fs::read(h.shim()).unwrap());
    assert_eq!(json_out["shim"]["sha256"], sha);
    assert_eq!(json_out["shim"]["paths_match"], true);
    for out in [plain.clone(), json_out.to_string()] {
        assert!(!out.contains(canary), "doctor never shows config contents");
    }
    let out = h.run(&["disconnect", "grok-cli"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "Grok: stopped watching\n"
    );
    // Doctor changes nothing.
    let before = snapshot(&h.state_dir());
    h.run(&["doctor", "--json"]);
    let after = snapshot(&h.state_dir());
    let names = |s: &Vec<(String, Vec<u8>)>| {
        s.iter()
            .filter(|(n, _)| !n.contains("sqlite3"))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&before), names(&after));
    // Doctor on a fresh home creates no state.
    let fresh = Home::new();
    let d = fresh.run(&["doctor"]);
    assert_eq!(d.status.code(), Some(0));
    assert!(String::from_utf8(d.stdout)
        .unwrap()
        .starts_with("CodePet has no saved data yet"));
    assert!(!fresh.state_dir().exists());
}

#[test]
fn clients_refresh_rewrites_only_stale_codepet_files() {
    let h = Home::new();
    h.install_grok();
    h.install_opencode();
    h.connect(&["grok-cli", "opencode"]);
    let js = h.home().join(".config/opencode/plugins/codepet.js");
    let good = fs::read_to_string(&js).unwrap();
    fs::write(&js, "// managed by codepet: an older release\n").unwrap();
    let out = h.run(&["clients-refresh"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "refreshed: OpenCode\n"
    );
    assert_eq!(fs::read_to_string(&js).unwrap(), good);
    // A file without the marker is never touched; a disconnected client is skipped.
    fs::write(&js, "// mine\n").unwrap();
    let out = h.run(&["clients-refresh"]);
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "");
    assert_eq!(fs::read_to_string(&js).unwrap(), "// mine\n");
}
