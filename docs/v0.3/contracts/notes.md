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
    forbids `given_at_ms` going back to NULL. (`earned_at_ms` superseded by §7 item 1.)
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
    ball, the painted twin of `badge_ball`; the cat's yarn waits for a `careIcons` override,
    superseded by §7 item 3)
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
7. **Forget vs the earning caps (decided by the owner, see §7 item 1).** Note 11 made Forget
   zero `activity_turns.earned_at_ms`, which the caps ignore, so "Forget coding history" also
   reset the 30 s / 12 per hour / 48 per day caps (only the jar of 10 remained), against
   X3 "capped durably". Options were: keep an hour-coarsened earn time for rows earned in the
   last 24 h, or say in X3 and Help that Forget resets the caps. The owner chose the first.

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
   fold cleared it. Resolved (WP2 choice, accepted by the owner, §7 item 2): every interactive
   `turn_done` clears it, and the treat check of that turn sets it when the verdict is
   `jar_full`, including the one late-tool re-check (which the Rust fold did not record).
   New `sessions` vectors: `jar_full_describes_only_the_last_finished_turn` and
   `late_tool_recheck_on_a_full_jar_sets_jar_full`.

## 7. Owner decisions after wave 1 (2026-10-16)

Binding owner-delegated decisions, made after WP0, WP1, WP2 and WP4 merged (`v0.3` @
`d49dcc3`). They supersede the contract text they name; the contract edits listed at the end
of each item are made (in their own commit, with `check_contracts.py` passing).

1. **Forget keeps the caps durable** (resolves §5 item 7; amends note 11). "Forget coding
   history" still deletes `agent_events` and `agent_sessions` and sets a given row's
   `given_at_ms` to 0. For `earned_at_ms`:
   - an earn that can no longer count toward any cap (`earned_at_ms <= now - 24 h`; the daily
     window is half-open) becomes 0, as before;
   - a newer earn keeps `ceil_hour(earned_at_ms) + 1` (`codepet_core::agent::forgotten_earn_ms`,
     `FORGOTTEN_MARK_MS = 1`). Rounding **up** means the kept time is never earlier than the
     real earn, so the 30 s per-session, 12 per rolling hour and 48 per rolling 24 h caps can
     only get stricter (by at most one hour), never looser: Forget can no longer be used to
     reset them. No identity is added: the row's HMAC keys are unchanged (they are immutable,
     X1) and the time keeps only hour precision;
   - the `+ 1` ms marks the time as kept by Forget: retention's coarsening never produces it
     and an ordinary earn never stores it (`agent::ordinary_earn_ms` moves an earn at exactly
     `hh:00:00.001` 1 ms later, which only makes the caps stricter). A marked time is cleared
     (set to 0) once it leaves the 24 h window (`treats::expire_forgotten`) by whatever runs
     first, without waiting for a hook: the settlement prologue of every write, a `state.get`
     (which settles when one is due), and every fold's retention call, even inside the hourly
     gate (run before the hourly coarsening, which skips marked times). So the next CodePet
     use after that day (at most about 25 h after the earn, given the rounding hour) leaves
     nothing of the forgotten history except the jar and XP rows. If CodePet is not used at
     all, the times stay in the local database until it next runs.
   - Forget is idempotent: a kept time maps to itself.
   Tests: `codepet-core` `forget_keeps_a_marked_hour_coarsened_earn_time_for_one_day`;
   `codepet-storage` `forget_history_keeps_every_cap_durable`,
   `forget_history_clears_activity_but_keeps_treats_and_xp`,
   `retention_floors_ordinary_earn_times_and_keeps_them`,
   `forgotten_earn_times_go_after_the_day_without_a_fold`,
   `an_ordinary_earn_never_looks_kept_by_forget`; `codepet-core`
   `an_ordinary_earn_never_carries_the_forget_marker`.
   **Contract edit (done):** the `schema-v5.sql` comment on `activity_turns.earned_at_ms` (a
   comment only: the DDL is unchanged), the `agent.forget_history` semantics in
   `protocol.json`, and the Help copy in `strings.json` (adult pages, exempt from K1/K2, K8 not
   applied to `help.*.body`):
   - `help.forget.body`: "Forget coding history deletes CodePet's record of coding sessions and
     events and clears the times of earned treats. Treats from the last day keep a time rounded
     to the hour, so the treat limits still count; CodePet clears it the first time it runs
     after that day. Treats still in the jar and growth already earned stay."
   - `help.privacy.body`, last sentence: "Forget coding history deletes it, except the times of
     treats from the last day, rounded to the hour, which the treat limits need until that day
     is over."
2. **`finished_jar_full` describes only the last finished turn** (§6 item 3, WP2's choice) is
   accepted by the owner. No change.
3. **The cat's play word is "Ball"** (`careWords.play`), matching the shared painted `play`
   icon (a red striped ball, the twin of `badge_ball`, §2 item 34), so the bubble says
   "Wheee! Ball!". The cat's `careLabels.play` ("Yarn tuft", Help/CLI only) is unchanged.
   Changed: `qml/assets/pets/cat/manifest.json`, `qml/assets/pets/registry.json`, its generated
   mirror `qml/shared/PetRegistry.js` and the generator table in `tools/petart/roster.py`. This
   supersedes §2 item 34's "the cat's yarn waits for a `careIcons` override": no cat override
   is planned, and the care-floor `play` patch note in `PROVENANCE.md` says so.
   **Contract edit (done):** `icons.json` `care_words.examples.cat.play` "Yarn" -> "Ball"
   (an example; `check_contracts.py` does not read it, `qml/tests/test_pet_assets.py` pins
   the cat's word).
4. **A brand-neutral Claude Code tile.** The orange tile with a white four-point sparkle was
   too close to a vendor mark. `app_claude_code` is regenerated through the WP4 pipeline as a
   single-cell patch of the `apps` sheet (the other four tiles are unchanged): a rose-pink
   tile with a white light bulb, no star, sparkle or asterisk shape and no orange. It was drawn
   with a crop of the other four selected tiles as the style reference (`gen.sh` nonce and byte
   checks passed on attempt 1) and reviewed by eye at 256, 64 and 20 px on dark and light.
   `tools/petart/icon_sheets.json` pins the patch; provenance is in
   `qml/assets/icons/PROVENANCE.md` (both trees byte-identical, `icons.py --check` OK).
   **Contract edit (done):** `icons.json` `apps.glyphs.app_claude_code` "spark" -> "light
   bulb", and the `apps.rule` glyph list now says "light bulb" and forbids star, sparkle and
   asterisk shapes. SPEC §4.1 (outside the repository) still names "spark"; this item
   supersedes it.

## 9. WP6 decisions (overlay and shared state, 2026-10-16)

Numbered 9 so it does not collide with section 7 (owner decisions after wave 1, branch
`v0.3-fix`) and section 8 (WP5). Earlier WP6 drafts called this section 7; "WP6 notes
item N" means item N here. No frozen contract file, schema, fixture or vector changed.
These fill gaps the spec left open; consumers named below must follow them.

1. **How the overlay opens Home (§3.8 `openHome`).** Omarchy's `shell call <id> …` reaches
   panel/overlay plugins only, and a bar widget has one instance per monitor (a plugin
   `IpcHandler` would register its target once per bar). The overlay therefore runs
   `omarchy-shell -q shell summon greyforge.codepet`, detached:
   `$OMARCHY_PATH/bin/omarchy-shell`, or, without an absolute `OMARCHY_PATH`,
   `/usr/bin/env OMARCHY_PATH=/usr/share/omarchy omarchy-shell` (the PATH wrapper refuses
   to run without `OMARCHY_PATH`, silently under `-q`). Omarchy's `summon` calls the
   focused bar's widget through the bar-widget panel contract. **WP5:** `BarWidget.qml`
   exposes `open()` (opens the popup at Home, including the Off cell rule of §3.1),
   `close()` and a boolean `opened` (done on `v0.3-wp5` at 9b316f0). Home requests are
   coalesced: at most one Omarchy call per `OverlayFlow.HOME_COOLDOWN_MS` (1 s), however
   fast the pet is clicked or the verbs arrive; the hop and heart still answer every
   click (K5). `status.home_requests` counts the calls sent.
   **Merge condition (spec §10 step 6, decided here):** the v0.2 bar on `v0.3` has no
   `open()`, and the overlay exits while `overlayOnLoad` is missing or false (the v0.2
   default), so the "stub `panel` verb keeps the old bar menu working" condition cannot
   hold against the old bar: no stub can open a v0.2 popup from outside. WP6 and WP5
   therefore merge **back to back** (WP6, then WP5 at once; no install, tag, release or
   hand-off of the integration branch between them), and the stub condition is waived.
   The overlay keeps `panel` and the v0.2 window verbs as aliases of `openHome`.
2. **"Pet on my desktop" is `overlayOnLoad`** (the existing manifest setting, §7.2
   decision 5). The overlay reads `$HOME/.config/omarchy/shell.json` read-only, exactly the
   file the Omarchy shell reads and writes (`userConfigPath`, which ignores
   `XDG_CONFIG_HOME`). The bar entry's settings are found like `CodePetBar.ownSettings`.
   In a file the Omarchy shell would use (a JSON object with `version: 1`): `true` keeps
   the pet; `false`, a missing key (the manifest default) or no CodePet cell at all (the
   cell was removed in Omarchy's bar settings without the uninstall) is off. Only no usable
   file (none, unparsable, oversized, not `version: 1`) is unknown, and a standalone
   overlay keeps running. **WP5:** `CodePetBar.paths().shellConfig` must use the same path
   (`$HOME/.config/omarchy/shell.json`, not `XDG_CONFIG_HOME`), so the bar, the overlay
   and the shell agree. The bar side is `omarchy-plugin/DesktopPetFlow.js` (pure, tested):
   `afterStart` (Let's go! sets the switch and launches `show`), `setDesktopPet` (on:
   `show`; off: no verb, the overlay exits by itself), `launchOnLoad` (only with the switch
   on, CodePet on and an armed life) and `launchOnEnabled` (Off to On). **WP5** writes the
   returned settings with `updateEntryInline` and runs the returned verb.
3. **The overlay exits by itself** when the drawn export has `enabled: false` or the switch
   is off (`OverlayFlow.exitReason`, `Qt.quit()`); started while Off, it exits without
   running codepet (K9). Off exits at once, and so does the switch turning off after this
   process saw it on. A switch read as off before the process ever saw it on exits only
   after the off has held for `OverlayFlow.SETTLE_MS` (1.5 s, one single-shot timer):
   Omarchy's `persistShellConfig` writes through a non-blocking `FileView.setText`, so the
   `show` launched right after Let's go! can read the old `false` first (K7). The pet is
   not drawn while an exit is pending. WP5 therefore needs no write confirmation before
   launching. `status.exit_reason` names a pending exit (`off`, `desktop_pet_off`, or "").
4. **The overlay sends no RPC on its own.** It draws `bar-state.json` only. Its two explicit
   reads are one startup `state.get` when the file is missing or unreadable, and one per
   `refresh` verb. `CODEPET_OPEN` is ignored (the launcher sends its verb right after
   starting; honouring both opened Home twice). It never sends `pet.events_seen`, care or
   any mutation; the v0.3.0 care ring (§3.8, optional) is **not** implemented and slips
   to v0.3.1.
5. **Overlay IPC and the launcher.** `openHome` and `panel` open Home; the v0.2 window verbs
   `tasks`, `care`, `companion`, `life` are aliases while the v0.2 bar menu and launcher
   send them. `status` drops `window`/`view` and adds `enabled`, `desktop_pet`,
   `exit_reason`, `calm`, `face`, `overlay`, `drawn`, `home_requests`; `supervisor` is
   always `false` (the launcher's `quit` check needs the key).
   **WP3** (`packaging/omarchy/codepet-overlay`): add a `panel` verb. Before starting an
   overlay for `show`/`toggle`/`panel` (and the aliases), when `$XDG_STATE_HOME/codepet/off`
   exists (§7.3) print `codepet-overlay: CodePet is off` and exit 0 without starting it.
   After a start or a call, read `status`; when `exit_reason` is not empty print
   `codepet-overlay: the desktop pet is off` (or `CodePet is off` for `off`) and exit 0,
   never "did not answer within 3 s" or "<verb> failed". The window verbs can go once WP5
   stops sending them.
   **WP5:** the v0.2 bar menu entries (Care, Tasks, Companion, Life, Show/hide pet) are
   removed in the same merge that lands the v0.3 popup.
6. **Celebrate once on the desktop.** The overlay keeps its own `seen`
   (`finished_seq`, `hello_seq`, `test_at_ms`, from the face's new `celebrate` field) and
   plays a growth celebration once by dropping the unseen growth from its own face
   (`OverlayFace.withoutGrowth`); only Home acknowledges growth in Rust. A new overlay
   process seeds `seen` and the played growth from the first export it draws
   (`OverlayFace.seenAtStart`: the export's `finished_seq`, `hello_seq`, a done ready
   check's `at_ms` and the newest unseen growth), so a restart (Off to On, the switch off
   and on, a bar reload or a session restart) never replays a celebration. The cost: a
   celebration exported before the process started is not played on the desktop (the bar
   and Home still show it). Nothing is persisted, so no new state file exists.
7. **Calm mode reaches the overlay live.** Calm mode is the bar entry's `calm` setting in
   the same `shell.json` (WP5 notes section 8 item 6: default off, because Omarchy exposes
   no system reduced-motion signal, so "follows system" means the plugin default). The
   overlay reads it with the `FileView` it already has for `overlayOnLoad` and applies a
   change at once (no walking, bob or hop). `CODEPET_REDUCED_MOTION=1` from the launcher
   applies only when there is no usable file or no CodePet cell; a `setReducedMotion` IPC
   call holds until the setting next changes. **WP5:** nothing more is needed (keep
   writing `calm` with `updateEntryInline`; passing the env var stays harmless). If a
   system reduced-motion signal appears later, the bar resolves it into `calm`; the
   overlay reads only the setting.
8. **`petstate.js`** is generated by `tools/petstate/gen_petstate.py` into
   `qml/shared/` and `omarchy-plugin/` (identical bytes; `--check` fails when stale,
   including after any `agent_vectors.json` change). Beyond the vector keys, the face has
   `celebrate` ({kind, seq, at_ms} or null), and the module exports `emptyFace`,
   `appName` (client id to display name) and `needLevel`. Replayed through both copies by
   `qml/tests/test_petstate.mjs`. **WP5:** load `omarchy-plugin/petstate.js` instead of
   re-deriving the order in `CodePetBar.js`.
9. **Readers.** `qml/omarchy/BarState.js` accepts `codepet.bar.v1` and `codepet.bar.v2`.
   The v0.3 blocks (`enabled`, `agent`, `treats`, `clients`) are all present (required
   under v2, validated under v1 too) or all absent (a v0.2 v1 file). The care
   compatibility keys `ready_to_work`/`readiness_reasons` are optional, so v0.3.1 can drop
   them. Once `CodePetBar.js` (WP5) also accepts v2, Rust can switch the label
   (`bar_export.rs` `BAR_SCHEMA`).

## 10. WP3 decisions and amendments (integrations + packaging, 2026-10-16)

Numbered 10 at the merge: section 7 is the owner decisions after wave 1 (branch `v0.3-fix`),
section 8 is WP5 and section 9 is WP6. Earlier WP3 drafts and comments called this section 7;
"WP3 notes §7 item N" in older text means item N here.

No frozen file changed: `commands.json`, the shim, the six Codex trust hashes and every
schema stay as they are (the rendered Claude and Grok files, the stored commands and the
shim are tested byte for byte against them at the reference paths). These entries record
what WP3 had to decide; where they differ from the SPEC text, they win.

1. **`TEST_STATE` signal (new contract for WP5).** The CLI cannot tell a `stateHome` test
   override from a user's own `XDG_STATE_HOME`. The bar sets `CODEPET_TEST_STATE=1` in the
   child environment (and the desktop-pet launcher's) whenever it runs with a `stateHome`
   override (`CodePetBar.js` `paths().childEnv`/`launcherEnv`, tested);
   `clients.connect`, `clients.disconnect` and `clients.test` then answer `TEST_STATE`,
   `clients.status` runs, and no clients job is resumed or run in such a process.
   **Fail-closed backstop (review round 2):** the signal is not trusted alone. A Connect
   never repoints an installed CodePet shim whose `S` is another state dir that still
   exists, or whose `B` is another `codepet` that still exists (`install::shim_conflict`):
   it fails `blocked` with a detail naming the other folder or binary, and a ready check
   (`clients.test`) refuses to run through such a shim. A tombstone, a missing shim or one
   whose old paths are gone is repointed as before. `codepet connect --repoint <id>...` is
   the explicit override (item 16). `codepet doctor` shows the conflict
   (`shim.conflict`, "Connect is blocked: ...").
2. **Internal command `codepet clients-refresh`** (post-freeze entry in `protocol.json`
   `cli.added`; `check_contracts.py` now compares the commands `main.rs` routes with that
   list): the installer's step 7 (§6.6). It refreshes connected clients' wholly CodePet-owned files
   (Claude marketplace tree, OpenCode `codepet.js`, Hermes plugin dir, Grok `codepet.json`)
   only when the marker is present and the bytes differ, upgrades an older shim in place
   with its own `S`/`B`, never runs a client and never edits Codex's shared file.
3. **The inbox dir.** Nothing in the default build created `$STATE/inbox`, and the shim
   drains and exits without it. The clients job creates it (0700, owner and mode checked)
   at Connect and before every ready check.
4. **Versions.** Sources, file-only: the mise wrapper's `installs/<tool>/latest` (Claude
   Code, OpenCode), the resolved binary path (`releases/<v>-<target>` for Codex,
   `downloads/grok-<v>-<target>` for Grok), the `pyproject.toml` of the checkout the
   `hermes` wrapper runs (else `$HERMES_HOME/hermes-agent/pyproject.toml`). Binaries are
   also looked for in each client's usual dirs (`~/.grok/bin`, `~/.claude/local`,
   `~/.local/bin`, mise shims, ...) because the bar's `PATH` may be short. Tested minimums:
   Claude Code 2.1.294, Codex 0.162.0, OpenCode 1.19.0, Hermes 0.21.3, Grok 1.0.52. When no
   file gives a version, the Connect job asks `<binary> --version` under the CLI rules
   (never Hermes); when that gives nothing either, the Connect proceeds and records no
   `installed_version` (the ready check still proves the path).
5. **Claude Code day-0 gate: passed offline** (`tools/integrations/claude-day0-gate.sh`,
   capture `crates/codepet-cli/tests/fixtures/claude-cli/day0-gate-2.1.294.txt`): run in a
   network namespace with no network, a throwaway `HOME` and `CLAUDE_CONFIG_DIR`, no model
   call. `CLAUDE_CONFIG_DIR` is honoured. Both manifests pass `claude plugin validate`.
   `install --json` answers `outcome: ok` also when already installed; `uninstall` and
   `marketplace remove` answer `outcome: failed` with `failureCode` `not_installed` /
   `not_configured` when there is nothing to remove (treated as done). Claude rewrites the
   whole `settings.json` (key order changes) and leaves `extraKnownMarketplaces: {}` after
   the removal; the key check therefore compares parsed values, treats an absent map as
   `{}` and allows only CodePet's own entries (`codepet@codepet-local`, `codepet-local`)
   to appear, change or go. A connected, enabled plugin skips the CLI (no rewrite on a
   second Connect). It also keeps a copy under `plugins/cache/codepet-local/` but loads in
   place from CodePet's marketplace dir. **Turned off in `/plugin`** (review round 2):
   `install` answers `ok` ("already installed") but would leave the plugin off, so Connect
   requires `enabledPlugins."codepet@codepet-local": true` afterwards and otherwise fails
   `blocked` ("CodePet is turned off in Claude Code; type /plugin there and turn it on"),
   and status reads that one entry of `settings.json` (with the installed-plugins entry,
   the only Claude keys status reads; O1) and reports such a plugin as not connected.
   `claude plugin enable` was not captured by the gate, so CodePet does not run it. The
   validated sources are pinned (`tests/fixtures/claude-cli/validated-sources-2.1.294.sha256`,
   unit test `shipped_manifests_are_the_validated_ones`): changing a manifest fails the
   test until the gate is run again.
6. **Hermes.** The key check compares `hooks`, `hooks_auto_accept`, `approvals`,
   `command_allowlist`, `permissions` and `plugins` (where only `codepet` may move between
   `enabled` and `disabled`, compared as sets because Hermes rewrites them sorted, and a
   `plugins.entries.codepet` may appear) with a small block-YAML reader; a `config.yaml`
   it cannot read fails the Connect as `blocked` before the CLI runs. In 0.21.3
   `agent_loop_stopped` carries `session_key` and `platform` but no `session_id`
   (`tui_gateway/session_lifecycle.py`), so the plugin drops it and the stale rule covers
   the turn. The plugin is about 140 lines (the SPEC aimed for under 100) and declares its
   nine hooks in `provides_hooks`. Offline gate (`tools/integrations/hermes-offline-gate.sh`,
   network namespace, throwaway `HERMES_HOME`, capture
   `crates/codepet-cli/tests/fixtures/hermes-cli-gate-0.21.3.txt`): `plugins validate`
   passes, `plugins doctor codepet` reports "runtime discovery, manifest parsing, import,
   and registration passed" with 9 hooks, and `enable`/`disable` rewrite `config.yaml`
   (lists re-indented and sorted, `disabled: []`, a new top-level `_config_version` and
   appended comment blocks), which the key check accepts. A read-only probe parsed the
   owner's real `config.yaml` with the reader (only the result was printed). Review round
   2: Stop watching without CodePet's plugin dir does not need `config.yaml` (one the
   reader cannot parse is no failure), and the plugin's per-session maps (`_tools`,
   `_waiting`) are dropped with the session on failed or cancelled turns, on finalize and
   when the 256-entry platform map evicts it (bounded, §5.6). That change was tested
   offline (`integrations/hermes/test_plugin.py`); the Hermes offline gate was not re-run.
7. **Codex.** A CodePet group is one whose every handler runs `*/codepet-hook codex`. The
   tombstone is the group left byte-identical (still trusted, so Codex shows no review
   prompt; the `disconnected` flag makes the shim exit); a later pass removes it once it is
   last, and an event array CodePet emptied is removed. Each Connect/Disconnect checks on
   the edited tree that every foreign `[hooks.state]` position (literal and canonical path)
   keeps the same handler fingerprint (else `blocked`, nothing written), writes
   `hooks.json.codepet-orig` (once) and `hooks.json.codepet-last` (0600), builds and fsyncs
   the new file, and compares the file's bytes with what it read as the very last step
   before the rename (3 tries, then `blocked`), keeping the file's mode. Nothing is ever
   restored after the rename: bytes that differ from CodePet's then belong to a writer that
   came after (herdr, slopbrake, Codex) and are left alone; the caller's status check
   decides whether CodePet's lines are in place. A `hooks.json` that is a symlink (stow,
   chezmoi) is never replaced: a change fails `blocked` ("add the lines by hand or set
   CODEX_HOME to the real folder"); with nothing of CodePet's in it Stop watching is a
   no-op. A CodePet line with `enabled = false` in `/hooks` reports
   `needs_step: codex_hooks_off` ("! Turned off in Codex"), like `[features] hooks = false`.
   `codepet doctor` warns when other hooks follow CodePet lines (Stop watching would leave
   them as tombstones). A read-only probe recomputed the fingerprints of the owner's four
   existing trust entries from the real `hooks.json`/`config.toml`: 4 of 4 match.
8. **Shim path.** Stored commands carry the literal shim path, so it must be usable in a
   shell and a JSON string unquoted: `[A-Za-z0-9._/+:@,=-]` only, else Connect fails
   `blocked` (`commands.json` `path_rule` still governs `S` and `B` inside the shim).
9. **Status.** `connected` follows the files: the client's `disconnected` flag or a
   missing artifact means false, and so does a stale one (CodePet's marker with other
   bytes: "Tap to connect"; Connect repairs it). For Codex, all six lines trusted (or no
   lines) clears `needs_step`; otherwise status keeps it.
10. **Jobs.** One job at a time (`$STATE/jobs/.lock`). The journal records kind, clients,
    `B`, pid and start time, attempts, per-client `recorded` steps, results (with a short
    plain detail, never file content) and the ready check's outcome. `state.get` and every
    `clients.*` call resume a non-done journal whose process is dead; after 3 attempts the
    job rolls back (unrecorded clients `failed: blocked`; OpenCode and Grok files removed
    again). Finished journals are kept 7 days (at most 20). `clients.test` with no
    connected client answers `INVALID_STATE` ("no connected app to check"); a client that
    already has a running job answers `BUSY` (new storage error `JobRunning`).
    Review round 2: a journal without a pid counts as starting for 150 s (longer than the
    120 s lock wait, so a job queued behind another is never started twice). Journals are
    written only under `$STATE/jobs/.lock`: recovery takes it without waiting and leaves
    every unfinished journal (and its running mark) alone while a job holds it, so it can
    never rewrite a journal the running job just finished. A job ends itself after 5
    minutes (then resumed or rolled back like a killed one), and a client CLI's output is
    read with a deadline, so a helper that escaped the process group while holding stdout
    cannot hang a job. **Off (E2):** a Connect or test job found while Off is ended
    without running anything (unrecorded clients `failed: blocked`, "CodePet was off"; a
    running ready check is abandoned: `test_state` back to null, no `ready_check` failure,
    new storage call `cancel_ready_check`); a Connect turned Off midway skips its ready
    check. Stop watching still runs while Off. Every Connect also sweeps CodePet's stale
    staging dirs (`<data_dir>/staging/.<name>.partial-<pid>`) and its temporary names
    beside Codex's `hooks.json` (`.hooks.json.codepet-*`). Fault points
    `disconnect_after_flag` and `test_after_begin` join `connect_after_artifact`.
11. **Ready check.** Stored commands run with only `PATH` and `HOME` (shell clients through
    `/bin/sh -c`), bounded to 3 s each. `meta.test_at_ms` is set again when the check ends,
    for both outcomes, so the "{name} is ready!" window starts at the result. A client that
    arrives clears an earlier `ready_check` failure.
12. **Client CLI environment.** `PATH`, `HOME`, and when set `CLAUDE_CONFIG_DIR`,
    `CODEX_HOME`, `HERMES_HOME`, `GROK_HOME`, `MISE_DATA_DIR`, `XDG_CONFIG_HOME`,
    `XDG_DATA_HOME` (the mise wrappers and OpenCode need the last two), plus Claude Code's
    `DISABLE_AUTOUPDATER=1` and `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`.
13. **Installer (§6.6).** The installer never restarts the shell (the restart-shell
    lesson): on an update it asks the running shell to rescan, which reloads the swapped
    plugin, and prints the restart command (with the live `HYPRLAND_INSTANCE_SIGNATURE`)
    only when the shell cannot be reached. It keeps the replaced `codepet` binary as
    `~/.cache/codepet/codepet-backup-<old version>-<ts>` (pruned to the newest once the
    data is migrated); when `codepet migrate` fails it prints the exact commands that put
    the previous binary, plugin and `current` runtime link back (§6.6.7 needs no rebuild). The 0.3.x gate checks the binary before anything
    is replaced and refuses a tree whose version is not 0.3.x: **until WP7 bumps the
    workspace version (still 0.2.1), `install.sh` and `tools/release/package.sh` refuse this
    branch on purpose**; the installer tests install from a copy whose version is 0.3.0.
14. **`codepet doctor --json`** is `codepet.doctor.v1`: `version`, `state_dir`,
    `state_exists`, `enabled`, `helpers_counted`, `shim` (`path`, `state`
    current|outdated|tombstone|missing|foreign, `sha256`, `paths_match`), `inbox`
    (`waiting`, `dropped`, `overflow`), `fold_backoff`, `clients[]` (detection, row,
    `artifact`, the `disconnected` flag, Codex trust counts) and `codex_login_shell`
    (`shell`, `ms`, `silent`, `timed_out`; only when Codex is installed). It opens no
    missing state dir. Review round 2: `shim.conflict` (why a Connect would refuse to
    repoint the shim, else null), and the login shell runs in its own process group with
    both pipes drained (capped) and never waited on past the 5 s deadline.
15. **OpenCode `setup()`** is a no-op, as herdr's plugin; which loader 1.19.0 uses stays a
    WP7 check (§5.5).
16. **`codepet connect --repoint` and a no-op `codepet disconnect`** (review round 2,
    post-freeze `protocol.json` `cli` entry). `--repoint` (first, with named ids, never
    `--all`; the bar never sends it) lets that Connect repoint a shim that serves another
    existing state dir or binary (item 1). `codepet disconnect <id>` for an app that is
    not connected and has none of CodePet's files prints "<name>: CodePet is not connected
    there; nothing to stop", exits 0 and records nothing (no flag, no row), so uninstall
    (which runs it for all five apps) never reports or warns about apps CodePet never
    touched. The bar's Stop watching still always runs the job. Result lines use the
    job's own verb ("couldn't stop watching" for a Disconnect).
