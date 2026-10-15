# Architecture (milestone 1)

Rust owns all task state and rewards. QML renders what Rust returns and sends explicit requests. There is no daemon: each `codepet` invocation answers one request and exits, except that a successful `task.start` continues as that task's per-job mock supervisor.

## Crates

- `codepet-core`: pure enums with stable string forms, draft validation (lexical paths, byte limits), the state transition table, reward arithmetic and the shared override-reason limit. It also keeps the earlier in-memory `Engine` used by `mock-demo` and core tests. No IO.
- `codepet-protocol`: request/response/event envelopes (v1), method names, error codes, typed params with `deny_unknown_fields`, and a bounded frame reader/writer (64 KiB, no NUL, valid UTF-8, trailing newline). No transport and no state.
- `codepet-storage`: the durable task engine on SQLite (bundled). It checks and creates the state directory (`$XDG_STATE_HOME/codepet`, 0700, owned by the effective UID, no symlink), opens the database with WAL and `synchronous=FULL`, migrates by `user_version` (backing up an older version first) and refuses newer or corrupt databases without modifying them. Every mutation is one `BEGIN IMMEDIATE` transaction; task state changes append a `task_events` row. Explicit care/rest mutations have idempotent receipts, and snapshot commands settle persisted sleep and checkpoint live execution without a resident timer (so `state.get` is classified as mutating; with nothing to settle it is a plain read, and a checkpoint never invalidates preflight approvals). It owns preflight tokens, the single active slot (a partial unique index), review digests, the acceptance/reward ledger with request idempotency, explicit recovery and debug-only fault injection points.
- `codepet` (CLI): `providers`, `state`, `rpc`, `mock-demo` and help. `rpc` reads one frame from stdin, dispatches it to storage, prints one frame and exits 0 or 1. The mock supervisor lives here.

## Request flow

1. The UI sends one envelope to `codepet rpc` (a new process per request).
2. The CLI parses and validates the frame and params before opening storage. Unknown or unimplemented methods are refused without touching the state directory.
3. Storage performs the action in one transaction and returns a typed result or error. Results are bounded so that every response fits one frame: lists use task briefs, drafts are capped at 32 KiB of JSON and artifacts at 24 KiB (see [milestone-1.md](milestone-1.md)).
4. After `task.start` commits `preparing` with the process's PID and `/proc` start time, the same process prints the response and then runs the mock supervisor. Each step is a compare-and-set that honors a `cancelling` request made by another process. The mock spawns nothing and never reads or writes the selected folder; it seals a simulated artifact that is never marked `passed`.
5. `task.recover` is explicit. A job whose recorded supervisor is gone becomes `interrupted`; a review whose digest no longer recomputes becomes `recovery_required`. Nothing is resumed or retried.

## QML

`qml/shared` holds the panel, task form and review components. They take a `host` object with `snapshot`, `busy`, `lastError` and `request()`. `qml/standalone/PreviewHost.qml` is a pure-QML host with fixed fixtures; it refuses every state-changing method and is not connected to Rust. `qml/omarchy/HostAdapter.qml` is the Quickshell host: one `codepet rpc` process per request, argv only, no shell. It has been exercised against a test stub, not the real binary. Quickshell imports appear only in the host entry `qml/shell.qml`, under `qml/omarchy` and in its test harness. See [qml/README.md](../qml/README.md).

## Limits of this milestone

- Paths are checked lexically by core and, at preflight and start, against the home directory and the CodePet state directory (resolved through symlinks). There is no staging, canonical scope check, ownership or mount policy for project folders, because nothing reads or writes them yet.
- Supervisor liveness is PID plus start time from `/proc`; there is no containment, cgroup or transient unit. A mock supervisor that is a child of the UI process may stop with it; recovery then marks the task `interrupted`.
- Settings are stored and validated; decay is not computed.
- History is kept indefinitely; there is no retention, export or deletion yet.
- There is no event streaming. Clients poll nothing on their own; they refresh after explicit actions and may read persisted events with `events.subscribe`.

Future target (spec): explicit short commands plus one per-job supervisor that owns staging, containment, execution and review; an optional job-scoped event helper; Omarchy/Quickshell as a host adapter separate from a plain Qt standalone launcher.

## Care and progression

Schema v2 adds independent care/rest counters, per-task start debit/execution accounting and a task-keyed demo XP ledger. Rust snapshots expose readiness and separate real/demo progression; QML never computes authoritative rewards or grants readiness. See [care-progression.md](care-progression.md).
