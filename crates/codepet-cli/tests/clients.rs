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
        // CI installs PyYAML (rust.yml): there a missing module is a failure, not a skip.
        assert!(
            std::env::var_os("CI").is_none(),
            "python3 has no yaml module for the Hermes stand-in (CI must install PyYAML)"
        );
        eprintln!("SKIPPED: python3 has no yaml module for the Hermes stand-in");
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

/// Waits until `job` died after its first attempt (a `CODEPET_FAULT` abort).
fn wait_dead(h: &Home, job: &str) -> Value {
    let started = std::time::Instant::now();
    loop {
        let j = h.journal(job).unwrap();
        if j["attempts"] == 1
            && j["pid"]
                .as_u64()
                .is_some_and(|p| !std::path::Path::new(&format!("/proc/{p}")).exists())
        {
            return j;
        }
        assert!(started.elapsed().as_secs() < 20, "{j}");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn job_of(r: &Value) -> String {
    assert_eq!(r["ok"], true, "{r}");
    r["result"]["job_id"].as_str().unwrap().to_owned()
}

/// spec §5.1 / notes §7 item 1: a Connect from another state dir (a `stateHome` test bar
/// that did not say so) never repoints the owner's shim; `--repoint` is the explicit way.
#[test]
fn a_connect_from_another_state_dir_never_repoints_the_shim() {
    let h = Home::new();
    h.install_grok();
    let j = h.connect(&["grok-cli"]);
    assert_eq!(j["test"], "done", "{j}");
    let real = fs::read(h.shim()).unwrap();
    let grok = fs::read(h.home().join(".grok/hooks/codepet.json")).unwrap();
    // A second state home B: only XDG_STATE_HOME differs (no CODEPET_TEST_STATE).
    let b_home = h.home().parent().unwrap().join("state-b");
    let b = b_home.to_str().unwrap().to_owned();
    let env = [("XDG_STATE_HOME", b.as_str())];
    let r = h.rpc_with(
        "b1",
        "clients.connect",
        json!({"clients": ["grok-cli"]}),
        &env,
    );
    let wait_b = |job: &str| -> Value {
        let path = b_home.join(format!("codepet/jobs/{job}.json"));
        let started = std::time::Instant::now();
        loop {
            if let Some(j) = fs::read_to_string(&path)
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                .filter(|j| j["done"] == true)
            {
                return j;
            }
            assert!(started.elapsed().as_secs() < 30);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    };
    let jb = wait_b(&job_of(&r));
    let res = h.result_of(&jb, "grok-cli");
    assert_eq!(
        (res["outcome"].clone(), res["reason"].clone()),
        (json!("failed"), json!("blocked")),
        "{jb}"
    );
    let detail = res["detail"].as_str().unwrap();
    assert!(
        detail.contains(h.state_dir().to_str().unwrap()) && detail.contains("--repoint"),
        "{detail}"
    );
    assert_eq!(jb["test"], Value::Null, "no ready check ran: {jb}");
    assert_eq!(
        fs::read(h.shim()).unwrap(),
        real,
        "the owner's shim is unchanged"
    );
    assert_eq!(
        fs::read(h.home().join(".grok/hooks/codepet.json")).unwrap(),
        grok
    );
    // A ready check started from B (whose files say Grok is connected) never runs the
    // owner's shim either: it would test, and write into, A.
    let a_test = h.bar()["agent"]["test"].clone();
    let jb = wait_b(&job_of(&h.rpc_with("b2", "clients.test", json!({}), &env)));
    assert_eq!(jb["test"], "failed", "{jb}");
    assert_eq!(h.result_of(&jb, "grok-cli")["reason"], "blocked", "{jb}");
    assert_eq!(h.bar()["agent"]["test"], a_test, "nothing reached A");
    // Doctor (from B) says why Connect is blocked.
    let d = h
        .cmd(&["doctor", "--json"])
        .env("XDG_STATE_HOME", &b)
        .output()
        .unwrap();
    let dj: Value = serde_json::from_slice(&d.stdout).unwrap();
    assert!(dj["shim"]["conflict"]
        .as_str()
        .is_some_and(|c| c.contains("another state folder")));
    let d = h
        .cmd(&["doctor"])
        .env("XDG_STATE_HOME", &b)
        .output()
        .unwrap();
    assert!(String::from_utf8(d.stdout)
        .unwrap()
        .contains("Connect is blocked"));
    // From A everything is as before.
    assert_eq!(h.run(&["doctor", "--json"]).status.code(), Some(0));
    // The explicit repoint.
    let out = h
        .cmd(&["connect", "--repoint", "grok-cli"])
        .env("XDG_STATE_HOME", &b)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(fs::read_to_string(h.shim())
        .unwrap()
        .contains(&format!("S='{}/codepet'", b)));
    // A's Connect is now the one refused; once B's state is gone, A takes it back.
    let j = h.connect(&["grok-cli"]);
    assert_eq!(h.result_of(&j, "grok-cli")["reason"], "blocked", "{j}");
    fs::remove_dir_all(&b_home).unwrap();
    let j = h.connect(&["grok-cli"]);
    assert_eq!(h.result_of(&j, "grok-cli")["outcome"], "connected", "{j}");
    assert_eq!(fs::read(h.shim()).unwrap(), real);
    for bad in [
        &["connect", "--repoint"][..],
        &["connect", "--repoint", "--all"],
    ] {
        assert_eq!(h.run(bad).status.code(), Some(2), "{bad:?}");
    }
}

/// E2: a job found while Off runs nothing; a Connect turned Off midway skips its ready
/// check and records no `ready_check` failure.
#[test]
fn nothing_runs_in_the_background_while_off() {
    // A killed Connect, then Off, then the popup opens (state.get).
    let h = Home::new();
    h.install_grok();
    let r = h.rpc_with(
        "k1",
        "clients.connect",
        json!({"clients": ["grok-cli"]}),
        &[("CODEPET_FAULT", "connect_after_artifact")],
    );
    let job = job_of(&r);
    let dead = wait_dead(&h, &job);
    h.ok("app.set_enabled", json!({"enabled": false}));
    h.ok("state.get", json!({}));
    let j = h.journal(&job).unwrap();
    assert_eq!(j["done"], true, "ended, not resumed: {j}");
    assert_eq!(
        (j["attempts"].clone(), j["pid"].clone()),
        (dead["attempts"].clone(), dead["pid"].clone()),
        "no job process ran"
    );
    assert_eq!(j["test"], Value::Null);
    let res = h.result_of(&j, "grok-cli");
    assert_eq!(
        (res["reason"].clone(), res["detail"].clone()),
        (json!("blocked"), json!("CodePet was off"))
    );
    assert!(!h.state_dir().join("test.nonce").exists());
    let bar = h.bar();
    assert_eq!(bar["clients"]["grok-cli"]["job"], Value::Null);
    assert_ne!(bar["clients"]["grok-cli"]["failed_reason"], "ready_check");
    assert_eq!(bar["agent"]["test"], Value::Null, "no ready check ran");
    h.ok("app.set_enabled", json!({"enabled": true}));

    // A ready check killed midway, then Off: abandoned without a verdict.
    let r = h.rpc_with(
        "k2",
        "clients.test",
        json!({}),
        &[("CODEPET_FAULT", "test_after_begin")],
    );
    let job = job_of(&r);
    wait_dead(&h, &job);
    h.ok("app.set_enabled", json!({"enabled": false}));
    h.ok("state.get", json!({}));
    let j = h.journal(&job).unwrap();
    assert_eq!(
        (j["done"].clone(), j["test"].clone()),
        (json!(true), Value::Null),
        "{j}"
    );
    let bar = h.bar();
    assert_eq!(bar["agent"]["test"], Value::Null, "{bar}");
    assert_ne!(bar["clients"]["grok-cli"]["failed_reason"], "ready_check");
    h.ok("app.set_enabled", json!({"enabled": true}));

    // Off while a Connect runs (Claude's install takes 2 s): no ready check afterwards.
    let h = Home::new();
    h.install_claude();
    fs::write(h.home().join("claude-slow"), "").unwrap();
    let job = job_of(&h.rpc("clients.connect", json!({"clients": ["claude-code"]})));
    let started = std::time::Instant::now();
    while !h
        .calls("claude-calls.log")
        .iter()
        .any(|c| c["argv"][1] == "install")
    {
        assert!(started.elapsed().as_secs() < 20);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    h.ok("app.set_enabled", json!({"enabled": false}));
    let j = h.wait_job(&job);
    assert_eq!(
        h.result_of(&j, "claude-code")["outcome"],
        "connected",
        "{j}"
    );
    assert_eq!(j["test"], Value::Null, "no ready check while Off: {j}");
    let st = h.status_of("claude-code");
    assert_eq!(st["failed_reason"], Value::Null, "{st}");
    assert_eq!(h.bar()["agent"]["test"], Value::Null);
    // Stop watching still runs while Off.
    let j = h.disconnect("claude-code");
    assert_eq!(
        h.result_of(&j, "claude-code")["outcome"],
        "disconnected",
        "{j}"
    );
}

/// A job queued behind another job's lock is never taken for dead (and started twice),
/// and recovery never rewrites a journal while a job holds the lock.
#[test]
fn a_queued_job_is_not_duplicated_and_recovery_waits_for_the_lock() {
    use std::os::unix::fs::DirBuilderExt;
    let h = Home::new();
    h.install_grok();
    h.ok("clients.status", json!({}));
    let jobs = h.state_dir().join("jobs");
    if !jobs.exists() {
        fs::DirBuilder::new().mode(0o700).create(&jobs).unwrap();
    }
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(jobs.join(".lock"))
        .unwrap();
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::LockExclusive).unwrap();
    let job = job_of(&h.rpc("clients.connect", json!({"clients": ["grok-cli"]})));
    // Queued for a minute already (the old 30 s start grace would call it dead).
    let path = jobs.join(format!("{job}.json"));
    let mut j: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(j["pid"], Value::Null, "still waiting for the lock: {j}");
    j["created_ms"] = json!(j["created_ms"].as_i64().unwrap() - 60_000);
    fs::write(&path, serde_json::to_vec_pretty(&j).unwrap()).unwrap();
    let before = fs::read(&path).unwrap();
    for _ in 0..3 {
        h.ok("state.get", json!({}));
    }
    assert_eq!(
        fs::read(&path).unwrap(),
        before,
        "not restarted, not rewritten"
    );
    assert_eq!(h.bar()["clients"]["grok-cli"]["job"]["kind"], "connect");
    drop(lock);
    let j = h.wait_job(&job);
    assert_eq!(j["attempts"], 1, "ran exactly once: {j}");
    assert_eq!(h.result_of(&j, "grok-cli")["outcome"], "connected");
}

/// notes §7 item 10: BUSY for a client with a running job, INVALID_STATE for a ready
/// check without a connected app.
#[test]
fn busy_and_no_connected_app_errors() {
    let h = Home::new();
    h.install_grok();
    assert_eq!(h.err_code("clients.test", json!({})), "INVALID_STATE");
    assert_eq!(
        h.err_code("clients.test", json!({"client": "grok-cli"})),
        "INVALID_STATE"
    );
    // A killed Connect stays marked running; the next clients.* call resumes it, and a
    // second job for the same client is refused while it runs.
    let r = h.rpc_with(
        "busy1",
        "clients.connect",
        json!({"clients": ["grok-cli"]}),
        &[("CODEPET_FAULT", "connect_after_artifact")],
    );
    let job = job_of(&r);
    wait_dead(&h, &job);
    assert_eq!(
        h.err_code("clients.connect", json!({"clients": ["grok-cli"]})),
        "BUSY"
    );
    assert_eq!(
        h.err_code("clients.disconnect", json!({"client": "grok-cli"})),
        "BUSY"
    );
    let j = h.wait_job(&job);
    assert_eq!(h.result_of(&j, "grok-cli")["outcome"], "connected");
    assert_eq!(j["attempts"], 2);
}

/// §5.2: every Connect clears first_seen, so the first real event after it says hello
/// exactly once.
#[test]
fn each_connect_waits_for_one_new_hello() {
    let h = Home::new();
    h.install_grok();
    h.connect(&["grok-cli"]);
    let send = |fixture: &str| {
        let f: Value = serde_json::from_str(
            &fs::read_to_string(format!(
                "{}/tests/fixtures/grok-cli/{fixture}.json",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap(),
        )
        .unwrap();
        let cmd = format!("{} grok-cli >/dev/null 2>&1 || true", h.shim().display());
        let mut child = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(&cmd)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", h.home())
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(f["stdin"].as_str().unwrap().as_bytes())
            .unwrap();
        assert!(child.wait().unwrap().success());
    };
    let wait_hello = |n: u64| {
        let started = std::time::Instant::now();
        loop {
            let bar = h.bar();
            if bar["agent"]["hello_seq"].as_u64() == Some(n) {
                return bar;
            }
            assert!(started.elapsed().as_secs() < 10, "{}", bar["agent"]);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    };
    assert_eq!(h.status_of("grok-cli")["first_seen_ms"], Value::Null);
    send("01-session-start");
    let bar = wait_hello(1);
    assert_eq!(bar["agent"]["hello_app"], "grok-cli");
    assert!(h.status_of("grok-cli")["first_seen_ms"].is_i64());
    send("02-prompt-submit");
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert_eq!(wait_hello(1)["agent"]["hello_seq"], 1, "no second hello");
    // Connect again: waiting for the app again, then one more hello.
    let j = h.connect(&["grok-cli"]);
    assert_eq!(h.result_of(&j, "grok-cli")["outcome"], "connected");
    assert_eq!(h.status_of("grok-cli")["first_seen_ms"], Value::Null);
    assert_eq!(
        h.bar()["agent"]["hello_seq"],
        1,
        "the ready check is no hello"
    );
    send("09-stop-end-turn");
    wait_hello(2);
    assert!(h.status_of("grok-cli")["first_seen_ms"].is_i64());
}

/// §9.2 "kill the clients job after each step": a killed Stop watching and a killed ready
/// check resume on the next state.get; a Stop watching that keeps dying rolls back.
#[test]
fn killed_disconnect_and_ready_check_jobs_resume_or_roll_back() {
    let h = Home::new();
    h.install_grok();
    h.connect(&["grok-cli"]);
    let fault = [("CODEPET_FAULT", "disconnect_after_flag")];
    let job = job_of(&h.rpc_with(
        "kd1",
        "clients.disconnect",
        json!({"client": "grok-cli"}),
        &fault,
    ));
    wait_dead(&h, &job);
    assert!(h
        .state_dir()
        .join("clients/grok-cli.disconnected")
        .is_file());
    h.ok("state.get", json!({}));
    let j = h.wait_job(&job);
    assert_eq!(
        (
            j["attempts"].clone(),
            h.result_of(&j, "grok-cli")["outcome"].clone()
        ),
        (json!(2), json!("disconnected")),
        "{j}"
    );
    assert!(!h.home().join(".grok/hooks/codepet.json").exists());

    h.connect(&["grok-cli"]);
    let fault = [("CODEPET_FAULT", "test_after_begin")];
    let job = job_of(&h.rpc_with("kt1", "clients.test", json!({}), &fault));
    wait_dead(&h, &job);
    h.ok("state.get", json!({}));
    let j = h.wait_job(&job);
    assert_eq!(
        (j["attempts"].clone(), j["test"].clone()),
        (json!(2), json!("done")),
        "{j}"
    );
    assert_eq!(h.bar()["agent"]["test"]["state"], "done");

    // A Stop watching that dies every time: rolled back after three attempts.
    let fault = [("CODEPET_FAULT", "disconnect_after_flag")];
    let job = job_of(&h.rpc_with(
        "kd2",
        "clients.disconnect",
        json!({"client": "grok-cli"}),
        &fault,
    ));
    let started = std::time::Instant::now();
    let j = loop {
        let j = h.journal(&job).unwrap();
        if j["done"] == true {
            break j;
        }
        assert!(started.elapsed().as_secs() < 60, "{j}");
        h.rpc_with(
            &format!("s{}", started.elapsed().as_millis()),
            "state.get",
            json!({}),
            &fault,
        );
        std::thread::sleep(std::time::Duration::from_millis(300));
    };
    let res = h.result_of(&j, "grok-cli");
    assert_eq!(
        (res["reason"].clone(), res["detail"].clone()),
        (json!("blocked"), json!("the job stopped too often")),
        "{j}"
    );
    assert_eq!(
        h.status_of("grok-cli")["connected"],
        false,
        "the flag keeps the shim silent"
    );
}

/// §5.1: every Connect sweeps CodePet's stale temporary names (a job killed mid-write).
#[test]
fn connect_sweeps_stale_codepet_temps() {
    let h = Home::new();
    h.install_grok();
    let staging = h.data_dir().join("staging");
    fs::create_dir_all(staging.join(".claude-code.partial-99999/plugin")).unwrap();
    fs::write(staging.join(".claude-code.partial-99999/plugin/x"), "x").unwrap();
    let codex = h.home().join(".codex");
    fs::create_dir_all(&codex).unwrap();
    for n in [
        ".hooks.json.codepet-99999",
        ".hooks.json.codepet-last.tmp",
        ".hooks.json.codepet-orig.tmp",
        ".hooks.json.codepet-restore",
    ] {
        fs::write(codex.join(n), "{}").unwrap();
    }
    fs::write(codex.join(".hooks.json.mine"), "keep").unwrap();
    fs::write(codex.join("hooks.json"), "{}").unwrap();
    let j = h.connect(&["grok-cli"]);
    assert_eq!(h.result_of(&j, "grok-cli")["outcome"], "connected");
    assert_eq!(fs::read_dir(&staging).unwrap().count(), 0);
    let mut left: Vec<String> = fs::read_dir(&codex)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(left, [".hooks.json.mine", "hooks.json"]);
}

/// Uninstall runs `codepet disconnect` for every app: an app CodePet never touched is
/// reported as nothing to stop (no flag, no row), even with a config the reader cannot
/// parse; Hermes' Stop watching needs no config.yaml without CodePet's plugin dir.
#[test]
fn stop_watching_an_untouched_app_records_nothing_and_never_warns() {
    let h = Home::new();
    h.install_hermes();
    // An anchor: valid YAML that CodePet's small block reader refuses.
    fs::write(
        h.home().join(".hermes/config.yaml"),
        "model: &m gpt\nother: *m\n",
    )
    .unwrap();
    h.ok("clients.status", json!({}));
    let rows_before = h.status_of("hermes");
    let out = h.run(&["disconnect", "hermes"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "Hermes: CodePet is not connected there; nothing to stop\n"
    );
    assert!(!h.state_dir().join("clients/hermes.disconnected").exists());
    assert!(
        !h.state_dir().join("jobs").exists()
            || fs::read_dir(h.state_dir().join("jobs"))
                .unwrap()
                .flatten()
                .all(|e| !e.file_name().to_string_lossy().ends_with(".json"))
    );
    assert_eq!(h.status_of("hermes"), rows_before);
    for app in ["claude-code", "codex", "opencode", "grok-cli"] {
        let out = h.run(&["disconnect", app]);
        assert_eq!(out.status.code(), Some(0), "{app}");
    }
    // The bar's Stop watching still runs the job, which succeeds.
    let j = h.disconnect("hermes");
    assert_eq!(h.result_of(&j, "hermes")["outcome"], "disconnected", "{j}");
    assert!(h.calls("hermes-calls.log").is_empty(), "hermes never ran");
}

/// A plugin the user turned off in Claude Code's /plugin list is not connected, and
/// Connect says what to do instead of reporting success.
#[test]
fn claude_code_turned_off_in_its_plugin_list_is_not_connected() {
    let h = Home::new();
    h.install_claude();
    let settings = h.home().join(".claude/settings.json");
    fs::write(&settings, pretty(&claude_settings())).unwrap();
    h.connect(&["claude-code"]);
    assert_eq!(h.status_of("claude-code")["connected"], true);
    let mut s: Value = serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
    s["enabledPlugins"]["codepet@codepet-local"] = json!(false);
    fs::write(&settings, pretty(&s)).unwrap();
    assert_eq!(h.status_of("claude-code")["connected"], false);
    let j = h.connect(&["claude-code"]);
    let r = h.result_of(&j, "claude-code");
    assert_eq!(
        (r["outcome"].clone(), r["reason"].clone()),
        (json!("failed"), json!("blocked")),
        "{j}"
    );
    assert!(r["detail"].as_str().unwrap().contains("/plugin"), "{j}");
    assert_eq!(j["test"], Value::Null, "{j}");
    // Turned on again in Claude: Connect succeeds.
    s["enabledPlugins"]["codepet@codepet-local"] = json!(true);
    fs::write(&settings, pretty(&s)).unwrap();
    let j = h.connect(&["claude-code"]);
    assert_eq!(
        h.result_of(&j, "claude-code")["outcome"],
        "connected",
        "{j}"
    );
}
