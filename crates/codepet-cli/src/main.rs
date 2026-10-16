//! CodePet short commands. No daemon: each invocation answers one explicit request and
//! exits; only a successful `rpc` task.start continues as that job's mock supervisor.
//!
//! v0.3 skeleton (spec §6.3): `migrate`, `fold`, `clients-job`, `on`, `off`, `helpers`,
//! `connect`, `disconnect`, `forget-history` and `doctor` are routed and answer
//! `NOT_IMPLEMENTED` (exit 1, one stderr line, no state touched) until their work package
//! lands. `hook` already keeps invariant O2: it always exits 0 silently.
mod clients_job;
mod demo;
mod doctor;
mod errors;
mod fold;
mod hook;
mod rpc;
mod supervisor;

use std::io::Write;

const HELP: &str = "CodePet: a desktop companion with durable mock execution
  codepet providers   Show provider support (mock only)
  codepet state       Print the state.get snapshot as JSON
  codepet rpc         Answer one protocol request frame read from stdin
  codepet mock-demo   Explicitly run an in-memory simulation
  codepet --version   Print the version
No daemon, real provider, project IO, timers or background polling. Only an explicit
task.start runs a per-job mock supervisor, which spawns no process.";

/// `codepet --version`; printed without opening storage.
const VERSION: &str = concat!("codepet ", env!("CARGO_PKG_VERSION"));

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
            0
        }
        (Some(Some("--version")), 1) => {
            println!("{VERSION}");
            0
        }
        (Some(Some("providers")), 1) => {
            println!("{PROVIDERS}");
            0
        }
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
        (
            Some(Some(
                c @ ("migrate" | "on" | "off" | "helpers" | "connect" | "disconnect"
                | "forget-history"),
            )),
            _,
        ) => not_implemented(c),
        (
            Some(Some(
                c @ ("--help" | "help" | "--version" | "providers" | "mock-demo" | "rpc" | "state"),
            )),
            _,
        ) => usage(&format!("{c} takes no arguments")),
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

/// The v0.3 skeleton's answer for a routed command whose work package has not landed:
/// one stderr line naming the protocol code, exit 1, nothing read or written.
pub(crate) fn not_implemented(command: &str) -> i32 {
    let _ = writeln!(
        std::io::stderr(),
        "{}: codepet {command} is {}",
        codepet_protocol::ErrorCode::NotImplemented,
        codepet_protocol::V03_PENDING_MESSAGE
    );
    1
}

fn usage(message: &str) -> i32 {
    let _ = writeln!(std::io::stderr(), "{message}. Use --help.");
    2
}
