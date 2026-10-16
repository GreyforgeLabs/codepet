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

19. **Hermes non-interactive sessions** (§5.6 "only cli/tui/desktop/acp is forwarded" vs §6.4
    "non-interactive"). Resolved for §5.6: the plugin drops other or unknown platforms and never
    sends them, and it never sends `interactive:false`. Hermes background or gateway work
    therefore never reaches CodePet and can never set `helpers_busy`; approval events (no
    platform) are sent only for surface `cli` or `transport:*` and count as interactive.
20. **`clients.status` has no receipt** although §6.2 says every new mutating method takes the
    request id. It is a file-only re-check whose answer must reflect the files now; a replayed
    answer would be stale, and the method is idempotent. Every other added method has a
    receipt (`app.set_enabled`, `app.set_helpers` and `agent.forget_history` gained one).
21. **`completed`/`failed`/`interrupted` leave the §5.8 allow-list.** The Hermes plugin maps
    `on_session_end` to `turn_done`/`turn_failed`/`turn_cancelled` itself and sends the kind;
    the hook never reads those keys. The plugin payload is frozen in
    `plugin-payload.schema.json`; the hook stays a lenient reader (bad `tools` -> 0, a
    non-boolean `interactive` -> true, `codepet_v` must be the integer 1).
22. **Ready check ownership.** §6.4 step 4 lets test events "set test_state"; with several
    tested clients that would flip `done` -> `failed`. Resolved: the hook compares
    `codepet_test` with `$STATE/test.nonce` and drops any mismatch or missing nonce
    (`test_nonce`), so a stale fixture can never fake "{name} saw {app}!". The fold keeps test
    records only while `meta.test_state = 'running'` (else outcome `dropped_test`, new) and never
    changes `test_state`. The clients job sets `running` (+ `test_at_ms`, new
    `meta.test_client`), judges arrivals from `agent_events`, sets `done`/`failed`, removes the
    nonce and exports. `agent.test.client` feeds the testing face and "Open a new {app} window.".
23. **Activity without a tool id or turn key.** A tool-less activity (Hermes clarify/approval
    replies, OpenCode permission replies) never counts as a tool and never spends the late
    re-check; it still turns `waiting`/`idle` into `working`. A keyless activity belongs to the
    open turn (Claude's `Notification` has no `prompt_id`, so the "second permission prompt"
    vector now has a keyless form). With no open turn, an activity ends `waiting` (-> `idle`)
    instead of leaving the pet waving until the 30-minute stale rule.
24. **Faces the priority table lacked.** `welcome` (row 1.6: a living unarmed life, i.e. the
    owner's upgraded Ginger before Let's go!: bar thumb + "Hi!" sparkle, no Home), `paused`
    (row 2 with asleep: `pet.pause` stays in the protocol, Home shows one Wake up that resumes),
    `testing` (row 5 when no app works: the ready check types for up to
    `test_done_delay_ms + test_arrival_ms`) and the `grew` celebration (first candidate of row 6,
    driven by `unseen_events`, no time window, cleared by the popup's `pet.events_seen`).
    Treats are hidden while paused, and `treats.give` answers `PET_NEEDS_CARE` then.
25. **Off and mutating methods** (only `treats.give` was defined). `protocol.json`
    `off_behaviour` splits every live method: cosmetic and bookkeeping methods,
    `clients.status` and `clients.disconnect` (uninstall needs it) run; anything that ages, arms,
    grows or ends a life, plus `clients.connect` and `clients.test` (the shim exits on `off`, so
    the ready check could only fail), answer `INVALID_STATE`. `codepet connect` says
    "CodePet is off. Turn it on first: codepet on". The frozen reason `off` comes right after
    `no_life`.
26. **Aggregate bound.** One `apps[]` entry per app cannot describe a lower state hidden under a
    higher one. An entry stays valid until its winning rows expire; a lower state of another
    session of the same app reappears at that session's next event (the no-polling rule), not
    by a timer. The `aggregate` vectors pin every other edge (half-open stale windows, the error
    window, approval rows, ordering, `helpers_busy`).
27. **Hash inputs.** `session_key` was the PK for both sessions and approval rows, and two
    clients sharing a raw session id could collide. Every HMAC now has a domain tag and the
    client id in its input (`agent_vectors.json` `hmac_keys`, vectors with a fixed salt). A
    Hermes approval event's event-id session field is `~ak:<approval_key>`; a keyless
    `turn_start` uses turn `~ev:<event_id>` (`~` never occurs in an id).
28. **Grok payload with only `hookEventName`.** It maps through a snake_case -> PascalCase table
    instead of being dropped; `hook_event_name` still wins when both are present.
29. **`client_hooks` rows.** `clients.status` writes `installed`, `detected_version` and
    `checked_at_ms` (new columns) for all five clients on every run, so `state.get` and the
    export never detect; the export's client entry gains `installed` and `version`. Rows are
    never deleted; before the first status the map is empty.
30. **Journal kinds without a table rebuild.** Clearing `death_pending` writes `recovered`
    with `dedup_key 'v5_migration'` and `data {"reason":"v5_migration"}`; treat evolution keeps
    `evolved` / `stage:<n>` with `task_id NULL` and the `treats.give` request id. Both kinds
    are already allowed by the v4 CHECK.
31. **`codepet connect --all`** matches the UI's Connect all: installed, unconnected clients
    without a running job, never Codex (named explicitly: `codepet connect codex`). Job ids are
    `job-` + 24 lowercase hex (they name `$STATE/jobs/<job_id>.json`); the DDL, the export
    schema and the job result schema all enforce the pattern.
32. **Helpers rows the spec did not word.** A running disconnect shows `row.disconnecting`
    ("Stopping…"), a running test `row.testing` ("Checking…"); "Turned off in Codex" offers
    Show me on an adult `codex.off*` page; a failure while still connected shows "Couldn't
    connect" with Try again and Stop watching. Time buckets: "just now" < 5 minutes, then the
    local calendar day (`helpers_rows`).
33. **Shim and command bytes** were described in prose only. `shim.sh.template`,
    `shim-tombstone.sh` and `commands.json` (generated by `tools/gen_commands.py`) freeze the
    shim, every stored command, the Claude and Grok files, the six Codex groups with their
    expected `trusted_hash`es, the argv forms and the ready-check payloads. Synchronous
    handlers omit `async` (Codex hashes it as `false`); Codex `SessionEnd` never carries it.
34. **Icon source key (WP4).** §4.2 says every sheet uses a flat `#00FF00` key, but the
    care-aquatic plant arch and the status calm leaf are green. `icons.json`
    `format.source_key` is now per sheet: green by default, magenta `#FF00FF` when a subject
    is green, recorded as `key` per sheet (and per patch) in `tools/petart/icon_sheets.json`.
    The same change adds single-cell `patches` (the care-floor `play` icon is a red striped
    ball, the painted twin of `badge_ball`; the cat's yarn waits for a `careIcons` override)
    and keeps nonces, rollout ids and local paths out of the shipped `PROVENANCE.md`.

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
- The shared-priority function (`shared_priority`, `petstate.js` `sharedPriority`; inputs:
  the export, `now`, the `seen` seqs; outputs: state, bar, line + vars, overlay, pulse, treats
  visibility, bubble, `wake_at_ms`) is in `agent_vectors.json` `priority`; `wake_at_ms` is null
  while Off (K9: zero timers). It is distinct from Rust `agent::pet_state`, the aggregate that
  builds `agent` (`aggregate` section).
- The popup's screen choice (`popup_routing`), the Connect card's apps (`connect_card`) and the
  Helpers rows (`helpers_rows`) are contracts for WP5, replayed by the checker.
- `memories.*` copy is keyed by `life_events.kind`; `mode_changed`, `configured`,
  `health_changed` and `death_pending` never reach the kid-facing journal. Help bodies
  (`help.*.body`) hold the adult text, exempt from K1/K2/K8 like the rest of Help.
- Icons: `egg` and `pencil` on a new `ui` sheet; the bar sheet gains `badge_egg` and
  `badge_sparkle`; `icons.json` `bar_cell` maps every priority `bar` value to what is drawn.
- Pixel budgets gained More (488), the Helpers row sheet (376), the memorial card (416) and the
  goodbye confirmation (500).

## 4. Not done in WP0 (by design)

No migration code, no fold, no hook body, no installer, no QML, no icons. The v5 DDL is a
contract file only (`SCHEMA_VERSION` stays 4). Live client checks (§9.5) are WP7.

## 5. WP1 review resolutions (2026-10-16)

Code now matches the frozen contract where WP1 had drifted. Nothing here changes a schema,
fixture or vector.

1. **A fresh install has no pet** (§2.1, K7, `popup_routing` `fresh-no-pet -> first_run`).
   Schema v4 still creates the unarmed first-run life #1 "Ginger" when it upgrades a v1-v3
   database, and v0.2-shaped databases keep it. A brand-new database created by v0.3 gets no
   life: `Store::migrate` sets a connection-private TEMP marker that v4 checks, so v4 writes
   only the empty `life_vitals` row and the presence clock. `pet.start` then adopts the picked
   species under its `defaultName` (or the typed name), origin `adopted`, ordinal 1. Lives
   stay immutable history; nothing is deleted.
2. **`db_id` without a life.** It was a digest of life #1's id. A database from v0.2 keeps
   exactly that value (life #1 origin `migrated`/`first_run`); a database created by v0.3 uses
   a domain-separated digest of `meta.hook_salt` (written once by v5, never rotated), which
   the first adoption does not change.
3. **Concurrent upgrades.** The `user_version` check, the `VACUUM INTO …bak-v<old>-<ms>`
   backup (through a second connection) and the migration all run under one writer lock.
   Of two processes upgrading at once (installer `codepet migrate` vs the reloaded bar's
   `state.get`, §6.6), the first backs up and migrates; the second finds schema 5 and reports
   "up to date". A taken backup name gets a `-<n>` suffix instead of failing.
4. **`codepet state` never migrates** (§6.2 and AGENTS.md win over §6.3's "unchanged"): it
   refuses an older database with `SCHEMA_UNSUPPORTED` and the `codepet migrate` hint.
5. **Hardcore needs the typed name in Rust** (§11-2, §3.6): `pet.rules` takes an optional
   `confirm_name`; gentle -> permadeath answers `INVALID_PARAMS` unless it matches the pet's
   name after `normalize_pet_name` (as `pet.retire`). Absent, it stays out of the receipt
   digest. `pet.start`/`pet.adopt` with `mode: permadeath` are not gated (adoption-time
   choice, as in v0.2; the v0.3 UI always sends `gentle`).
6. **Off + asleep is a plain read.** A nap frozen by Off no longer makes `state.get` take
   the writer lock and commit a no-op settlement.
7. **Open (owner decision): Forget vs the earning caps.** Note 11 makes Forget zero
   `activity_turns.earned_at_ms`, which the caps ignore, so "Forget coding history" also
   resets the 30 s / 12 per hour / 48 per day caps (only the jar of 10 remains), against
   X3 "capped durably". Options: keep an hour-coarsened earn time for rows earned in the last
   24 h (the precision retention keeps after 24 h anyway), or say in X3 and Help that Forget
   resets the caps. Unchanged until the owner decides.

## 6. Amendments after the freeze (WP2 review, 2026-10-16)

Each amendment changed the generator, the contract file, `check_contracts.py` and every
consumer together. The SPEC text (§5.1 shim, §5.8 event id, §6.4) predates them; where they
differ, these entries win.

1. **The shim and the tombstone silence the shell itself.** With stdin never closed, the
   0.3 s drain ends with `timeout -s KILL`, and bash (`/bin/sh` on Arch) reports the killed
   job ("<shim>: line 5: <pid> Killed ...") on the shell's own stderr. Shell clients redirect
   it in their stored command, but OpenCode and Hermes run the shim directly (argv form), so
   K10 failed for them. Resolved: `exec >/dev/null 2>&1` is the first command of
   `shim.sh.template` and `shim-tombstone.sh` (stdin stays for the drain); the
   `commands.json` sha256 values were regenerated (the stored command strings and the Codex
   trust hashes do not include the shim bytes, so they are unchanged). The shim smoke now
   runs the argv form and the tombstone with stdin left open.
2. **State changes are never deduped by the 3 s bucket.** SPEC §5.8 assumed legitimate
   repeats "have different payloads or times", but a second Claude Code permission prompt
   for the same tool is byte-identical (the Notification has no turn or tool id and a
   generic message), as is a repeated Codex `PermissionRequest`, so the second "needs you"
   was swallowed and the pet showed "working" while the client waited. Resolved: for the
   idempotent kinds `session_start`, `waiting`, `idle_prompt` and `session_end`
   (`event_ids.state_kinds`) the bucket of the event id and of the dedupe key is `at_ms`
   itself, so only deliveries of one firing within the same millisecond share an id. A real
   duplicate delivery of a state change is harmless. `activity`, `turn_start` and the
   closers keep the 3 s bucket.
3. **`finished_jar_full` describes only the last finished turn.** The reference model
   changed the flag only when a turn qualified, so after a full-jar turn a short tool-less
   turn still exported `true` ("jar is full", even after the treats were given); the Rust
   fold cleared it. Resolved (WP2 choice, flagged for the owner): every interactive
   `turn_done` clears it, and the treat check of that turn sets it when the verdict is
   `jar_full`, including the one late-tool re-check (which the Rust fold did not record).
   New `sessions` vectors: `jar_full_describes_only_the_last_finished_turn` and
   `late_tool_recheck_on_a_full_jar_sets_jar_full`.

## 8. WP5 popup decisions and contract requests (2026-10-16)

Numbered 8 because the fix package's "Owner decisions after wave 1" is section 7 (branch
`v0.3-fix`). Nothing in a frozen contract file changed. These are the choices WP5 made where
the spec left room, and what WP5 asks the contract owners to adopt.

1. **Popup window.** The popup is an Omarchy `KeyboardPanel` (the `PopupCard` API with
   keyboard focus on demand), not a `PopupCard`: an xdg-popup gets no keyboard focus at map
   time, and the name field and the typed-name confirmations (Hardcore, Say goodbye) need
   typing. Offscreen Quickshell has no layer-shell backend, so `test_bar_plugin.py` stages a
   test-only stand-in (`qml/tests/stubs/live/KeyboardPanel.qml`). Keyboard entry in the real
   shell is not verified offscreen (WP7 nested-rig item).
2. **Opening Home from the desktop pet.** The widget implements Omarchy's bar-widget panel
   contract, which WP6's overlay uses (`omarchy-shell -q shell summon greyforge.codepet`,
   WP6 notes item 1): `open()` opens the popup at Home (on the Off cell it turns CodePet on
   first, the §3.1 rule), `close()` closes it and `opened` reports it. The bar leader also
   registers the IPC target `greyforge.codepet` with `openHome()`, `close()` and `toggle()`
   for direct calls (`qs ipc -p $OMARCHY_PATH/shell call greyforge.codepet openHome`).
3. **`petstate.js`.** WP5 ships a hand port of the contract's `priority` reference as
   `omarchy-plugin/petstate.js` with the frozen API `sharedPriority(export, now, seen)`;
   `qml/tests/test_bar_model.mjs` replays every `priority` vector against it. WP6 owns the
   generated module: at merge, WP6's generated file replaces WP5's unchanged (an add/add
   conflict resolves to WP6's copy) and the same vectors must still pass.
4. **Copy WP5 needed that the contract lacks** (in `tools/ui/strings-wp5.json`, merged into
   `omarchy-plugin/strings.json` after every contract key; the generator refuses an
   override): `btn.back` "Back", `btn.next` "Next" (Codex guide pager), `name.type` "Type
   a name" (empty name field), `err.generic` "Oops! Please try again." (any failed request;
   `NAME_INVALID` shows `err.name_too_long`), `helpers.none` "No coding friends found yet."
   (Helpers with no installed app). Request: add them to `strings.json`; then delete them
   from `strings-wp5.json`.
5. **Status line vs a monospace theme font (K3 against K13).** Omarchy's `Style.font.family`
   is the user's monospace font (about 0.6 em a character). At `Style.font.heading` the card
   holds about 35 characters, so `act.working.two` with most app pairs ("Ginger is helping
   Claude Code and OpenCode." is 43), `aquatic.mood.fainted` and `robot.mood.fainted` do not
   fit. The line keeps one line by stepping its size down (`fontSizeMode: HorizontalFit`)
   to no less than `Style.font.body`, which is a K3 deviation for those lines only;
   `copy_lint.py` lists them and fails a line that would not fit even at body size.
   Request: shorter copy, e.g. `act.working.two` = "{name} helps {app} and {app2}." and
   the fainted lines without the second sentence.
6. **Calm mode** is the plugin setting `calm` (default off). Omarchy exposes no system
   reduced-motion signal, so "follows system" means off. It stops the popup's animation
   clock, the tile and badge pulses, and reaches the desktop pet as
   `CODEPET_REDUCED_MOTION=1` on the next launcher call (WP6: honour it; a running overlay
   learns it only on its next start).
7. **Pet on my desktop** follows WP6's flow (WP6 notes item 2, `DesktopPetFlow.js`): the
   switch is `overlayOnLoad`; Let's go! sets it and launches `show`; the More and menu
   switches launch `show` when turned on and nothing when turned off (the overlay reads the
   setting and exits by itself); the leader's startup shows the pet only with the switch on,
   CodePet On and an armed life; turning CodePet on shows it when the switch is on. After
   WP6 merges, the widget can call `DesktopPetFlow.js` instead of its own copies of these
   rules (same behaviour). With a `stateHome` test override every `codepet rpc` child gets
   `CODEPET_TEST_STATE=1` (WP3 notes item 1), so the clients methods answer `TEST_STATE`.
8. **Celebrations seen.** The bar widget keeps `seen` (agent_vectors.json
   `priority.celebrate.seen`) in memory and advances it when the popup closes after showing a
   celebration; a shown growth is acknowledged then with `pet.events_seen`. A shell restart
   may replay a celebration still inside its 30 s window.
9. **Memories** has no journal read method in the protocol: the page shows the living pet
   ("{name} came home.", from the snapshot's `adopted_at_ms`) and the ended pets from
   `pet.memorial`. The other `memories.*` keys stay unused until such a method exists.
10. **Connect card after Connect.** The title area shows the pet and the face line while the
    ready check types, then `act.ready` and `hint.open` for the first connected app; a
    failed row shows its reason, and Try again connects the failed apps again. Done and
    Close go Home; nothing is sent.
11. **Codex guide pictures** are drawn as small terminal sketches (no screenshots ship),
    one per page, with page dots and Next / Back.
12. **Card size.** The popup padding is `Style.space(12)` (the spec's 32 px top + bottom at
    base 16) and the height cap `min(Style.space(480), 90% of the output)`. K13 is measured
    against that cap in the PySide smokes (stub theme) and, under the real Omarchy Style and
    Color, in `test_bar_plugin.py` at base sizes 12, 16 and 20.
13. **Bar label** `status` (v0.2) reads as `name`; the manifest offers `name` and `none`.
14. **The owner decisions after wave 1** (section 7, branch `v0.3-fix`) need nothing more in
    the popup: Help shows `help.forget.body` and `help.privacy.body` from the contract, so the
    replacement text of section 7 item 1 appears once `strings.json` carries it (then run
    `tools/ui/gen_plugin_strings.py`; `test_bar_plugin.py` and CI fail while the plugin copy
    is stale); the cat's "Ball" and the new Claude Code tile come from the registry and the
    icon trees as they are.
