//! CodePet short commands. No daemon: each invocation answers one explicit request and
//! exits; only a successful `rpc` task.start continues as that job's mock supervisor.
mod demo;
mod errors;
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

fn usage(message: &str) -> i32 {
    let _ = writeln!(std::io::stderr(), "{message}. Use --help.");
    2
}
