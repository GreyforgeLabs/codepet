# WP0 notes: offline source reads, decisions and spec contradictions

Date: 2026-10-16. Base: `release/v0.2.1` @ `21ed16b`, branch `v0.3`. Spec: run folder
`SPEC-v0.3.md` (rev 2), owner defaults of §11 approved, README addenda binding (On/Off,
supported clients Claude Code, Codex, OpenCode, Hermes and Grok; Muse deferred).

## 1. Offline source reads

No model call, no client session and no client config was read or written for these.

### 1.1 Codex: is an `async` `PermissionRequest` honoured? **Yes. Register it async.**

Source: `github.com/openai/codex`, files fetched raw at tag `rust-v0.162.1` and again at
`rust-v0.163.0` (the installed CLI is now `codex-cli 0.163.0`, newer than the spec's
0.162.1):

| File | sha256 at 0.162.1 | at 0.163.0 |
|---|---|---|
| `codex-rs/hooks/src/engine/discovery.rs` | `763704f5ae5f227d186dae8f5339edd6fde4f4a54c88c8ffb5ef9ad1b5a368eb` | identical |
| `codex-rs/hooks/src/engine/dispatcher.rs` | `ba1e8c8a6e2489b9f9d1272f208a251cfb3b0383292cb88adb1c57da3b5cc399` | identical |
| `codex-rs/hooks/src/events/permission_request.rs` | `552883009d6a012e37f7e1c27bf1e35028ccc92a9d5fb96200df6770279b737e` | identical |
| `codex-rs/hooks/src/engine/mod.rs` | `15d6f819a0de303dd98bfd3ae8050779188ef47e2517081bd4debac913f7d10a` | `Arc` refactor only; `execution_mode()` identical |

Findings:
1. `discovery.rs` (handler normalization): `let runs_async = r#async && event_name != HookEventName::SessionEnd;`.
   Only `SessionEnd` is forced synchronous (with the warning "running async SessionEnd hook
   synchronously"). `PermissionRequest` keeps `async: true`. The stored config keeps the
   literal `r#async` value, so the trust hash covers `"async": true`.
2. `engine/mod.rs`: `ConfiguredHandler::execution_mode()` returns `Async` for
   `Command { async: true }`; `can_apply_control_effects()` is true only for `Sync`.
3. `dispatcher.rs::execute_handlers_with_metadata`: an `Async` handler is handed to
   `command_runtime.schedule_async_hook(...)` and skipped (`continue`); only synchronous
   handlers are awaited and parsed for decisions.
4. `permission_request.rs::run` calls `dispatcher::execute_handlers` and resolves the
   decision only from the awaited results, so an async CodePet handler can never allow or
   deny an approval.

Consequence for WP3: register `PermissionRequest` with `"async": true` (six Codex lines to
trust: UserPromptSubmit, PermissionRequest, PostToolUse, Stop, Interrupt, SessionEnd).
`SessionEnd` must be written **without** `async` (it would run synchronously anyway and
log a warning). The `Interrupt` timeout normalizes into 1–3 s (`normalize_command_hook`),
so the stored `"timeout": 2` is kept as is.

### 1.2 Grok: headless flags from `grok --help`

`grok 1.0.52 (0920c8169e27)`, run as `grok --help`, `grok --version` and
`grok agent --help` with `env -i`, a throwaway `HOME`/`XDG_*` under the run's
`scratch/wp0/fakehome`, stdin `/dev/null`, a 10 s timeout. It printed help only (it created
an empty `~/.grok` directory inside the throwaway home, nothing else).

Headless evidence (contract: `agent_vectors.json` → `interactivity`):

| Token | Help text |
|---|---|
| `-p`, `--single <PROMPT>` | "Single-turn prompt. Prints the response to stdout and exits" |
| `--prompt-file <PATH>` | "Single-turn prompt from a file" |
| `--prompt-json <JSON>` | "Single-turn prompt as JSON content blocks" |
| `--output-format <FMT>` | "Output format for headless mode" |
| `--json-schema <SCHEMA>` | "Implies --output-format json" |
| `agent headless` | "Run the agent headlessly over the Grok WebSocket relay" |

Not treated as headless: `agent stdio` (editor/ACP front ends, a person is there, like
Hermes `acp`), `agent serve` (WebSocket server, unknown front end) and **`agent leader`**.
Important: the TUI can share a "leader" process (`--leader`, `[cli] use_leader`). If hooks
run inside `grok agent leader`, a naive "token `agent` → headless" rule would hide the
owner's real sessions. **WP7 must check** which process is the hook's nearest `grok`
ancestor in leader mode. `--agent <NAME>` is an unrelated option and never matches the
pair rule. Prefix forms `--single=…` etc. count (clap accepts `--flag=value`).

### 1.3 Codex subcommands (`codex --help`, throwaway HOME)

`exec` has the alias `e`, and `review` also "Run[s] a code review non-interactively". The
interactivity rule therefore is: `argv[1] == "e"`, or any exact token `exec` or `review`.
`codex app-server` (daemon-hosted TUI sessions) stays interactive.

## 2. Contradictions in the spec and how WP0 resolved them

1. **`RULES_NOT_ACKNOWLEDGED` vs `ACKNOWLEDGEMENT_REQUIRED`.** §6.2 names a new code for
   `pet.start`, but `pet.adopt`/`pet.rules` already answer an unacknowledged start with
   `ACKNOWLEDGEMENT_REQUIRED`. Resolved: reuse `ACKNOWLEDGEMENT_REQUIRED`; no new code.
2. **`pet.rename` limits.** §6.2 says 1–96 bytes and ≤ 32 grapheme clusters; core's
   `normalize_pet_name` (used by `pet.adopt`) allows 1–24 Unicode scalar values and ≤ 96
   bytes. Counting graphemes needs a new dependency (AGENTS.md: avoid new deps). Resolved:
   `pet.start`/`pet.rename` use core's rule (24 scalars is stricter than 32 graphemes), and
   map its refusal to `NAME_INVALID`. The §9.1 test (32 emoji, 128 bytes → `NAME_INVALID`,
   no constraint error) still holds.
3. **Export label `codepet.bar.v2` vs the v0.2 readers.** Both readers (`CodePetBar.js`,
   `qml/omarchy/BarState.js`) refuse any schema label other than `codepet.bar.v1`, so a v2
   label would make a still-loaded v0.2 bar ignore the file (§6.6 expects graceful
   degradation; it would be frozen, not broken). Resolved: Rust keeps writing the
   `codepet.bar.v1` label with the v0.3 blocks added (v1 readers ignore unknown keys) until
   WP5 and WP6 both accept v1 and v2; then `BAR_SCHEMA` switches to `BAR_SCHEMA_V2` (one
   line, owned by whichever of WP5/WP6 merges last, at the latest WP7). The contract schema
   accepts both labels; the fixtures use v2.
4. **K1's `/reload-plugins` exception** is listed under K1 (digits), but the literal has no
   digit; it trips **K2** ("plugin"). Resolved: the literal is exempt from both.
5. **K2 bans "turn", but §3 copy uses the verb**: "Turn on" (a K3 primary action),
   "Turn {name} into…", "! Turned off in Codex". Resolved: the lint allows the verb phrases
   `turn on/off`, `turned on/off` and `turn {name} into`; the noun stays banned.
   (`check_contracts.py` implements this; WP5's `copy_lint.py` should copy it.)
6. **`death_pending` location.** §6.6 says "clear `pet_care.death_pending`"; in schema v4
   the column is `life_vitals.death_pending`. Resolved in `schema-v5.sql` step 7.
7. **`events_7d`.** §6.2/§5.2 list `events_7d`, §6.6's `client_hooks` has only
   `events_total`. Resolved: `events_7d` is computed by `clients.status` from
   `agent_events` (7-day retention), not stored.
8. **Missing durable sources for export fields.** §8's `agent` block has `finished_at_ms`,
   `finished_app`, `finished_treat`, `finished_jar_full`, `hello_at_ms`, `hello_app` (§6.4
   step 5 even writes `meta.hello_app`) and `error_at_ms`/`error_app`, but §6.6 adds none of
   them except the seqs. Resolved: added `meta.finished_*`, `meta.hello_at_ms`,
   `meta.hello_app` and `agent_sessions.error_at_ms` (export = max over sessions).
9. **Treat-check bookkeeping.** The late-tool rule (§6.4 (b)) re-runs the check "once" and
   needs the finished turn's start time. Resolved: `agent_sessions.last_done_started_ms` and
   `last_done_rechecked`. Distinct-tool counting per turn needs the session, so
   `agent_events.session_hmac` was added (HMAC like every other key).
10. **Hermes approval rows** are "a lightweight row keyed by its HMAC" with no table named.
    Resolved: `agent_sessions.approval = 1` rows (never hold a turn); waiting → `waiting`,
    activity with the same key → `idle`.
11. **"Forget coding history clears the timestamps of `activity_turns`."** Clearing
    `given_at_ms` to NULL would put given treats back in the jar. Resolved: Forget sets
    `earned_at_ms = 0` and, for given rows, `given_at_ms = 0`; caps ignore 0. A trigger
    forbids `given_at_ms` going back to NULL.
12. **Helpers row persistence.** "Couldn't connect" + reason must survive after the job
    ends, and the spinner needs the job. Resolved: `client_hooks.last_outcome`,
    `last_reason`, `job_id`, `job_kind`; the export's client entry carries `first_seen_ms`
    (null = "Waiting for {app}"), `failed_reason` and `job`.
13. **Grok payload keys.** The allow-list (§5.8) lists `notification_type` and `tool_name`,
    but Grok sends `notificationType` and `toolName` (camelCase; `hook_event_name` is
    PascalCase while `hookEventName` is snake_case). Resolved: the camelCase aliases are on
    the allow-list; prefer `hook_event_name`. A Grok `Stop` with a reason other than
    `end_turn`/`channel_closed`/`shutdown` is dropped (the stale rule covers the turn).
14. **Non-interactive turns and `finished_seq`.** §6.4 increments `finished_seq` on every
    `turn_done`, but background helpers must "change no face". Resolved: only interactive
    sessions increment `finished_seq`; `hello_seq` counts any non-test event (it proves the
    connection).
15. **Several needs.** §2.4 says "top need by priority: food, water, play, wash". Resolved:
    needs at now/critical outrank needs at soon; inside a group, that order decides.
16. **`no_pet` row.** The shared priority has no row for "no life" (first run, after a
    goodbye). Added as row 1.5 (after Off): bar `egg`, no line, no overlay.
17. **Removed CLI/protocol pieces still present.** WP0 keeps `providers`, `mock-demo`, the
    supervisor and the 17 removed methods working; WP1 moves them behind `legacy-tasks`.
    `Method::V03_REMOVED` lists them so WP1 cannot miss one.
18. **`agent_vectors.json` path.** §9.1 puts it in `crates/codepet-core/tests/`; WP0's list
    puts it in `docs/v0.3/contracts/`. Resolved: one canonical file in the contracts folder;
    `codepet-core::agent` tests `include_str!` it (no copy to drift). WP2 may move it and
    update the two include paths (core test, protocol test reads `protocol.json` likewise).

## 3. Other WP0 decisions (not contradictions)

- New error codes appended after `NO_ACTIVE_PET` (positions of the v0.2 lists are stable):
  `NOT_IMPLEMENTED`, `ALREADY_ARMED`, `NAME_INVALID`, `NO_TREATS`, `PET_NEEDS_CARE`,
  `TEST_STATE`. None is retryable.
- New methods appended after `lifecycle.rules`; all mutating (`clients.status` writes
  `client_hooks` and re-exports). They validate params, then answer `NOT_IMPLEMENTED`
  before opening storage, so the skeleton creates no state.
- `codepet hook` already honours O2 in the skeleton (exit 0, no output, no reads, no
  directories). The other new commands print one `NOT_IMPLEMENTED: …` stderr line, exit 1.
- `StatePaths` gains `data_dir` (`$XDG_DATA_HOME/codepet`, else `$HOME/.local/share/codepet`;
  an unusable `XDG_DATA_HOME` is ignored rather than failing) for the shim, integrations
  and staging. Nothing creates these paths yet.
- `legacy-tasks` is declared (empty) in all four crates; the CLI's feature enables it in the
  other three. CI now runs clippy and tests with and without it.
- Event kinds, app/session states, client ids and every numeric rule live in
  `codepet-core::agent` and are tested against `agent_vectors.json` `constants`.
- `strings.json` keys not named by the spec were chosen in WP0: `react.<need>.plain`,
  `err.name_too_long`, `more.*`, `btn.sleep`, `menu.*`, `bar.hi`, `memorial.line`,
  `btn.meet_new`, `codex.title`, `help.*`.
- The shared-priority function's contract (inputs: the export, `now`, the `seen` seqs;
  outputs: state, bar, line + vars, overlay, pulse, treats visibility, bubble, `wake_at_ms`)
  is in `agent_vectors.json` `priority`; `wake_at_ms` is null while Off (K9: zero timers).

## 4. Not done in WP0 (by design)

No migration code, no fold, no hook body, no installer, no QML, no icons. The v5 DDL is a
contract file only (`SCHEMA_VERSION` stays 4). Live client checks (§9.5) are WP7.
