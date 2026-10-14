# CodePet

A manually controlled coding companion, built toward **Linux portability with Omarchy integration by default**. Rust owns task policy; Qt Quick/QML supplies the lightweight UI. This repository is an **initial mock-only scaffold**, not an installed desktop pet or functioning coding agent.

## What works today

- Dependency-free Rust workspace: `codepet-core` and `codepet` CLI
- Four high-is-good care meters, initially 70, with capped +20 acceptance rewards
- Explicit Start capability tied to a reviewed draft revision and engine instance
- One active task, including waiting, review, cancellation, and recovery
- Job-scoped bounded status events, typed errors, framing/settings/input limits
- Mock execution, cancellation and restart-policy modeling; no automatic retries or resume
- Explicit artifact acceptance; in-memory idempotent receipts and missing/failed-check override
- Standalone QML **visual mock**, disconnected from Rust, with form/progress/review controls
- Unit tests covering manual-start, wrong/stale tokens, duplicate acceptance, reward boundaries, recovery, bounds and fail-closed unsupported providers

## Try the Rust scaffold

Requires Rust 1.85+ (tested using 1.85.0 and 1.99.0). No third-party Rust dependencies.

```sh
cargo test --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run -p codepet -- providers
cargo run -p codepet -- mock-demo
```

`mock-demo` explicitly simulates one task up to review, then exits. It never accepts automatically, edits a project, invokes a provider, or leaves a service running. Each invocation starts fresh in memory. Tests simulate explicit acceptance to verify reward rules.

For the standalone visual preview and open host-validation checklist, see [qml/README.md](qml/README.md). The QML preview and Rust demo are **separate prototypes**, not an end-to-end connected app.

## Not implemented yet

- Persistent SQLite state, transactional acceptance/reward ledger, real crash recovery or rollback
- Real JSON IPC, Unix sockets, Qt-to-Rust bridge, production Start token/review hashing
- Canonical path/symlink/XDG checks, staged workspaces, file diffs, safe apply or backups
- Provider execution, authentication, sandboxing, process containment, deadlines or verification commands
- Decay calculations (settings are modeled only), filesystem retention, configuration loading
- Omarchy/Quickshell plugin, tray/bar registration, packaging, assets or sprite animation
- Runtime/visual testing on Qt, Omarchy, Wayland, X11 or other distributions
- Memory/CPU/binary-size benchmarking or demonstrated resource budgets

Codex, OpenCode, Hermes and Claude Code are explicitly unsupported. Nothing in this scaffold can launch them. Do not connect a real provider until containment, storage, review/apply and adapter release gates are implemented and tested.

The desired architecture has **no permanently resident Rust daemon**: short explicit commands and one bounded per-job supervisor while a task runs. Omarchy-specific integration belongs in an adapter; the portable core/standalone UI must not require Hyprland or Quickshell. “Any Linux” is the portability goal, not a tested compatibility claim.

## Documentation

- [Full target specification](docs/implementation-spec.md): design, not completed implementation
- [Architecture and milestone boundaries](docs/architecture.md)
- [Security and manual-only invariants](docs/security.md)
- [Provider compatibility](docs/providers.md)
- [Validation report](docs/validation.md)
- [Licenses and upstream status](docs/licenses.md)

No upstream code or sprites are vendored in this scaffold. Preserve this distinction until upstream commits, assets and licenses are audited.
