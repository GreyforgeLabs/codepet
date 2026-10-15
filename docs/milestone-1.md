# Milestone 1: durable mock execution

Status: **implemented (mock only), 2026-10-14**, with the deviations listed below. This is
the contract for spec phase 1 (Rust side). No real provider, sandbox, staging, apply, decay
settlement, socket transport or desktop test is part of this milestone. Everything here
remains fail-closed for real providers. Gate results are in [validation.md](validation.md).

## Deviations from the original contract (2026-10-14 review fixes)

A review found that valid inputs could produce results larger than one 64 KiB frame. A
large draft could then hold the single active slot with no way to read its review digest,
and a committed mutation could answer `OUTPUT_LIMIT`. The fixes changed the contract:

1. **Lists carry task briefs, not full summaries.** `state.get` (`active_task`, `recent`),
   `history.list` and `task.recover` return the *task brief* shape below (no folder, scope or
   artifact text; an instructions excerpt). The brief includes the review digest, so a task
   can always be discarded or abandoned from a snapshot.
2. **New read method `task.get`** (`task_id` → full task summary, any state). The UI loads a
   draft with it before editing.
3. **Drafts are bounded by JSON-encoded size.** Folder, scope and instructions together must
   encode to at most 32 KiB of JSON (`INVALID_PARAMS` otherwise, before anything is stored).
   Core's byte limits still apply.
4. **Artifacts are capped at 24 KiB JSON-encoded** because `task.review` has no artifact
   paging yet. The spec's 2 MiB artifact limit with paginated review is deferred. The mock
   artifact no longer repeats the folder and scope.
5. **`history.list` has a compound cursor.** Optional `before_task_id` (requires
   `before_ms`) and a `next` cursor in the result; a page also stops early at 48 KiB of
   briefs. `before_ms` alone keeps its strict "created before" meaning.
6. **Acceptance and review recompute the review digest** from the stored artifact. A
   `review_ready` task whose artifact no longer matches is refused by `task.review` and
   `task.accept_artifact` with `REVIEW_CHANGED` (no state change, no reward);
   `task.recover` then moves it to `recovery_required`. `task.discard` deliberately does
   not require a matching artifact: it never rewards or touches files and is the way out.
7. **Override reasons:** one limit, 512 UTF-8 bytes after trimming, without NUL, shared by
   core, protocol and storage. An over-long reason is `INVALID_PARAMS`, not
   `OVERRIDE_REQUIRED`.
8. **Preflight path checks:** a folder is resolved through its deepest existing ancestor,
   so a symlinked parent cannot hide the state directory. The account home from
   `/etc/passwd` is refused in addition to `$HOME`. When no home can be determined,
   preflight refuses (`STORAGE_UNAVAILABLE`). `task.start` repeats the folder check.
9. **Token clock:** `task.start` also refuses a token when the clock reads earlier than the
   token's creation time (`TOKEN_EXPIRED`), so a backward clock step cannot extend it.
10. **Results always fit.** Tests build maximal drafts, artifacts, receipts and request ids
    and check that every method's response fits one frame. If a mutation's result ever did
    not fit, the `OUTPUT_LIMIT` message says the request may have been applied.

## Goals

1. Durable SQLite state with explicit migrations (spec §6.2): meters, settings, tasks,
   events, one-use preflight tokens, acceptances, rewards and request idempotency.
2. Transactional exactly-once rewards (REW-05): acceptance, reward row, meter update,
   state transition and request result commit in one SQLite transaction.
3. A versioned, bounded protocol (spec §7, IPC-01): newline-delimited JSON envelopes,
   64 KiB frames, typed error codes, shared by short commands and a later job socket.
4. Rust-owned UI state: short `codepet` commands return snapshots and perform explicit
   actions; QML only renders what Rust returns and never computes rewards.
5. A fake provider that cannot run any command: the mock supervisor spawns no process.
6. Tests for stale approvals, replayed tokens, repeated acceptance, cancellation across
   processes, process restart and transactions interrupted by an injected crash.

## Crates

| Crate | Owns | Dependencies |
| --- | --- | --- |
| `codepet-core` | Pure enums, draft validation, transition table, reward arithmetic, existing in-memory model | none |
| `codepet-protocol` | Envelopes, methods, error codes, typed params, bounded framing | serde, serde_json |
| `codepet-storage` | XDG state dir checks, SQLite schema/migrations, durable task engine, tokens, ledger, recovery, fault points | core, rusqlite (bundled), serde, serde_json, sha2 |
| `codepet` (CLI) | Short commands, `rpc` dispatcher, mock supervisor, error mapping | core, protocol, storage, serde_json |

No other third-party crates. Builds are offline against the cached registry.

## Core additions (pure, no IO)

- String forms (stable, used in SQLite and JSON): states `draft preparing running
  waiting_for_user verifying review_ready cancelling cancelled failed interrupted
  accepted_artifact discarded recovery_required`; categories `hunger thirst mental_health
  hygiene`; providers `mock codex opencode hermes claude_code`; verification `passed failed
  not_verified`. Each enum gets `as_str()` and `parse(&str) -> Option<Self>`.
- `State::allowed_transition(from, to) -> bool`, matching spec §8 for the states above.
  `State::is_terminal()`; `occupies_slot()` unchanged in meaning.
- `Meters::apply_reward(current_hundredths) -> new` (+2000, clamp 10000) as a pure helper the
  storage ledger calls; the in-memory `Engine` keeps working and its tests keep passing.

## Protocol

- Request `{"v":1,"id":"<1..128 of [A-Za-z0-9._:-]>","method":"…","params":{…}}`.
  Unknown top-level fields rejected. `v != 1` → `PROTOCOL_MISMATCH`.
- Response `{"v":1,"id":"…","ok":true,"result":{…}}` or
  `{"v":1,"id":"…","ok":false,"error":{"code":"…","message":"…","retryable":false}}`.
  If the id is unrecoverable, id is `null`.
- Event `{"v":1,"event":"task.state","seq":N,"task_id":"…","data":{…}}`.
- Error codes (SCREAMING_SNAKE): spec list `BUSY INVALID_STATE STALE_REVISION
  UNSUPPORTED_PROVIDER AUTH_UNAVAILABLE SANDBOX_UNAVAILABLE SCOPE_CHANGED BASELINE_CONFLICT
  DISK_LIMIT OUTPUT_LIMIT CANCEL_INCOMPLETE APPLY_RECOVERY_REQUIRED PROTOCOL_MISMATCH` plus
  `INVALID_REQUEST UNKNOWN_METHOD INVALID_PARAMS NOT_FOUND TOKEN_INVALID TOKEN_EXPIRED
  REVIEW_CHANGED ACKNOWLEDGEMENT_REQUIRED OVERRIDE_REQUIRED IDEMPOTENCY_CONFLICT
  RESYNC_REQUIRED CAPACITY STORAGE_UNAVAILABLE SCHEMA_UNSUPPORTED INTERNAL`.
- Methods: `hello state.get events.subscribe providers.inspect task.create task.update
  task.preflight task.start task.cancel task.review task.accept_artifact task.discard
  history.list settings.update pet.pause`, plus `task.recover` and `task.get`. `task.logs`, `task.apply`
  and `task.rollback` parse but return `INVALID_STATE` with "not implemented in mock milestone".
- Framing: a bounded reader that never buffers more than 64 KiB + 1 bytes for one frame,
  rejects NUL, invalid UTF-8, empty frames and frames without a trailing newline; an
  oversize frame is drained to its newline and reported. Writer refuses to emit a frame
  over 64 KiB (callers return `OUTPUT_LIMIT`).
- Typed params (serde, `deny_unknown_fields`); enum-valued fields are strings validated by
  the CLI through core's `parse`.

## Storage

- State dir: `$XDG_STATE_HOME/codepet` (fallback `$HOME/.local/state/codepet`). Refuse a
  relative XDG value, a symlinked dir, a dir not owned by the current UID (read from
  `/proc/self/status`), or group/other permission bits (fail closed, do not chmod an
  existing dir). Create missing dirs 0700. DB `codepet.sqlite3` pre-created 0600.
- Pragmas: `foreign_keys=ON`, `journal_mode=WAL`, `synchronous=FULL`, `busy_timeout=5000`.
  Every mutation runs in `BEGIN IMMEDIATE`, which serializes writers across processes.
- Migrations via `user_version`, current = 1. Newer DB → `SchemaUnsupported`, untouched.
  Failing `quick_check` → `Corrupt`, untouched. Upgrading an existing nonzero version first
  writes `VACUUM INTO` backup `codepet.sqlite3.bak-v<old>-<millis>`.
- Single active slot enforced durably by a partial unique index over tasks in slot-occupying
  states (`preparing running waiting_for_user verifying review_ready cancelling
  recovery_required`), mapped to `Busy`.
- Task ids: UUID v4 text from `/dev/urandom`. Preflight tokens: 32 random bytes hex, only
  the SHA-256 stored, one-use, 5-minute expiry, superseded by a newer preflight, invalidated
  by `task.update` and `settings.update`.
- Review digest: hex SHA-256 over a canonical JSON manifest (task id, revision, category,
  provider, artifact SHA-256 and length, verification, simulated flag).
- Acceptance: request id is an idempotency key. Same request id → stored response; same id
  with different params → `IdempotencyConflict`. A second acceptance of an accepted task
  returns the recorded receipt without another reward. `rewards.task_id` UNIQUE.
- Supervisor liveness: running rows record supervisor PID and `/proc/<pid>/stat` start time.
  `recover` treats a supervisor as alive only if both match; it never resumes work.
  Active states with a confirmed-dead supervisor → `interrupted`. `review_ready` stays
  readable after its digest is recomputed and matches; mismatch → `recovery_required`.
  If review metadata is missing, explicit recovery fills only the missing fields: a fresh
  opaque digest identifies abandonment, and missing verification becomes `not_verified`.
  Existing artifact bytes and digest are preserved. This also repairs tasks already stuck
  in `recovery_required`. The replacement metadata never enables acceptance; the user must
  explicitly abandon with the exact returned digest to release the slot without a reward.
- Fault points: `CODEPET_FAULT=<name>` is honored only in debug builds
  (`cfg(debug_assertions)`) and aborts the process at that point. Names:
  `accept_before_commit`, `accept_after_commit`, `start_after_commit`,
  `supervisor_running`, `review_before_commit`.
- Clock is injectable for tests (token expiry).

## CLI

- `codepet` / `--help`: help only. `providers`, `mock-demo` keep their current behavior.
- `codepet rpc`: reads exactly one request frame from stdin, prints exactly one response
  frame, exits 0 for `ok:true`, 1 for `ok:false`, 2 for usage errors.
- `codepet state`: shorthand for `state.get`, JSON on stdout.
- Mock supervisor: after `task.start` commits `preparing`, the same process prints the start
  response, flushes, then becomes the per-job mock supervisor: preparing → running →
  verifying → review_ready (or failed), with `mock.step_ms` (0..=2000, default 0) between
  steps. At every step boundary it re-reads state with a compare-and-set; if the task is
  `cancelling`, it records that no child process exists (mock) and marks `cancelled`. It
  spawns no process and never reads or writes the selected project folder.
- Mock outcomes (draft field `mock.outcome`): `artifact` (review, `not_verified`),
  `provider_failure` (→ `failed`), `check_failed` (review, `failed`, labelled SIMULATED).
  There is no mock outcome that claims `passed`.

## Deferred, stated plainly

Real providers, staging, sandbox, cgroups, transient systemd units, job socket and event
streaming helper, apply/rollback, decay settlement and checkpoint sidecar, retention, Qt
C++ standalone launcher, live Omarchy/Quickshell testing, resource measurement.

## Result shapes (shared by CLI, storage snapshot and QML)

Meters are integer hundredths (7000 = 70.0). Timestamps are Unix milliseconds.

`state.get` result:

```json
{
  "schema_version": 1, "protocol_version": 1, "last_seq": 12,
  "meters": {"hunger": 7000, "thirst": 7000, "mental_health": 9000, "hygiene": 7000},
  "decay": {"enabled": false, "paused": false},
  "settings": {"revision": 1, "decay_enabled": false, "decay_per_hour": [100, 100, 50, 50], "timeout_seconds": 1800},
  "active_task": null,
  "recent": [],
  "providers": [
    {"id": "mock", "label": "Mock", "available": true, "note": "Simulation only; runs no command"},
    {"id": "codex", "label": "Codex", "available": false, "note": "Not yet supported on this version"}
  ]
}
```

`providers` lists mock, codex, opencode, hermes, claude_code (labels Mock, Codex, OpenCode,
Hermes, Claude Code); only mock is available. `recent` holds at most 10 task briefs,
newest first. `active_task` is the brief of the slot-occupying task, if any.

Task brief (lists: `state.get`, `history.list`, `task.recover`):

```json
{
  "task_id": "uuid", "revision": 1, "state": "review_ready",
  "category": "mental_health", "provider": "mock",
  "mock": {"outcome": "artifact", "step_ms": 0},
  "instructions_excerpt": "…", "instructions_bytes": 30, "instructions_sha256": "hex",
  "created_at_ms": 0, "updated_at_ms": 0,
  "review": {"digest": "hex", "verification": "not_verified", "simulated": true, "artifact_bytes": 412},
  "receipt": null
}
```

`instructions_excerpt` is the longest prefix whose JSON encoding fits in 256 bytes.

Task summary (`task.get`, `task.review` and the mutations below):

```json
{
  "task_id": "uuid", "revision": 1, "state": "review_ready",
  "category": "mental_health", "provider": "mock",
  "folder": "/abs/path", "scope": ["src"], "instructions": "…",
  "mock": {"outcome": "artifact", "step_ms": 0},
  "created_at_ms": 0, "updated_at_ms": 0,
  "review": {"digest": "hex", "verification": "not_verified", "artifact": "…", "simulated": true},
  "receipt": null
}
```

Receipt: `{"task_id","category","amount_hundredths":2000,"verification","override_reason",
"request_id","accepted_at_ms"}`.

- `task.get` params: `task_id` → task summary.
- `task.create` params: `category provider folder scope instructions mock?` → task summary.
- `task.update` params: `task_id expected_revision` + the draft fields → task summary.
- `task.preflight` params: `task_id revision` → `{"task_id","revision","preflight_token",
  "expires_at_ms","summary":{"category","provider","folder","scope","instructions_sha256",
  "network":"none (mock)","writable":"none (mock: no project files are read or written)",
  "timeout_seconds"}}`.
- `task.start` params: `task_id revision preflight_token` → `{"task_id","state":"preparing","seq"}`.
- `task.cancel` params: `task_id` → task summary (idempotent).
- `task.review` params: `task_id` → task summary (must have a review).
- `task.accept_artifact` params: `task_id review_digest acknowledged override_reason?`
  (request envelope `id` is the idempotency key) → `{"receipt":{…},"duplicate":false}`.
- `task.discard` params: `task_id review_digest` → task summary.
- `task.recover` params: `{}` → `{"changed":[{"task_id","from","to"}],"active_task":brief|null}`.
- `events.subscribe` params: `after_seq limit?(≤100)` → `{"events":[event…],"last_seq"}`.
- `history.list` params: `before_ms? before_task_id? limit?(≤50)` →
  `{"tasks":[brief…],"next":{"before_ms","before_task_id"}|null}`. Pass `next` back as the
  cursor; `null` means no older task exists.
- `settings.update` params: `expected_revision decay_enabled decay_per_hour timeout_seconds`
  → settings object.
- `pet.pause` params: `paused` → `{"paused":bool}`.
- `hello` params: `client? versions?` → `{"protocol_version":1,"supervisor":null,"last_seq"}`.
