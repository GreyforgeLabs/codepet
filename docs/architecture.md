# Architecture (v0.6)

CodePet combines a desktop companion, bounded pet-owned coding work, and optional
metadata-only observation of coding sessions started outside CodePet. These are
separate pipelines with separate privacy and reward rules.

## Components

- `codepet-core`: pure care, lifecycle, growth, agent normalization and priority rules.
- `codepet-protocol`: bounded, versioned pet/observer JSON requests and typed parameters.
- `codepet-storage`: SQLite schema 6, immutable life history, durable reward receipts,
  bounded observer state and atomic bar exports.
- `codepet` CLI: short-lived pet RPC, migration, hook/fold and client jobs;
  `work-rpc` controls owned jobs, and a bounded `work-run` process supervises execution.
- `work/{filesystem,review,runtime}.rs`: bounded source handling, sealed review/application,
  and contained provider supervision. RPC orchestration builds one status snapshot
  and derives its private-data-free export from the same result.
- `omarchy-plugin/`: bar cell, care/stats popup and attended-presence recorder.
- `codepet-pet` (`crates/codepet-pet`): the native desktop pet. A single-threaded Wayland
  layer-shell client (smithay-client-toolkit, tiny-skia, ab_glyph, image-webp) that draws
  the parsed `bar-state.json`, watches its inputs with inotify, keeps one timer, and sends
  only explicit care, scout and placement requests. Its face and scene rules are a Rust
  port of `qml/shared/*.js`, checked against them by a differential test. It is the
  one resident CodePet process while the pet is shown (about 4 MB measured).
- `qml/work.qml`: compact card window (opened by a click on the pet) and the explicit
  native task/review window.
- `qml/shell.qml`: the previous Quickshell desktop pet, kept as the
  `CODEPET_PET_ENGINE=quickshell` fallback.

## Pet-owned work

```text
bounded local repo metadata discovery → cooldown/readiness checks
    → bounded source preflight → isolated read-only scout with the selected client
    → suspected finding → explicit Fix → isolated code candidate
    → explicit Review/Apply → conflict-checked writes + one durable reward
```

The backend chooses a discovered repo; a scout request cannot supply an arbitrary
path or prompt. A single attempt examines at most ten eligible source trees within
shared budgets, then launches at most one provider job. A pending finding or review
keeps the active slot. No automatic fix, application, retry queue or crash resume
follows. Advanced research/custom jobs require explicit scope, task and Start.

Owned execution uses user-systemd resource limits and Bubblewrap PID/mount
isolation. The original source folder is absent from the provider namespace.
Read-only scouts/research and writable code jobs share the containment controller;
cancellation or deadlines end its process tree. No idle Rust daemon polls.

Private job files retain bounded baseline hashes, source copies, reports and review
changes. Apply compares the reviewed digest and destination bytes, preserves
recovery data, and records a permanent, life-bound acceptance receipt. Accepted
jobs give 20 XP and matching care once. Scouting grants nothing. See [work](work.md)
and [security](security.md) for exact limits and concurrency caveats.

## External coding-session observations

```text
coding app event → silent shim → bounded normalizer → metadata-only inbox
    → short-lived fold → SQLite → bar-state.json → bar + desktop pet
```

The observer hook never opens SQLite or creates directories. It records enums,
counts, flags, HMAC identifiers and timestamps; never transcripts, prompts, source
or paths. The fold takes `fold.lock` before the database, batches events and exits.
External events cannot settle care, arm, revive or kill a pet, or start owned work.

A qualifying interactive finished turn can earn one treat, uniquely keyed by
session and turn. Durable caps are one per session per thirty seconds, twelve per
rolling hour, forty-eight per rolling day and ten in the jar. Errors, cancellation,
tests and tool calls alone never earn. Duration qualifies work; it never awards XP.
Giving a treat explicitly grants five XP once. Owned jobs load no observer hooks.

## UI, power and storage

The surfaces watch atomic `bar-state.json` and the display-only `work-state.json`.
The latter omits private paths, prompts, reports, findings and errors. The compact
card explicitly hydrates its private details through `work-rpc get`. File changes
merge display state; only relevant task transitions need a full read. An eligible
one-shot deadline can request a scout; there is no fast idle polling loop.

The bar counts attended presence. Off freezes care and earning and exits the
desktop pet. New jobs require On, an awake living pet, mood and energy; automatic
scouts also require a started life. Turning Off, changing life or starting a nap
while owned execution/application is active is refused until that work is stopped
or recovered. Cancel and review recovery do not depend on new-job readiness.

Only `codepet migrate` and pet RPC `state.get` migrate. SQLite uses schema 6 (per-life care tempo);
existing lives, birth timestamps, care, XP and observer connections are preserved.
Pet mutations use one `BEGIN IMMEDIATE`, settle/body/finalize ordering and durable
request IDs. The work controller serializes its slot separately and freezes
attended decay during active work/review.

The additive bar export retains the `codepet.bar.v1` envelope for older readers;
current readers accept v1 and v2. The old task tables remain read-only in the
default build; `legacy-tasks` is retained as non-default compatibility code.
The [v0.3 contracts](v0.3/contracts/README.md) remain the observer contract, not a
prohibition on the separate owned-work pipeline. [Validation](validation.md)
distinguishes tests from actual desktop and provider qualification.
