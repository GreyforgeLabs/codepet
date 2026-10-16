//! CodePet short commands. No daemon: each invocation answers one explicit request and
//! exits. (With the retired `legacy-tasks` feature only, a successful `rpc` task.start
//! continues as that job's mock supervisor.)
//!
//! v0.3 (spec §6.3): `migrate`, `on`, `off`, `helpers` and `forget-history` (WP1); `hook`
//! and `fold` (WP2; the hook keeps invariant O2: it always exits 0 silently); `connect`,
//! `disconnect`, `doctor` and the internal `clients-job` and `clients-refresh` (WP3).
mod clients;
mod clients_job;
mod commands;
#[cfg(feature = "legacy-tasks")]
mod demo;
mod doctor;
mod errors;
mod fold;
mod hook;
mod install;
mod rpc;
#[cfg(feature = "legacy-tasks")]
mod supervisor;

use std::io::Write;

const HELP: &str = "CodePet: a desktop pet that keeps you company while your coding apps work
  codepet state            Print the state.get snapshot as JSON
  codepet rpc              Answer one protocol request frame read from stdin
  codepet migrate          Upgrade the saved pet data to this version (the installer runs it)
  codepet on | off         Turn CodePet on or off (Off freezes the pet; nothing runs)
  codepet helpers on|off   Count background helpers (headless coding sessions) or not
  codepet forget-history   Forget coding history (treats and growth are kept)
  codepet connect <app>... | --all
                           Let the pet watch coding apps (claude-code, codex, opencode,
                           hermes, grok-cli); --all skips Codex, which needs a grown-up step
  codepet disconnect <app> Stop watching a coding app
  codepet doctor [--json]  Plain-word health check (changes nothing)
  codepet --version        Print the version
No daemon and no background polling. CodePet only observes coding apps you connect; it never
starts, stops, blocks or answers them.";
#[cfg(feature = "legacy-tasks")]
const LEGACY_HELP: &str = "Retired (legacy-tasks build only):
  codepet providers   Show provider support (mock only)
  codepet mock-demo   Explicitly run an in-memory simulation";

/// `codepet --version`; printed without opening storage.
const VERSION: &str = concat!("codepet ", env!("CARGO_PKG_VERSION"));

#[cfg(feature = "legacy-tasks")]
const PROVIDERS: &str = "Mock: available (simulation only)
Codex: unsupported
OpenCode: unsupported
Hermes: unsupported
Claude Code: unsupported";

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let command = args.first().map(|a| a.to_str());
    let code = match (command, args.len()) {
        (None, _) | (Some(Some("--help" | "help")), 1) => {
            println!("{HELP}");
            #[cfg(feature = "legacy-tasks")]
            println!("{LEGACY_HELP}");
            0
        }
        (Some(Some("--version")), 1) => {
            println!("{VERSION}");
            0
        }
        #[cfg(feature = "legacy-tasks")]
        (Some(Some("providers")), 1) => {
            println!("{PROVIDERS}");
            0
        }
        #[cfg(feature = "legacy-tasks")]
        (Some(Some("mock-demo")), 1) => {
            demo::run();
            0
        }
        (Some(Some("rpc")), 1) => rpc::run(),
        (Some(Some("state")), 1) => state(),
        // v0.3: the hook never reports a usage error (O2), whatever its arguments.
        (Some(Some("hook")), _) => hook::run(&args[1..]),
        (Some(Some("fold")), _) => fold::run(&args[1..]),
        (Some(Some("clients-job")), _) => clients_job::run(&args[1..]),
        (Some(Some("doctor")), _) => doctor::run(&args[1..]),
        (Some(Some("migrate")), 1) => commands::migrate(),
        (Some(Some("on")), 1) => commands::set_enabled(true),
        (Some(Some("off")), 1) => commands::set_enabled(false),
        (Some(Some("helpers")), 2) => match args[1].to_str() {
            Some("on") => commands::set_helpers(true),
            Some("off") => commands::set_helpers(false),
            _ => usage("helpers takes on or off"),
        },
        (Some(Some("forget-history")), 1) => commands::forget_history(),
        (Some(Some("helpers")), _) => usage("helpers takes on or off"),
        (Some(Some(c @ ("migrate" | "on" | "off" | "forget-history"))), _) => {
            usage(&format!("{c} takes no arguments"))
        }
        (Some(Some("connect")), _) => clients::connect_cli(&args[1..]),
        (Some(Some("disconnect")), _) => clients::disconnect_cli(&args[1..]),
        (Some(Some("clients-refresh")), 1) => clients::refresh_cli(),
        #[cfg(feature = "legacy-tasks")]
        (Some(Some(c @ ("providers" | "mock-demo"))), _) => {
            usage(&format!("{c} takes no arguments"))
        }
        (Some(Some(c @ ("--help" | "help" | "--version" | "rpc" | "state"))), _) => {
            usage(&format!("{c} takes no arguments"))
        }
        (Some(Some(other)), _) => usage(&format!("Unsupported command: {other}")),
        (Some(None), _) => usage("Unsupported command"),
    };
    std::process::exit(code);
}

fn state() -> i32 {
    match rpc::state_get() {
        Ok(value) => {
            let mut out = std::io::stdout().lock();
            match writeln!(out, "{value}").and_then(|()| out.flush()) {
                Ok(()) => 0,
                Err(_) => 1,
            }
        }
        Err(f) => {
            let _ = writeln!(std::io::stderr(), "{}: {}", f.code, f.message);
            1
        }
    }
}

fn usage(message: &str) -> i32 {
    let _ = writeln!(std::io::stderr(), "{message}. Use --help.");
    2
}
