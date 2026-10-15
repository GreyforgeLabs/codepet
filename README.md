# CodePet

A manually controlled coding companion, built toward **Linux portability with Omarchy integration by default**. Rust owns task policy and state; Qt Quick/QML supplies the lightweight UI. This repository is at **milestone 1: durable mock execution**. It is not an installed desktop pet or a functioning coding agent: the only provider is a mock that runs no command.

## What works today

- Rust workspace with four crates: `codepet-core` (pure rules and the older in-memory model), `codepet-protocol` (versioned, bounded JSON envelopes), `codepet-storage` (SQLite state) and the `codepet` CLI
- Durable state in SQLite under `$XDG_STATE_HOME/codepet` (fallback `~/.local/state/codepet`): explicit migrations, ownership/mode checks on the state directory, WAL, and one `BEGIN IMMEDIATE` transaction per mutation
- Four high-is-good care meters (start 70), +20 per accepted artifact, clamped at 100. Acceptance, reward row, meter update, state change and idempotency record commit in one transaction; a repeated acceptance returns the stored receipt without a second reward
- One task owns the active slot, enforced by a unique index across processes; one-use, 5-minute preflight tokens tied to the exact draft revision and settings revision
- `codepet rpc`: one request frame on stdin, one response frame on stdout (64 KiB frames, typed error codes). `codepet state` prints the snapshot
- A per-job **mock supervisor**: after an explicit `task.start`, the same process steps the task to review (or failure) and honors cancellation from another process. It spawns no process and never reads or writes the selected folder
- Review digests over the stored artifact, rechecked at review, acceptance and recovery; explicit recovery that marks dead jobs `interrupted` and damaged reviews `recovery_required`, and never resumes work
- Shared QML panel, form and review components that render only what Rust returns, with two hosts: a pure-QML preview with fixtures (`qml/standalone`) and a Quickshell adapter that runs `codepet rpc` per request (`qml/omarchy`)
- `mock-demo`: the older in-memory simulation, unchanged

## Try it

Requires Rust 1.85+ as declared; this milestone was built and tested locally with Rust 1.98.1 only (see [validation](docs/validation.md)). Dependencies: serde, serde_json, rusqlite with bundled SQLite (needs a C compiler), sha2, and tempfile for tests. `--locked` builds need these crates from crates.io or an already populated Cargo registry cache.

```sh
cargo test --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run -p codepet -- providers
cargo run -p codepet -- mock-demo
printf '%s\n' '{"v":1,"id":"r1","method":"state.get","params":{}}' | cargo run -q -p codepet -- rpc
```

`codepet rpc` and `codepet state` create the state directory if it is missing. To experiment without touching your real state, set `XDG_STATE_HOME` to a temporary directory. Request and result shapes are in [docs/milestone-1.md](docs/milestone-1.md).

## Not implemented yet

- Real providers (Codex, OpenCode, Hermes, Claude Code), authentication, adapters and version probes
- Staging, sandboxing, cgroups/transient units, process containment, deadlines and real verification commands
- Apply, rollback, diffs, `task.logs`; artifact paging (artifacts are capped at 24 KiB of JSON for now)
- Decay settlement (settings are stored only), retention and history deletion
- Job socket and event streaming; the Qt C++ standalone launcher
- The Quickshell adapter has been tested headlessly against the real `codepet rpc` (one manual loop), but never in a live Omarchy panel; no Omarchy plugin registration, bar widget, packaging, assets or sprite animation
- Runtime or visual testing on a live Omarchy, Wayland or X11 session or other distributions
- CPU, memory or binary-size measurements; resource budgets remain design targets

Codex, OpenCode, Hermes and Claude Code are explicitly unsupported. Nothing in this repository can launch them. Do not connect a real provider until containment, staging, review/apply and adapter release gates are implemented and tested.

The desired architecture has **no permanently resident Rust daemon**: short explicit commands and one bounded per-job supervisor while a task runs. Omarchy-specific integration belongs in an adapter; the portable core and standalone UI must not require Hyprland or Quickshell. "Any Linux" is the portability goal, not a tested compatibility claim.

## Documentation

- [Milestone 1 contract and deviations](docs/milestone-1.md)
- [Full target specification](docs/implementation-spec.md): design, not completed implementation
- [Architecture and milestone boundaries](docs/architecture.md)
- [Security and manual-only invariants](docs/security.md)
- [Provider compatibility](docs/providers.md)
- [Validation report](docs/validation.md)
- [Licenses and upstream status](docs/licenses.md)
- [QML UI](qml/README.md)

`docs/source-manifest.sha256` records the original import and is now stale: on 2026-10-14, 9 of its entries no longer match (milestone 1 changed the Cargo files, README, core, CLI and QML README) and 2 listed QML files were removed (see [qml/README.md](qml/README.md)). It is kept unchanged as the import record. No upstream code or sprites are vendored. Preserve this distinction until upstream commits, assets and licenses are audited.

## Source-art catalog

The [generated art library](docs/art-library/README.md) preserves 15 AI-generated concept/pose sheets with prompts, hashes and QA limitations. These are not tested animation atlases and are not integrated into the runtime.
