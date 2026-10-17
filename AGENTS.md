# CodePet development invariants

Read README.md and docs/implementation-spec.md before modifying behavior. The spec is a target, not evidence that security or runtime features exist. For v0.3 work, the frozen contracts are in docs/v0.3/contracts/ (start with its README.md and notes.md).

## Operator correction — 2026-10-17 (takes precedence over v0.3 product direction)

Latest correction: the pet should be a simple icon that automatically knows where
to find bugs in local repositories. The manual folder/task form is an advanced
fallback, not the primary interaction. Bounded metadata discovery and read-only
automatic scouting now precede an explicit Fix click; a repair still uses isolated
execution and explicit reviewed Apply. Scouting itself grants no XP or care.
The older blanket prohibition on automatic provider launches is superseded only
for this bounded scout path. No automatic fixes, application, retry or queue follows.

Read [docs/decisions/0003-care-driven-real-work.md](docs/decisions/0003-care-driven-real-work.md).
The operator delegated the game-loop design after rejecting the earlier care-arrow
interpretation. Useful accepted work feeds the pet: bug → food, research → water,
build → mood; each accepted job earns 20 XP once. Care buttons enter jobs; free
recovery care stays available. Mood and energy gate new work; food/water never need
to be full. Read docs/work.md. The new default-build work controller is independent
of the retired mock pipeline. Do not cite the earlier wording of decision 0003 as
an immutable operator requirement.

The v0.3 observe-only restriction below still governs externally launched coding
sessions. It does not prohibit CodePet-owned scouting or explicitly started repairs
under the scope, isolation, containment, review and acceptance rules below.
No falling meter, cosmetic timer or unrelated observer event authorizes a task.
Keep the restored numeric stats, desktop dragging and pinning.

## v0.3 invariants (SPEC-v0.3 §6.1)

CodePet observes local coding clients (Claude Code, Codex, OpenCode, Hermes, Grok CLI) and never runs them. These replace the v0.2 provider-launch rules.

- **O1 Observe only.** CodePet never launches, signals, blocks or answers a coding client, and never reads transcripts or project folders. Client config files are read only by `clients.status`, `doctor` and the Connect/Disconnect jobs, triggered by the user, and only for the named keys (Codex `[features]`/`[hooks.state]`, CodePet's entry in Claude's installed-plugins list and its own `enabledPlugins` entry, the settings key check). Hooks, the fold and exports never read them. They are written only by Connect/Disconnect, directly (CodePet-owned files, Codex `hooks.json`) or through the client's own CLI.
- **O2 Hooks never hurt the client.** Every path exits 0 with empty stdout and stderr. The hook never opens SQLite, never creates directories, takes only a non-blocking lock, makes no network call, and is bounded by the shim's 0.3 s drain and its own `alarm(1)`.
- **O3 No content is stored.** Only enums, counts, flags, HMAC'd keys and timestamps (SPEC §5.10). Never prompts, assistant text, tool names, inputs or outputs, paths, cwd, project names, transcript paths, error text, model names or approval commands.
- **X1** A turn earns at most one treat, enforced by `UNIQUE(session_key, turn_key)`.
- **X2** No treat for errors, cancels, session start, tool calls alone, test events, non-interactive sessions, or while Off.
- **X3** Earning is capped durably (1 per session per 30 s, 12 per rolling hour, 48 per rolling 24 h, jar of 10).
- **X4** XP is never time-based. Duration only qualifies a turn.
- **R (kept)** Reward only after explicit acceptance, exactly once. Tapping **Treats** is the acceptance.
- **E1** Agent events are display hints and treat sources only. The fold writes only agent tables, `client_hooks`, the agent fields of `meta` and the export. It runs no settlement, decay, revival or death, and never changes care, arming or life state.
- **E2 Off is total**, apart from the login shell Codex itself starts before the shim.
- **Kept from v0.2:** no resident daemon and no idle polling (the fold and the clients job are bounded, short-lived processes); one `BEGIN IMMEDIATE` per mutation; state-dir safety (0700/0600, no symlinks); request-id idempotency; lifecycle integrity triggers; presence can only under-charge; transaction order (settle → body → finalize → `state_version++` → commit) for UI and presence mutations; bar export rules.
- **Lock order:** `fold.lock` before the DB, everywhere.
- Only `codepet migrate` and `codepet rpc` `state.get` may migrate. The hook, the fold and the clients job never migrate.

## General

- Preserve original files and historical data. Fail closed on uncertainty or conflicts. Never delete a shim a client may still reference; tombstone it.
- Keep Omarchy host APIs outside the portable core and standalone QML UI.
- Keep untrusted text plain text, bound inputs/events/logs/artifacts, and avoid new dependencies without a reason.
- Run cargo fmt --all --check, cargo test --workspace --locked and cargo clippy --workspace --all-targets --locked -- -D warnings, each also with `--features legacy-tasks`, plus `python3 docs/v0.3/contracts/check_contracts.py`. Update tests for every state/policy change.
- State verification limits honestly. QML source review does not mean Qt/Omarchy tested; unmeasured resource goals (such as the hook latency budget) are not benchmarks.

## Retired task pipeline (`legacy-tasks`, retained for non-default compatibility)

The v0.2 mock task pipeline is unrouted in v0.3.0 and kept only behind the non-default `legacy-tasks` cargo feature for compatibility. While it exists it keeps its v0.2 rules: one task owns the active slot until containment confirms all children stopped; no host-project writes, shell interpolation, implicit network/secrets or privileged actions; reward only after explicit result acceptance in one atomic durable transaction; failed execution/containment is terminal. No new feature work may depend on it. Its historic data tables stay read-only and are never dropped.
