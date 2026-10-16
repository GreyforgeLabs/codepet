# Architecture (v0.3)

CodePet observes coding apps and maintains a desktop pet. It never launches a
coding app or reads its transcripts or project files.

## Components

- `codepet-core`: pure care, lifecycle, growth, agent normalization and priority rules.
- `codepet-protocol`: bounded, versioned JSON requests and typed parameters.
- `codepet-storage`: SQLite schema v5, receipts, immutable life history, bounded
  agent state, durable treat limits and atomic bar exports.
- `codepet` CLI: short-lived RPC, migration, hook, fold, client jobs and diagnostics.
- `omarchy-plugin/`: the bar cell, kid-facing popup and attended-presence recorder.
- `qml/shell.qml`: the Quickshell desktop pet, reading the same exported state.

## Data flow

```text
coding app event → silent shim → bounded normalizer → metadata-only inbox
                                                      ↓
                                             short-lived fold
                                                      ↓
                                           SQLite → bar-state.json
                                                      ↓
                                            bar + desktop pet

explicit UI action → one codepet rpc → durable transaction → export
```

The hook never opens SQLite or creates directories. It records enums, counts,
flags, HMAC identifiers and timestamps, then may request a fold. The fold takes
`fold.lock` before opening the database, batches events and exits. Agent events
cannot settle care, arm, revive or kill a pet. No persistent Rust process polls.

A qualifying interactive finished turn can earn one treat, uniquely keyed by
session and turn. The caps persist across restarts: one per session per thirty
seconds, twelve per rolling hour, forty-eight per rolling day, and ten in the jar.
Errors, cancellation, tests and tool calls alone never earn. Duration qualifies
work; it never awards XP. Giving a treat is explicit acceptance and grants five
XP exactly once through a request-id receipt.

## UI and power

Both surfaces watch atomically replaced `bar-state.json`. Filesystem events cause
reads, never RPC. Startup and popup opening call `state.get`; explicit mutations
return and republish state. A successful reply triggers a file re-read. Desktop
launch decisions live in `DesktopPetFlow.js`; face priority is generated from the
shared agent vectors. No retired task window is routed.

The bar records presence only while enabled and attended. Off freezes care and
earning, stops its timers, and makes the desktop pet exit. The shim checks the Off
flag before starting the binary. The bar cell stays available to turn CodePet on.
Codex's client-owned login shell precedes that check.

`pet.memories` is a read-only, bounded journal projection with exclusive sequence
pagination. It never exports raw journal data, acknowledges growth or settles time.

## Storage and compatibility

Only `codepet migrate` and RPC `state.get` can migrate. The installer keeps recovery backups, migrates saved state and rescans the shell. Migration retains pet identity, care,
growth and life history. The old task tables are preserved; the default build
cannot route their methods. `legacy-tasks` is transitional and non-default.

Mutations use one `BEGIN IMMEDIATE`; UI/presence mutations settle, execute,
finalize, increment `state_version`, and commit before exporting. Request IDs make
retryable mutations idempotent. Lifecycle triggers protect ended lives and history.

The v0.3 export deliberately retains the `codepet.bar.v1` envelope label with
additive fields. This lets a still-loaded v0.2.1 reader survive the upgrade window.
Both v0.3 readers accept v1 and v2. The SQLite version is independently v5.

See [contracts](v0.3/contracts/README.md), [security](security.md), and
[validation](validation.md) for policy and actual verification.
