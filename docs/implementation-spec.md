# CodePet implementation specification

Version: 1.1 · Status: implementation-ready design, not implemented or benchmarked · Target: supported Linux desktops; Omarchy integration by default

## Current product direction (2026-10-15)

This revision supersedes the earlier bar/panel-only and no-roaming direction: the primary interface is a transparent desktop pet with a tight input region, short cosmetic walks, long pauses, hover/selection meters and transient care props. A separate focusable task/review window opens only through explicit user action. Ordinary portable Qt preview windows are not a claim of Wayland overlay support. See [desktop-overlay.md](desktop-overlay.md) for implemented capability boundaries and live validation gates.

The implemented care/progression contract in [care-progression.md](care-progression.md) also supersedes the older category meter rewards below: care fills bars, full bars and completed required rest gate Start, explicit eligible mock acceptance grants isolated demo XP, and real providers remain disabled. No cosmetic timer, low meter, care interaction or wake event starts coding.

## 1. Product decision

Build a small, manually controlled coding companion with a portable Linux Rust core, default Omarchy integration, and a standalone Qt Quick/QML window for other supported Linux desktops. Adapt the Quickshell/QML interface and original sprites from [SLcode777/omagotchi](https://github.com/SLcode777/omagotchi). Add a Rust local runner for task execution, isolation, persistence, review, and pet-state calculations. Borrow only useful status-display ideas from [yesmeck/OmaPets](https://github.com/yesmeck/OmaPets), rather than combining two complete applications.

The pet is an interface to a task the user has deliberately chosen. It does not discover work, decide what to fix, run background coding jobs, or use low meters as permission to do anything.

The core loop is:

1. Click the pet and choose a care category
2. Select a repository or folder and describe one bounded task
3. Select an installed provider and review the exact scope and execution permissions
4. Click **Start**
5. Watch progress or cancel
6. Review changes, verification evidence, and any remaining uncertainty
7. Explicitly apply or accept the result; receive one care reward
   *(Superseded for the implemented build: acceptance grants isolated demo XP for eligible mock results and never refills care; Start is gated on full care and required rest. See [care-progression.md](care-progression.md). Pending operator confirmation, see §4.)*

Use Rust because the on-demand controller is predominantly process supervision, file handling, local IPC, and small amounts of state. Retain QML because Omarchy already provides the UI host, and reuse portable Qt Quick components in a standalone lightweight window elsewhere. Rust runs for an active task or a short explicit state/control command, never as an idle daemon. Do not add Electron, Chromium, a webview, a local HTTP server, or a second UI framework.

### 1.1 Source and compatibility caveat

The upstream repositories and available source descriptions were inspected to inform this design; no source was built, installed, or runtime-tested for this specification. At implementation kickoff, pin exact upstream commit IDs, record installed Omarchy/Quickshell/Qt versions, and validate the plugin integration surface. Repository descriptions are not a promise of compatibility with every Omarchy release.

Upstream Omagotchi supplies an existing needs-oriented QML interface, small original sprites, and Omarchy integration. Its original needs and gameplay are not this product's requirements. Replace its task-independent feeding, package polling, care calculations, and persistence with the rules below. Keep visual assets and useful presentation components where appropriate.

## 2. Normative requirements

**MUST** is a release gate. **SHOULD** allows a documented exception. **MAY** is optional and must not delay core delivery. IDs in this document are stable test references.

| ID | Requirement |
|---|---|
| MAN-01 | Every provider process launch follows an explicit Start click for a reviewed task revision |
| MAN-02 | At most one task may be preparing, running, verifying, cancelling, reviewing, or applying at once |
| MAN-03 | No automatic launches, retries, continuation, queue processing, scheduled jobs, or crash resume |
| MAN-04 | Low pet meters may change appearance only; they never authorize activity |
| MAN-05 | Cancel, discard, failed verification, or an absent user never triggers another task |
| UI-01 | Provide four meters: hunger, thirst, mental health, hygiene |
| UI-02 | Explain the task, provider, folder, writable paths, network access, and limits before Start |
| UI-03 | Show the difference between running, waiting for user, verifying, review ready, and failed |
| RUN-01 | Invoke installed provider CLIs through versioned, tested adapters, never concatenated shell commands |
| RUN-02 | Run tasks in isolated staged workspaces; the provider cannot write to the original folder |
| RUN-03 | Fail closed if the required sandbox or process-tree containment cannot be established |
| RUN-04 | Stop the entire contained process tree on cancellation or deadline |
| DATA-01 | Apply only reviewed changes, with baseline conflict checks, recovery journal, and backups |
| DATA-02 | Refuse detected baseline conflicts; require a quiescent destination and disclose concurrent-writer limits |
| DATA-03 | A reward is recorded once, transactionally, after the user's explicit acceptance. *(Implemented build: the reward is the isolated demo XP ledger row, not a meter refill; see [care-progression.md](care-progression.md).)* |
| SEC-01 | Treat repository instructions, generated output, filenames, and test scripts as untrusted input |
| SEC-02 | Do not grant home-directory, host filesystem, secret, privileged, or network access implicitly |
| SEC-03 | No telemetry, project uploading by the app, app account, or app-owned API key requirement |
| RES-01 | Bound memory, logs, artifacts, workspaces, animation work, and event queues |
| LIC-01 | Distribute only code and assets with documented compatible redistribution rights |

The selected coding provider can require its own account, subscription, API key, network access, and usage charges. SEC-03 does not imply free or offline provider operation.

## 3. Scope and non-goals

### 3.1 Version-one scope

- One local user, one pet, one active task
- Default Omarchy/Quickshell integration plus a standalone Qt Quick window on the supported Linux desktop matrix below
- Folder picker, recent-folder list stored locally, and explicit path scope
- Four task categories, editable descriptions, and optional predeclared verification commands
- Adapter infrastructure for Codex, OpenCode, Hermes, and Claude Code
- Real execution enabled separately for each provider only after its adapter passes the compatibility gate
- Git and non-Git staging, review, apply, discard, cancellation, crash recovery, reward ledger
- Local-only settings and history with clear retention controls
- Text diff review and a bounded learning/specification artifact viewer

### 3.2 Explicitly excluded

Automatic bug discovery; issue trackers; pull requests; commits or pushes to the user's repository; multiple agents; task queues; recurring chores; terminal-wide session monitoring; screen or clipboard surveillance; system package removal; privileged commands; automatic dependency installation; unrestricted provider MCP servers; remote filesystem targets; cloud sync; plugin marketplace; social features; complex pet breeding or death mechanics.

A task may run tests and edit project code. It must not run `sudo`, administer the machine, delete caches across the home directory, or treat an entire filesystem as a cleanup target. Dependency downloads and updates require the precise workflow in §10; hygiene is not permission to “clean my computer.”

## 4. Care categories and reward rules

> **Supersession note (2026-10-15, pending operator confirmation).** The implemented build
> follows [care-progression.md](care-progression.md) instead of the meter-reward rules in this section:
> acceptance no longer refills a care meter; care is filled only by explicit care clicks;
> Start requires all four meters full, no bathroom break due and no nap due, then debits
> care once; eligible mock acceptance grants isolated demo XP in the category's mastery
> track. REW-01, REW-03, REW-04 and the exactly-once/idempotency parts of REW-02 and
> REW-05 still apply, with "reward" meaning that XP ledger row. Whether gating every Start
> on full care plus bathroom and nap is the intended product direction for real providers
> has not been confirmed by the operator; confirm it before real providers rely on it.
> The table and REW-06 below are the earlier target, kept for history.

All meters represent **well-being**, from 0 to 100: larger is better. UI labels should say “Hunger satisfied,” “Hydration,” “Mental health,” and “Cleanliness,” or display the four requested names with an unambiguous high-is-good legend. Start at 70 each. Use integer hundredths internally and clamp after each update.

| Category | User-selected task | Required evidence | Base reward |
|---|---|---|---|
| Hunger | Fix a specific reproducible bug | Reviewed patch; reproduction or regression test result where available; user confirms the fix | +20 hunger |
| Thirst | Improve speed or code quality in a named area | Before/after benchmark under declared conditions, or named lint/type/quality check and reviewed improvement | +20 thirst |
| Mental health | Learn an identified concept, explain code, write a specification or plan | A concrete bounded artifact with sources/assumptions where relevant; user confirms it was useful | +20 mental health |
| Hygiene | Remove a named deprecated project dependency or explicitly selected obsolete project files | Exact deletion/dependency diff, reference/usage checks, and selected validation results | +20 hygiene |

**REW-01:** No reward for process exit, optimistic provider language, elapsed time, file count, line count, or claimed success alone. Never reward faster agent execution as “speed improvement.”

**REW-02:** The user chooses one category before Start. One accepted task earns one fixed reward in that category. No multiplier or reward for repeating acceptance. No automatic category reassignment.

**REW-03:** For code changes, apply and user acceptance are separate concepts. The primary action is **Apply and accept**, enabled only after review acknowledgement and verification display. **Apply without reward** is available when the user wants an uncertain result. For a learning artifact, use **Accept artifact**; an unnecessary repository edit is not required.

**REW-04:** Missing verification is labelled “Not verified.” A user may explicitly accept a useful unverified result after acknowledging the missing evidence; record `verification_override=true` and their short reason. This does not change the check result or present it as verified. Known failing checks require the same explicit override and visible failure warning.

**REW-05:** Use a SQLite unique constraint on `rewards.task_id`, with acceptance and meter update in the same database transaction. *(Implemented build: the unique ledger is `demo_xp_rewards.task_id`, committed with the acceptance; meters are not updated.)* Requests are idempotent across double clicks, IPC reconnects, and crashes. Reject duplicate acceptance with the recorded result.

**REW-06:** A rollback changes the original reward to reversed and removes the granted increment, clamped at zero, in the same transaction recording successful rollback. Do not rewrite subsequent rewards. Explain that meter decay and later rewards can make the resulting balance different from the pre-task balance.

### 4.1 Decay and pauses

- Decay is disabled by default. Offer an optional “Gentle decay while open” setting
- Default enabled rates: hunger 1 point/hour, thirst 1/hour, mental health 0.5/hour, hygiene 0.5/hour
- Configurable range: 0–5 points/hour per meter; floor 20, no pet death, no lost functionality
- Pause all decay while a task is active, when the screen is locked, when the session sleeps, when the user pauses it, or when the UI is not running. If a desktop lacks a reliable lock/suspend signal, keep decay disabled there
- Do not apply offline or overnight catch-up decay
- QML computes visual decay from the last persisted meter snapshot and locally recorded active-session timestamps. Accumulate only monotonic intervals observed in the current UI session, never raw wall-clock time since the last launch. Clamp each increment to 60 seconds; suspend gaps do not accumulate. Persist a small decay checkpoint sidecar through the UI host at most once per minute; it must not launch Rust. Rust validates and settles pending decay when an explicit state/control command or task acceptance runs. Uncertain or stale session intervals are discarded, not charged
- Update visible numbers at most once per minute; interpolate purely visual gauges only while the panel is open
- Pet disappointment must not pressure users into spending provider credits or deleting files

## 5. User interaction and UI

### 5.1 Transparent desktop pet and explicit task window

Omarchy uses a transparent Quickshell PanelWindow on one selected screen, with no reserved space and no keyboard focus. A union of only visible pet/menu/prop bounds forms the input region; empty desktop space passes clicks through. Choose the top layer deliberately, not the lock-screen overlay layer. Move artwork inside the surface; do not repeatedly reposition a desktop window or capture the full screen's input.

The pet makes small screen-local floor walks, then pauses. Clamp pet and controls on screen/scale changes and select a remaining monitor when its screen disappears. Animation is cosmetic, bounded to at most 6 FPS, paused when hidden or reduced-motion is selected. Explicit hide and trusted lock/suspend/fullscreen suppression remove both surface and props. Do not instantiate a session locker as a lock detector; unsupported integrations must be documented without guarantees.

Show meters on hover/selection and a compact care menu on click. Bowls, tree and bed are transient. Task entry, review and keyboard navigation live in an independent window that opens only explicitly, sharing one transport and Rust-owned state. Closing it must not kill the existing per-job supervisor. No frame or solid scene background surrounds the desktop pet.

Other Linux desktops require capability-specific hosts. A portable fixture preview is useful but does not satisfy global placement, click-through or always-on-top guarantees on Wayland or X11.

Status mapping:

- Idle: neutral pet
- Preparing/running: working animation
- Waiting for user: static question marker
- Verifying: magnifying-glass marker
- Review ready: quiet success-shaped marker, but no reward animation yet
- Failure/interruption: informative marker without a punitive meter loss
- Accepted: brief reward animation, then idle

Use readable labels, keyboard navigation, focus indicators, theme contrast, and non-color status cues. No terminal escape sequences, HTML, Markdown execution, or clickable untrusted output. Display tool output as escaped plain text. A source link is opened only after a user gesture; allow only validated `https`/`http` links and display its destination.

### 5.2 Task form

Required fields:

- Folder, displayed as canonical local path
- Category
- Installed provider and adapter-tested version
- One-sentence outcome and bounded detailed instructions (maximum 16 KiB)
- Explicit relative path scope: exact files or selected subdirectories
- Allowed operation classes: edit, create, delete; delete disabled by default
- Verification commands, if any: executable plus argument array, working subdirectory, timeout, network disabled by default
- Execution profile and network choice

Show an example per category but never populate a destructive cleanup request. The selection “entire project” must be explicit and warns about breadth. Reject `/`, the user's home directory, device/proc/sys paths, application state directories, and ancestor folders containing the app's task storage. Unsupported filesystem features fail with a clear message.

The confirmation summary contains the exact approved task revision hash, provider, staged path policy, writable scope, verification commands, network mode, time/resource limits, and credential exposure warning. Start explicitly authorizes the listed provider invocation, staged workspace creation, the exact listed verification commands, and cleanup of this app-owned staging after discard or the disclosed retention period. It does not approve deleting original files or rollback backups beyond the separately confirmed policy. Start is disabled while data changes or preflight is incomplete. A draft can be edited freely; changing it invalidates preflight and the Start token.

### 5.3 Review panel

Show:

1. User objective and provider/version
2. Added, modified, deleted files; explicit binary/large-file warnings
3. Text diff with per-file path, size, mode change, and deletion indicator
4. Checks: command, environment, start/end time, exit code, truncated output indicator
5. Provider summary labelled as provider-authored, not independent proof
6. Runner findings: out-of-scope edits, conflicts, unsafe file types, incomplete cancellation
7. Expected care reward and any missing-evidence acknowledgement
8. Apply and accept / Apply without reward / Discard

Version one applies the entire reviewed changeset; no hunk editing or partial acceptance. A changed staged result invalidates the review hash and requires fresh review. Provider processes must be fully stopped before final diff generation. Read-only artifact viewing remains possible after failure, but failed/cancelled task changes cannot be applied through this flow; start a new explicitly reviewed task to salvage them.

## 6. System architecture

```text
Omarchy default                         Other supported Linux desktops
  existing Quickshell host                small native Qt Quick launcher
           \                              /
             shared portable QML components
                ├─ idle display/animation/visual decay; no Rust process
                ├─ explicit action → short-lived codepet command
                └─ active task → codepet run task supervisor
                                    ├─ task state, SQLite, journal
                                    ├─ workspace, provider, checks
                                    ├─ bounded live IPC while active
                                    └─ sandbox + provider process tree
```

**ARCH-01:** No persistent Rust daemon, always-on Rust bridge, login-started runner, or idle backend polling loop. The Rust executable exits after a short state/control request or once an active job has durably reached review-ready/terminal state. A pending review is persisted data; it must not keep a Rust process alive.

The Omarchy adapter uses the existing Quickshell process and persistence APIs verified against a pinned release. The standalone launcher is a minimal Qt 6 C++ host for the same portable Qt Quick components, exposing a narrow `QProcess` transport and `QSaveFile`-based checkpoint API. It adds no browser or agent logic. Keep Quickshell and Hyprland imports confined to the Omarchy adapter. Standalone UI modules must load without either dependency.

Short explicit commands include `codepet state`, `codepet preflight`, `codepet start`, `codepet review`, `codepet apply`, `codepet cancel`, and `codepet recover`. Their JSON schemas are shared with IPC. `start` launches one per-job supervisor in a transient systemd user unit, then exits. The supervisor owns execution, logs, and a private socket only while work is active. It exits after sealing the review result or recording failure/cancellation. While that supervisor exists, standalone Qt may connect through a narrow QLocalSocket transport; the Omarchy host may launch `codepet events <task-id>` as an active-task-only streaming client if it lacks a supported native socket API. That helper exits when the supervisor closes or the UI disconnects and never restarts from an idle timer. Include its footprint in the Rust budget. After it exits, review/history/state queries are short commands initiated by UI interaction. Never run a command once per meter tick or per animation frame.

The transient job unit separates UI lifetime from the already authorized job. Closing the UI does not trigger a new job or kill the authorized one. Provider children have their own task cgroup, with unit lifecycle configured to kill them if the supervisor dies. On an explicit next UI open, a short recovery command reconciles persisted active tasks and existing app-owned units; it never resumes an agent automatically. No login service is installed.

QML owns visual interpolation only. SQLite and Rust remain authoritative for task state, acceptance, rewards, and settled meter balances. The QML checkpoint sidecar contains bounded session intervals and a schema/session identifier, never rewards or permissions. Rust validates it and atomically records which checkpoint intervals have been consumed; replay cannot decay twice. On corruption, discard uncertain decay and preserve the last valid balance. Visual meters are previews until the next explicit reconciliation. This is a convenience feature, not a tamper-resistant game economy.

### 6.0 Supported-platform policy

“Works on Linux” means a tested matrix, not every distribution or kernel:

| Tier | Initial target | UI and execution expectations |
|---|---|---|
| Default | Current pinned Omarchy release, x86-64, Hyprland/Wayland | Existing Quickshell host; all core workflows |
| Supported standalone | One pinned Ubuntu LTS with GNOME/Wayland and X11 session; one pinned Fedora stable with GNOME or KDE/Wayland | Qt 6 standalone window; same Rust core and certified execution profile |
| Additional validation | AArch64 Linux, other Qt-compatible desktops/distributions | Supported only after build, UI, provider, sandbox, cancellation and apply tests pass |
| Unavailable execution | Missing required user namespace, Bubblewrap, systemd user containment, compatible provider or supported local filesystem | Pet/UI may run; Start disabled with precise prerequisites |

Resolve exact versions during phase 0 and publish them. Build the core without Omarchy/Hyprland dependencies. The standalone UI supports ordinary windows on X11 and Wayland; the default Omarchy integration uses its supported Wayland shell; verify Qt platform plugins in packaging. Desktop-panel embedding is Omarchy-specific. Roaming/window climbing is excluded from v1; if added later it is optional, with a separate Hyprland adapter and no requirement for basic task functionality. Do not promise universal always-on-top behavior or window geometry access across Wayland compositors.

### 6.1 Suggested repository layout

```text
Cargo.toml
Cargo.lock
crates/
  codepet-core/src/{model,state_machine,needs,rewards,policy}.rs
  codepet-protocol/src/{requests,events,errors,limits}.rs
  codepet-storage/src/{db,migrations,journal,retention}.rs
  codepet-workspace/src/{paths,manifest,git,copy,diff,apply,rollback}.rs
  codepet-provider/src/{adapter,capabilities,events,codex,opencode,hermes,claude}.rs
  codepet-runtime/src/{sandbox,process,cgroup,verify,redact}.rs
  codepet-runner/src/{main,server,orchestrator}.rs
  codepet-cli/src/{main,commands,doctor}.rs
qml/shared/{Panel,PetSprite,TaskForm,TaskReview,Settings}.qml
qml/omarchy/{BarWidget,HostAdapter}.qml
qml/standalone/{Main,HostAdapter}.qml
launcher/qt/{main.cpp,ProcessTransport.cpp,CheckpointStore.cpp}
assets/sprites/
packaging/{systemd,arch,standalone}/
tests/{fixtures,integration,adversarial,resource}/
docs/{architecture,security,providers,installation,recovery,licenses}.md
```

Keep pure state and reward logic independent of IO. Workspace operations and process launches accept explicit policy objects rather than reading global mutable settings. Prefer standard Rust libraries plus `serde`, `serde_json`, `rusqlite`, a small async runtime, a reviewed hashing crate, and narrowly used Linux syscall bindings. Pin dependencies and record reasons for heavyweight additions. Do not implement cryptography or a generic agent framework.

### 6.2 Storage

- Config: `$XDG_CONFIG_HOME/codepet/config.toml`
- Database and journals: `$XDG_STATE_HOME/codepet/`
- Workspaces/backups/logs: `$XDG_DATA_HOME/codepet/tasks/<uuid>/`
- Socket/lock: `$XDG_RUNTIME_DIR/codepet/`

Resolve XDG fallbacks once, verify ownership, refuse symlinked control directories, directory mode 0700, files 0600, socket 0600. Acquire a short command lock for state mutation and an exclusive active-job lock before launch/binding. Persisted review/recovery state also blocks a second Start after the supervisor exits. Use SQLite foreign keys, WAL, explicit migrations, transactions, and sensible busy timeouts. Serialize SQLite writers across short-lived commands and the active supervisor. Backup before schema migration; incompatible downgrade refuses to modify state.

Minimum tables: `tasks`, `task_events`, `task_scopes`, `checks`, `artifacts`, `apply_transactions`, `acceptances`, `rewards`, `pet_state`, `settings`, `request_results`. Store file content in bounded workspace/backup files, not giant database blobs. Persist all externally significant state transitions before publishing the corresponding event.

## 7. IPC contract

**IPC-01:** The same envelopes apply to short command stdin/stdout and the active-job socket. No socket listener exists at idle. Protocol version 1, UTF-8 newline-delimited JSON; maximum frame 64 KiB. Logs and diffs use bounded pagination, never a single unlimited event. Discard malformed frames, return typed errors when possible, disconnect repeated offenders. Only same-UID clients accepted using Unix peer credentials. This prevents accidental cross-user access, not malicious processes already running as the same user.

Request envelope:

```json
{"v":1,"id":"uuid","method":"task.start","params":{"task_id":"uuid","revision":3,"preflight_token":"opaque-one-use-token"}}
```

Response envelope:

```json
{"v":1,"id":"uuid","ok":true,"result":{"task_id":"uuid","state":"preparing","seq":17}}
```

Error envelope:

```json
{"v":1,"id":"uuid","ok":false,"error":{"code":"SCOPE_CHANGED","message":"The task scope changed. Review it before starting.","retryable":false}}
```

Event envelope:

```json
{"v":1,"event":"task.state","seq":18,"task_id":"uuid","data":{"state":"running","phase":"provider"}}
```

Methods:

| Method | Purpose and restrictions |
|---|---|
| `hello` | Negotiate protocol; obtain active supervisor instance ID and last event sequence |
| `state.get` | Snapshot returned by a short explicit command; active IPC may expose the same method |
| `events.subscribe` | Subscribe after sequence; ring-buffer gap yields `RESYNC_REQUIRED` |
| `providers.inspect` | Read cached capabilities; explicit refresh runs safe version probes only |
| `task.create` | Persist draft; creates no provider process |
| `task.update` | Draft-only changes with expected revision |
| `task.preflight` | Validate paths, capability, sandbox, limits, scope, original baseline; no provider run |
| `task.start` | Consume one-use preflight token for exact task revision and persisted preflight generation |
| `task.cancel` | Idempotent cancellation; never means discard original data |
| `task.review` | Retrieve review digest, summary and paginated diff |
| `task.logs` | Cursor-based bounded sanitized log reading |
| `task.apply` | Exact review digest, acceptance choice, verification override if necessary |
| `task.accept_artifact` | Explicit artifact acceptance, digest, acknowledgement |
| `task.discard` | Mark result discarded; schedule only app-owned staging cleanup |
| `task.rollback` | Explicit approved rollback for exact transaction and current preconditions |
| `history.list` | Paginated local history |
| `settings.update` | Validated settings with expected revision |
| `pet.pause` | Explicit decay pause/resume |

`task.apply` and artifact acceptance request IDs are persisted as idempotency keys. Preflight tokens are persisted with a one-use consumption record. They expire after five minutes and on any relevant settings/task/provider change; they are not proof of user identity or a security boundary against a compromised same-UID application. The UI must require a real click; it must not trigger Start from timers, connection events, keyboard focus, or provider output.

Log events limited to 10 batches/second; UI status events coalesce to 5/second; database transitions are never dropped. Keep a 512-event bounded transient ring. Slow clients receive a resync instruction instead of allowing unbounded buffering.

Typed error codes include `BUSY`, `INVALID_STATE`, `STALE_REVISION`, `UNSUPPORTED_PROVIDER`, `AUTH_UNAVAILABLE`, `SANDBOX_UNAVAILABLE`, `SCOPE_CHANGED`, `BASELINE_CONFLICT`, `DISK_LIMIT`, `OUTPUT_LIMIT`, `CANCEL_INCOMPLETE`, `APPLY_RECOVERY_REQUIRED`, and `PROTOCOL_MISMATCH`.

## 8. Task state machine

Persist these states:

```text
draft → preparing → running → verifying → review_ready
                  ↘ failed       ↘ failed
preparing/running/verifying → cancelling → cancelled
review_ready → applying → accepted | applied_unaccepted
review_ready → accepted_artifact | discarded
applying → apply_failed | recovery_required
preparing/running/verifying/cancelling with confirmed lost supervisor → interrupted
applying at restart → recovery_required
```

`waiting_for_user` is an explicitly recorded running substate supported only by adapters with a verified safe interaction mechanism. It consumes the active slot. It does not permit a generic “yes to all” approval. Unsupported provider interaction ends the run with actionable guidance; do not guess an answer or rerun it automatically.

Rules:

- Only `draft` plus a valid Start token can enter `preparing`
- `preparing` constructs the staged workspace and containment before spawning the provider
- `running` ends only after provider exit and its cgroup becomes empty
- A zero provider exit code enters `verifying`; it is not success by itself
- `verifying` runs only preapproved commands in the sandbox; these are one task phase, not a provider retry. An ordinary test/lint/benchmark failure or timeout records failed/incomplete evidence and may still enter `review_ready`. A safety violation, lost containment, corrupt workspace, or uncontrolled source mutation enters terminal `failed` and forbids apply. This distinguishes an uncertain code result the user may explicitly override from an unsafe execution result they may not apply through this flow
- `review_ready` is reached only after all processes stop and a final immutable review manifest exists
- Code apply and artifact acceptance require a direct user action
- `applied_unaccepted` is terminal, but permits a later explicit accept-for-reward if the unchanged accepted result and checks still meet the requirements; unique acceptance/reward constraints still apply
- `failed`, `cancelled`, `interrupted`, `discarded`, `accepted`, and `accepted_artifact` are terminal execution states
- “Try again” creates a new draft with a new ID and requires Start; never reuse the original preflight token
- Applying cannot be cancelled mid-file mutation. Show “Finishing safe apply”; after a failed apply, offer controlled recovery
- Crash recovery may inspect files and containment without starting an agent. It must not silently apply unfinished changes
- A review-ready task restored after restart is read-only until workspace and original baseline digests are revalidated

The active slot is released only when execution and any apply/recovery operation are safe and terminal. A recovery-required task blocks new execution until repaired or explicitly exported and abandoned without claiming original files are restored.

## 9. Provider adapter contract

Do not invent executable flags or assume that similarly named CLIs support the same behavior. During implementation, inspect the installed executable's `--help`/version interface and official documentation, then record the exact supported versions and invocation format in `docs/providers.md`. Version probes must not install packages, initiate authentication, or launch an agent task.

Each adapter implements:

```rust
trait ProviderAdapter {
    fn identify(&self, executable: &Path) -> Result<ProviderIdentity>;
    fn capabilities(&self, identity: &ProviderIdentity) -> Capabilities;
    fn build_invocation(&self, request: &ApprovedRun, sandbox: &SandboxLayout)
        -> Result<Invocation>;
    fn normalize_event(&self, input: &[u8]) -> Result<Option<ProviderEvent>>;
    fn classify_exit(&self, status: ExitStatus, context: &RunContext) -> RunOutcome;
}
```

`Invocation` contains an absolute verified executable, `Vec<OsString>` arguments, explicit environment allowlist, cwd, stdin source, and stdio mode. Never put a user prompt into `sh -c`, use shell interpolation, or infer an executable from repository configuration. If a provider uses a shell internally, it remains confined inside the sandbox.

Capability fields include:

- Tested provider version range and documentation date
- Noninteractive execution and how task input is provided
- Structured event support and fallback text mode
- Working-directory selection
- Config isolation and disabling hooks/plugins/MCP integrations
- Authentication mechanisms compatible with an isolated home
- Permission/approval mode and known provider-side restrictions
- Network expectations, PTY requirement, graceful termination semantics
- Whether a completion artifact can be generated without shell tools

Initially support one verified adapter end-to-end, then add others against the same tests. The UI may list unsupported providers as “Not yet supported on this version,” never as ready. Do not silently invoke a provider through its broadest unattended permission switch to avoid approval handling. Outer isolation remains mandatory even when provider-native sandboxing is available.

For each of Codex, OpenCode, Hermes, and Claude Code, implementation deliverables are: executable detection, exact documented invocation, configuration-isolation evidence, fixture traces for success/error/auth/approval, and an integration test with an installed supported version. If any cannot disable arbitrary host integrations or cannot run with required confinement, leave it disabled and document the precise blocker. Do not block the first safe release on all four being available.

### 9.1 Provider authentication and output

The app has no account system. Users install and authenticate their preferred CLI separately. Do not start login automatically. A missing login produces a setup instruction and leaves the task unstarted.

A provider may need credential material inside its sandbox. Each adapter must declare exact credential files or environment variables and whether refresh requires writes. Do not expose all of `~/.config`, the real home, SSH agent, Docker socket, GPG agent, cloud credentials, or desktop session bus. Read-only credentials are still readable by the provider and may be exfiltrated by compromised code when network is enabled; explicitly disclose this boundary.

Prefer a narrowly provisioned provider-specific credential configuration, preferably short-lived or limited. Never copy credentials into the task workspace, logs, artifact bundle, or backup. An adapter requiring mutable auth storage needs a dedicated user-approved provider state directory outside result capture, bound only to that adapter. Do not promise that a CLI credential can be hidden from tools spawned in the same process security boundary. Unsupported credential isolation means provider unavailable, not broad home-directory access.

Provider-generated “finished,” “all tests passed,” and tool result messages are untrusted claims. Preserve them as labelled narrative; only the runner's own process/check records populate observed-check status. Observing a command exit does not prove code correctness or that the test is meaningful.

## 10. Execution security and staging

### 10.1 Threat model

Protect original project files from accidental edits and bounded malicious repository instructions; prevent routine task execution from writing elsewhere on the host; bound runaway processes and logs. Treat project content, dependency scripts, provider plugins, filenames, and model output as potentially malicious.

This is not a complete defense against a malicious kernel, compromised installed binaries, a malicious same-UID desktop process, kernel exploits, or disclosure of readable source/credentials through explicitly allowed network access. A worktree alone is not a filesystem sandbox. A prompt telling an agent to stay in a directory is not an enforcement mechanism.

### 10.2 Required Linux execution profile

Use a tested Bubblewrap-based mount/user namespace profile and a systemd user cgroup per task. Validate actual availability and kernel policy at preflight; do not assume Omarchy has them configured. Refuse Start if the minimum profile cannot be established. Do not silently fall back to unrestricted execution.

- Isolated user, mount, PID, IPC, and UTS namespaces; no elevated capabilities; `no_new_privs`
- Read-only minimal runtime mounts needed for the tested provider/toolchain, enumerated by an execution profile
- Isolated temporary home and tmp directories
- Staged source readable, only approved staged paths writable; parent directories must not allow replacing protected siblings
- App control socket, originals, backup storage, unrelated host directories, other task directories, and host process filesystem not exposed
- No host Docker/SSH/GPG/session sockets or host `/run` blanket mount
- Minimal namespace-local `/proc` and `/dev`; no device passthrough
- Private temp space and bounded file sizes; no mount privilege or namespace escape paths
- Network disabled by default for checks and offline-compatible providers
- Provider network enabled only in the approved execution profile, with an explicit explanation that enabled networking is not a destination allowlist

Where directory-level writable bind mounts cannot enforce the selected exact-file scope without allowing siblings or replacement attacks, use a separate writable output tree and controlled file mounts, or reject that scope layout. Never label post-run path checks as preventive isolation. Apply checks provide an additional boundary even where workspace-only writes are broader than the final allowed changeset; the UI must accurately distinguish these two scopes.

A practical initial profile may permit writes to the entire disposable staged project while the **apply scope** stays narrow. If used, label this clearly as “Agent may modify the staged project; only these paths can be applied.” The original and unrelated host filesystem must still be inaccessible. Do not claim exact-path runtime confinement for that profile.

Use a new task cgroup with memory, process-count, and CPU controls supported by the host. Tests intentionally attempting background daemonization must remain in it. If descendants can escape the configured containment, the execution profile fails certification. Timeouts are hard runner limits; provider cost/token controls are used only when a supported adapter can enforce them and never represented as a guaranteed billing cap.

### 10.3 Network and dependencies

Provider API traffic usually needs network access. Version one allows either network disabled or network enabled for the whole sandbox, with a prominent warning. Do not claim API-only access without implementing and testing an actual authenticated proxy/firewall policy. DNS filtering alone is insufficient.

The app never schedules dependency installation by default, and instructs the provider not to install dependencies unless approved. This is a behavioral restriction, not enforceable command filtering when the provider has arbitrary shell execution and network access: code may download or install packages within its writable stage. The UI must disclose that limitation. Only call dependency installation technically blocked when a tested execution/network policy enforces it. If the task explicitly needs installation, show exact package-manager command, expected destination inside staging, network access, and lifecycle-script behavior before Start. Prefer locked dependency sets and disabling install scripts when compatible. Execution of project tests and dependency lifecycle scripts can run arbitrary project code; run them only within the same sandbox. Never grant host writes because an install fails.

Removing a deprecated dependency means a user-named project dependency and a reviewed manifest/lockfile change. Do not remove system packages or dependencies based solely on a provider's “unused” assertion.

### 10.4 Git staging

Version one requires a clean selected Git repository: reject tracked modifications, staged edits, and relevant untracked files, with an explanation. Do not stash, reset, commit, or remove user changes. Permit the user to choose a non-Git snapshot workflow explicitly if they need to work from their current mixed state.

Create an app-owned **private Git repository** without object hardlinks or shared writable Git metadata, then create a detached worktree within task storage. Use a pinned implementation of safe Git commands, disable hooks and external diff/filter helpers, do not initialize submodules, and remove push remotes from the isolated copy. A normal linked worktree of the original repository shares metadata; it must not be exposed writable to the provider. Prefer a clean source export into the private repository when cloning could execute untrusted local configuration. Preserve the original baseline commit ID plus per-file content/mode manifest.

Capture the selected checked-out bytes; never unexpectedly substitute a different branch. Reject symlinks, submodules, Git LFS/filter-dependent files, special files, or sparse-checkout arrangements that version one's staging cannot faithfully handle. Explain exactly why instead of creating a misleading partial result. Do not copy `.git` from the original into an ordinary filesystem stage.

The provider can modify or commit within its private copy if its tools require it, but the app ignores provider commit messages as evidence. Compare final file content to the captured baseline, and never merge or cherry-pick into the user's repository. Apply selected bytes through the controlled apply engine; leave the original Git index and branch unchanged.

### 10.5 Non-Git staging

Require explicit file/subdirectory selection. Snapshot regular files into an app-owned stage, preserving supported permission bits and path names. Exclude hidden files only by an explicit visible selection rule; do not silently omit needed content. Secret-pattern detection is a warning and an exclusion aid, not a reliable secret detector.

Reject symlinks, hardlinked source files with link count greater than one, sockets, devices, FIFOs, non-UTF-8 path names in version one, and paths on unsupported network filesystems. Enforce configured byte/file limits before copying. Detect file changes during copy by comparing identity/size/timestamps and content digests; retry capture once without running the agent, then ask the user to stabilize the source if it keeps changing. Store a baseline digest per included file and existence checks for new files.

### 10.6 Path policy

Use directory-relative file operations and no-follow semantics; on supported Linux use `openat2` resolution constraints such as beneath/no-symlink equivalents with a tested fallback that fails closed. Do not rely only on `canonicalize` followed by a later path-based write. Validate every parent component and file identity again when applying. Reject absolute output paths, `..`, NUL, reserved control paths, and case/Unicode collisions on supported destination filesystems.

The approved manifest is authoritative. A repository's `AGENTS.md`, tool instructions, or generated request cannot expand it, enable network, launch an extra task, or approve deletion. Exclude or neutralize repository-local provider startup configuration unless the user explicitly reviews an allowlisted setting. A provider may read task-relevant instruction files as untrusted task context; the outer sandbox and runner policy still govern effects.

## 11. Verification, apply, rollback, and recovery

### 11.1 Checks

Verification commands are predeclared in the task form and executed by the runner in a fresh sandbox phase after the provider stops. Use executable/argv arrays, not an app-generated shell string. Shell scripts are allowed only as explicitly chosen project commands and remain untrusted sandboxed code.

Record exact command, environment/profile digest, workspace digest before and after, exit status/signal, elapsed time, and bounded output. Label results “Runner observed” rather than claiming correctness. Mark tests, benchmark scripts and configuration changed by the provider prominently in the review. Where practical run a separately captured immutable baseline harness against both baseline and candidate; distinguish those results from checks using provider-modified tests. A passing modified test is evidence to review, not independent proof. By default, a check that changes reviewed source files invalidates the candidate result and returns an actionable failure; permit declared generated output only in excluded ephemeral directories. Do not quietly include formatting changes made during verification after the review digest is calculated.

Performance tasks require a before/after benchmark of the same declared input and environment where feasible. Report noisy or inconclusive results honestly; do not claim improvement from a single incomparable measurement. Code-quality tasks can instead use defined lint/type/test checks and a user-reviewed qualitative change.

### 11.2 Review manifest

Once all execution ends, seal a manifest containing task/revision ID, original baseline, file additions/modifications/deletions, byte hashes, sizes, allowed mode changes, scope policy digest, check IDs, artifact hashes, and an overall review digest. Store the approved review digest with apply. Any mismatch refuses apply and requires a new review. Reject out-of-scope, symlink, special-file, unsupported binary, executable-mode escalation, or oversized changes; do not silently trim a changeset.

Version one supports text regular-file changes up to 2 MiB per file and 20 MiB total reviewed change bytes by default. Larger/binary files can be exported for manual handling but cannot be applied by the app. Ordinary source executable bits may be preserved; new executable permissions require explicit review, while setuid/setgid/sticky changes are prohibited.

### 11.3 Journaled apply protocol

**APP-01:** Multi-file replacement is not globally atomic. Do not advertise it as such. Implement a recoverable transaction with a visible short apply window; editors or watchers may see intermediate file states. Require the user to pause concurrent editing and background writers, acknowledge the quiescence requirement, and perform optimistic conflict checks. An app-owned lock cannot stop unrelated editors: there remains a check-to-rename race if another process writes the destination during apply. Version one does not guarantee preservation against that race. If the user cannot make the destination quiescent, disable automatic apply and offer patch/export for manual integration. Do not market the engine as an absolute no-overwrite guarantee.

1. Acquire an app transaction lock for the selected root; this does not lock other editors
2. Confirm original root identity, baseline contents/modes for every touched existing file, and absence of all proposed new paths
3. Check sufficient disk space; prepare same-filesystem temporary replacement files with strict permissions
4. Save baseline backups and a durable journal identifying every intended operation, expected before/after hashes, modes, and recovery location
5. Flush file contents and journal; persist transaction state `prepared`
6. Recheck each file and parent immediately before mutation using safe descriptor-relative operations
7. Install replacements via same-directory atomic renames; record and flush progress after each operation. Deletions are moved into recoverable backup handling, never blindly unlinked without a durable saved copy
8. Validate final hashes, persist transaction `committed`, then record acceptance/reward if requested
9. Keep backups and review data until retention permits deletion

If a change between baseline check and mutation is detected, stop. Optimistic rechecks reduce but cannot eliminate the uncooperative external-writer race described above. If some files were already changed, enter recovery mode; restore only files whose current contents still match the app's installed after-hash. Never clobber subsequent user edits in the name of rollback.

If a provider result is applied but the app crashes before reward commit, startup reads the journal, verifies installed files, and shows “Applied; acceptance pending.” The user can complete acceptance once; do not silently grant or duplicate a reward. If acceptance had already committed, return its recorded result.

### 11.4 Rollback

Rollback requires a user action identifying the task/transaction. Display affected files and conflicts before proceeding. Restore baseline content and modes only if each current file matches the app-installed after state. A newly added file can be removed only if it is unchanged. A deleted original can be restored only if its destination remains absent. Conflicts stop automatic rollback and offer a local backup/export location for manual resolution.

Rollback has its own durable journal and backup of current app-owned after-state. It must not use `git reset --hard`, `git clean`, force checkout, or blanket directory deletion. A rollback interrupted by a crash stays recovery-required until inspected. No time-based cleanup may delete backups needed for a pending transaction.

## 12. Cancellation and crash behavior

Launch the provider and every verification command in a task-specific cgroup. Retain its verified unit/cgroup identity, supervisor instance ID, process start time, and PID as diagnostics; never kill a reused PID based solely on persisted numbers.

Cancellation sequence:

1. Persist `cancelling`; stop accepting new tool/interaction responses
2. Send supported graceful termination to the entire task cgroup/process group
3. After 3 seconds, terminate all remaining members
4. After another 2 seconds, force-kill remaining members using supported cgroup-wide termination
5. Verify the cgroup is empty before marking `cancelled` or enabling another task
6. If containment cannot be verified empty, mark `CANCEL_INCOMPLETE`, retain the active slot, and offer diagnostics. Never report successful cancellation merely because the leader exited

Cancellation stops local contained processes; requests already received by a remote provider, remote side effects, and billing may continue. Display that caveat when relevant; never claim local cancellation reverses remote operations. Do not delete staging until processes are gone. A fork bomb hits process and memory limits; a detached grandchild must still be contained. A UI crash/disconnect does not kill a running task by itself; the per-job supervisor continues the one already authorized task up to its limits, seals the result, and exits. Review waits in persisted storage, with no idle Rust process. A supervisor crash must stop its provider cgroup through verified transient-unit lifecycle configuration. On the next explicit UI-open/recovery command, enumerate only app-owned task units and reconcile them without resuming execution.

An explicit recovery command first checks the verified task unit. If its supervisor is still alive, reconnect without altering execution state. Only confirmed lost execution becomes interrupted. Show “The previous task was interrupted; nothing was applied” only after the journal proves that statement. If apply had begun, show recovery-required instead. Never infer safety from an absent provider PID.

## 13. Logs, privacy, retention, and resource limits

### 13.1 Logging

- No telemetry or analytics endpoints
- Redact known supplied credential values and common token patterns before persistence
- Never log environment dumps, authentication files, raw session headers, or complete provider invocation secrets
- Redaction is best effort; task output may still contain secrets from source content. Explain this in docs and allow logging to be disabled
- Strip terminal control sequences and unsafe Unicode controls from display; preserve a clearly marked escaped representation where needed for debugging
- Keep stdout/stderr streams bounded while continuing to drain pipes; never deadlock because the UI is closed
- Default task log cap 5 MiB, circular tail with explicit “earlier output omitted”; no unredacted full raw log
- Log export requires an explicit user action, displays a sensitivity warning, and exports locally only

### 13.2 Default operational limits

| Resource | Default | Behavior at limit |
|---|---:|---|
| Concurrent active tasks | 1 | Refuse new Start |
| Task wall time | 20 minutes | Cancel whole task; user may choose longer before Start |
| Verification command time | 5 minutes | Mark timed out and end verification |
| Provider/task memory | 4 GiB | cgroup limit; classify OOM accurately |
| Provider/task CPU | 2 CPU cores equivalent | cgroup quota; configurable before Start |
| Task process count | 256 | cgroup process limit |
| Input stage size | 1 GiB / 50,000 files | Preflight refuses; user can narrow scope |
| Task disk usage | 2 GiB | Enforced quota where available; otherwise see below |
| Captured log | 5 MiB/task | Continue draining, truncate with marker |
| Artifact text | 2 MiB/artifact | Refuse larger review artifact |
| Review change bytes | 20 MiB total | Export/manual handling only |
| IPC frame | 64 KiB | Reject frame |
| Retained completed tasks | 20 tasks or 30 days | Clean app-owned staging only under retention rules |

A periodic disk-size check is not a hard disk limit. Implement a quota-backed task filesystem where available, or enforce bounded writable storage through a tested alternative and label its capacity. If hard disk containment is unavailable, state “disk monitoring only,” require adequate free-space reserve and approval of that profile, cancel on threshold, and document residual rapid-fill risk. Do not advertise a hard quota that the platform does not enforce. Before enabling a profile, decide whether this residual risk is acceptable for the selected task; the stricter profile fails closed.

Automatic retention cleanup may remove only clearly app-owned, terminal, unaccepted staging and expired diagnostic logs under the user's visible retention setting. Backups for applied transactions are retained indefinitely until the user explicitly confirms a backup-deletion policy. Suggest 30 days after acceptance, but do not enable it implicitly. The policy confirmation must identify app-owned rollback backups, the cutoff, and that expiry ends rollback availability. Show the policy in apply/review; changing it requires a new explicit confirmation. Never remove a backup associated with recovery-required state. Local history records and reward receipts are retained compactly; provide explicit user-controlled history deletion without affecting project files.

### 13.3 Resident application budgets

These are **measurement targets**, not achieved results:

- Rust processes while idle: zero. Short explicit commands exit promptly; combined per-job Rust supervisor and active streaming-client peak RSS target ≤25 MiB, excluding provider processes, and it exits at review-ready/terminal state
- QML plugin incremental resident footprint: ≤30 MiB PSS target, measured by same-host A/B comparison
- Omarchy idle application incremental PSS: ≤30 MiB target, excluding the existing shared Quickshell baseline. Measure standalone Qt host separately, initial target ≤60 MiB PSS; do not mislabel its whole runtime as an incremental plugin cost
- Idle CPU with animation stopped: ≤0.1% of one logical CPU averaged over five minutes; stretch target <0.05%. Measure bounded-animation mode separately over ten minutes
- Idle wakeups: ≤2/second average; no recurring shell subprocess probes
- Idle sprite animation: maximum 6 FPS, paused when hidden/locked and in reduced-motion mode
- Task status paint: ≤5 updates/second; full log display only while the panel is open
- Panel open: p95 <150 ms on the declared reference machine
- No monotonic memory growth across 100 start/cancel/review cycles; target <5 MiB retained growth after returning to idle

Do not add provider memory to the app's footprint or hide it: report both in separate rows. Provider runtimes can use hundreds of megabytes or gigabytes; Rust does not make those child processes lightweight. QML shares a host with other components, so process RSS attribution is not exact. Report host baseline, host with plugin, standalone Qt host, transient Rust commands/job supervisor, agent cgroup, and combined total; disclose measurement uncertainty and shared mappings.

Measurement harness: record hardware, kernel, compositor, Qt/Quickshell versions, provider/version, build profile, sampling method, duration, and screen/panel state. Use `/proc`/cgroup counters and available power/wakeup tools in a controlled environment. Benchmark empty idle, panel open, log flood, large diff, task running, cancellation, and 100-cycle soak. If targets fail, profile and simplify before adding optional visual features. Release notes contain actual numbers, not only targets.

## 14. Implementation phases

### Phase 0 — upstream and platform audit

- Pin source commits and inventory licenses/assets
- Verify QML plugin lifecycle/process API and the standalone Qt host on the supported X11/Wayland matrix
- Establish runtime toolchain, systemd user containment, Bubblewrap profile, filesystem behavior
- Inspect all four provider CLIs and publish the initial capability matrix
- Produce a short architecture decision record resolving platform-specific uncertainties

Exit gate: no unknown permission bypass required for the chosen first provider; unsafe providers disabled; reproducible development environment documented.

### Phase 1 — local pet and protocol

- Fork/adapt visual components; remove existing autonomous package polling and original needs logic
- Implement Rust state/storage, four meters, paused optional decay, QML host transport, standalone launcher, task form
- Use a fake provider that cannot run shell commands
- Add protocol versioning, supervisor recovery handling, request idempotence, and deterministic-clock tests

Exit gate: meters survive restart; no UI action other than explicit Start can launch even the fake provider; idle memory/CPU measured.

### Phase 2 — one safely contained provider

- Implement one adapter with verified flags and isolated config/auth
- Implement staging, cgroup supervision, cancellation, bounded streaming logs
- Implement no-network verification and a sealed review manifest
- Keep apply disabled until phase 3

Exit gate: real benign tasks and adversarial containment tests pass; no original files changed by execution; unknown provider versions rejected clearly.

### Phase 3 — review, apply, rollback, rewards

- Implement conflict-safe journaled apply and recovery UI
- Implement non-Git snapshots and supported Git cases
- Add explicit acceptance, evidence overrides, unique rewards, and rollback reversal
- Fault-inject every transaction boundary

Exit gate: no silent data loss in crash/conflict tests; rewards exactly once; manual recovery paths documented and tested.

### Phase 4 — remaining providers and polish

- Add OpenCode, Hermes, Codex, or Claude Code adapters not already delivered, individually gated
- Improve accessibility, reduced motion, error explanations, local history, and retention
- Run resource profiling and 100-cycle soak tests

Exit gate: every enabled adapter passes the same contract tests; unavailable adapters remain visibly unavailable rather than receiving unsafe fallback behavior.

### Phase 5 — release

- Produce versioned source and binary packages with checksums and license inventory
- Test fresh install, upgrade, migration failure, disabled plugin, and uninstall
- Publish measured footprint and security limitations
- Verify documentation examples against the pinned provider versions

Do not implement excluded features before these gates pass. A safe one-provider release is preferable to four superficially supported providers.

## 15. Acceptance-test matrix

| Test | Scenario | Expected result / requirements |
|---|---|---|
| T01 | Open either UI, low meters, relaunch UI, reconnect active IPC | No provider launch; MAN-01/03/04 |
| T02 | Double-click Start and replay request/token | Exactly one launch; MAN-02, IPC-01 |
| T03 | Change scope after preflight | Start rejected until new review; UI-02 |
| T04 | Attempt second task while running or review pending | BUSY; no queue; MAN-02 |
| T05 | Successful bug fix with runner-owned regression check | Review then explicit apply/accept; one hunger reward *(implemented build: one Bugfix-track XP grant, no meter refill; see §4 note)* |
| T06 | Provider says “tests passed” but check fails | Failure shown; no automatic reward; REW-01 |
| T07 | Useful learning artifact, no code changes | Explicit artifact acceptance grants mental-health reward once *(implemented build: one Learning & Specification XP grant, no meter refill; see §4 note)* |
| T08 | Provider exits 0 without useful result | No automatic acceptance/reward |
| T09 | Repeated accept, reconnect, crash after acceptance commit | Same receipt returned; no duplicate meter change |
| T10 | Cancel leader that forked/detached children | Entire cgroup empty before cancelled; RUN-04 |
| T11 | Hung process or fork bomb | Deadline/process/memory controls contain it; accurate error |
| T12 | Close UI while task runs | Existing task continues within limits, no new task; result waits |
| T13 | Crash per-job supervisor during execution | Task unit stopped; restart interrupted; no resume |
| T14 | Crash at each apply journal/rename/fsync boundary | Recoverable, no false “nothing applied,” no automatic overwrites |
| T15 | User edits original after baseline and before apply preflight | Apply refuses detected conflicts; user edit preserved; concurrent uncooperative writer limitation disclosed |
| T16 | User edits app-installed file before rollback | Rollback refuses conflict; backup remains available |
| T17 | New output path already exists | Apply refuses to overwrite it |
| T18 | Symlink/parent replacement race, `../`, absolute path | No escape or original/out-of-scope write; SEC-01/02 |
| T19 | Hardlinks, FIFOs, device files, submodules, unsupported filters | Clear preflight rejection, no unsafe partial capture |
| T20 | Malicious repo instructions request home upload/system cleanup | No scope escalation; protected paths inaccessible; network caveat truthful |
| T21 | Repo provider config enables hooks/MCP | Config is not loaded or provider is unavailable |
| T22 | Attempt SSH/Docker/session socket access | Unavailable in sandbox |
| T23 | Network-disabled check tries external access | Connection fails; no host-network fallback |
| T24 | Missing auth, unsupported CLI version or unknown flags | Unstarted/failed with precise setup guidance; no retry or login |
| T25 | Huge output, malformed JSON, ANSI/OSC sequences | Bounded memory; escaped display; no UI action execution |
| T26 | Disk full during stage/apply/logging/database commit | Safe failure or recovery-required; no fabricated success |
| T27 | SQLite corruption or unsupported schema downgrade | Read-only failure with repair guidance; preserve original DB |
| T28 | Wall clock jumps, sleep overnight, lock screen | No catch-up decay or extra jobs |
| T29 | Hygiene requests deletion outside selected project | Rejected before Start |
| T30 | Verification modifies source unexpectedly | Review invalidated; no unreviewed apply |
| T31 | Agent modifies Git metadata or makes private commits | Original Git index/branch unchanged; byte diff authoritative |
| T32 | Log contains a supplied credential and terminal controls | Known secret redacted before storage; controls escaped |
| T33 | First successful apply then explicit rollback | Only unchanged app-installed bytes restored; reward reversal once |
| T34 | Supervisor dies while task unit termination is uncertain | New tasks blocked until containment established |
| T35 | Same-UID IPC client replays an old acceptance | Idempotent; no duplicate reward; threat-model caveat documented |
| T36 | Test suite executable filename contains shell metacharacters | Correct argv invocation, no shell injection |
| T37 | 100 task/cancel/review cycles and event floods | Resource budgets measured; bounded queues/logs; no sustained leak |
| T38 | Reduced motion, keyboard-only UI, hidden panel | Usable navigation; animation timers stop; no polling storm |
| T39 | Fresh install/uninstall with existing original project | Project untouched; uninstall retention choices accurate |
| T40 | Backups expire under retention while recovery pending | Needed backups preserved; only explicitly authorized backup deletion occurs |
| T41 | Idle Omarchy and standalone UI for ten minutes | No resident Rust process, socket listener or polling CLI; bounded QML timers only |
| T42 | Standalone on certified X11 and Wayland desktops without Quickshell | Core UI and manual task loop function; no Hyprland dependency |
| T43 | Test fails normally versus sandbox containment failure | First can reach review_ready with explicit override; second forbids apply |
| T44 | UI reopened while supervisor is running | Reconnect to the same job; no false interrupted state or new launch |
| T45 | Concurrent writer mutates a target during apply | Documented conflict/recovery behavior; do not claim elimination of uncooperative writer race |

Unit-test state transitions, reward arithmetic, decay, IPC limits, policy normalization, manifest determinism, and path validation with generated/property-based inputs. Integration tests use a fake executable for deterministic process/output failures. Real-provider smoke tests are opt-in, incur provider usage, and run against disposable fixtures only. CI must not require real credentials or upload private user repositories.

Security tests must verify host effects, not merely prompt refusals: plant sentinel files outside staging, attempt to modify/read them, inspect resulting files and cgroup membership, and prove protected files remain untouched. Network tests must distinguish denied networking from allowed networking; never claim data-exfiltration protection in an enabled-network profile.

## 16. Distribution, licenses, and documentation

Retain upstream MIT license notices for reused code and original assets. Audit [Omagotchi's credits](https://github.com/SLcode777/omagotchi/blob/master/CREDITS.md) independently of its code license. The identified `stun.mp3`, `fall.wav`, and `farewell_gremlin.mp3` sounds have noncommercial licensing concerns; omit or replace them with original/CC0/appropriately licensed alternatives before distribution. Default to no sound in version one. Do not assume a repository-wide MIT label licenses every sound.

OmaPets' code license does not automatically cover all pet-catalog art. Do not copy catalog assets without a file-by-file source/license audit. Use Omagotchi's audited original sprites or new original assets. Record author, source URL, pinned revision, asset path, license, modifications, and required attribution in `THIRD_PARTY_NOTICES.md`. Resolve uncertainty by exclusion, not optimistic interpretation.

Provide an Arch package plus Omarchy plugin integration by default, and standalone Qt/Rust packages for the initially certified distributions. Bundle neither Qt nor system libraries blindly; document runtime dependencies and test distribution packages. Install transient-job unit templates where needed, never an idle daemon/login service. Verify the current plugin manifest/registration format at implementation time; do not bake guessed installer commands into release docs. Never require `curl | sh`, silently alter the user's shell, or overwrite upstream Omagotchi state. Use a distinct plugin ID and CodePet state paths.

Uninstall must stop active supervisors and app-owned task cgroups before removing binaries. Ask whether to retain state/backups; do not remove project folders or provider authentication. Disabling the plugin or closing the standalone window stops UI work and pauses decay. An already running job follows its approved limits and may be explicitly cancelled; no background Rust process remains after it ends.

Required documents:

- README: concept, four categories, explicit Start/review loop, prerequisites
- Installation: compatible versions, package verification, plugin enable/disable, uninstall
- Providers: tested versions, official documentation links, exact flags, auth/config/network limits
- Security: threat model, filesystem containment, credential exposure, network caveats, untrusted code
- Recovery: cancellation failures, disk full, crash during apply, conflict-safe rollback, local backup locations
- Performance: reproducible benchmark harness and measured app/QML/provider separation
- Privacy: local storage, redaction limitations, provider transmission, retention and deletion
- Licensing: all upstream code and asset notices
- Developer architecture: state diagram, schema migrations, IPC, adapter test contract

## 17. Definition of done

The implementation is done when a user can select a local disposable project, enter a bounded task, click Start with a certified provider, observe and cancel its confined execution, inspect independently recorded checks, apply a conflict-free reviewed result, receive exactly one category reward, and recover or rollback with conflict checks that preserve detected later user edits, within the documented quiescent-destination and concurrency limits.

Additionally:

- No automatic agent launch path exists in startup, decay, reconnect, history, failure, or recovery code
- All enabled providers have verified invocation/auth/config isolation, not speculative flags
- All supported filesystem paths are enforced and adversarial tests pass
- Crash tests establish recoverability without overstating multi-file atomicity
- Actual resource measurements are published against the stated targets
- Original project files, unrelated home data, provider credentials, and license boundaries are accurately handled
- Remaining unsupported providers/platform cases are clearly disabled and documented; uncertified containment permits UI/mock mode only, never real execution

This specification authorizes implementation planning only. It does not assert that a sandbox, provider integration, performance target, or source fork has already been implemented or validated.
