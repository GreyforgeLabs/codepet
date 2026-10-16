# Security status (v0.2)

CodePet still has no real execution surface. It cannot launch coding providers, tests, shells or commands from a task. The mock supervisor spawns no process, opens no socket and never reads or writes the selected project folder. There are no network calls. This is deliberate fail-closed behavior, not proof that a future runner is secure.

## What is enforced now

- **Manual start only.** A task runs only after an explicit `task.start` with a one-use preflight token for the exact draft revision and settings revision. Tokens are 32 random bytes; only their SHA-256 is stored. They expire after 5 minutes, are superseded by a newer preflight, are invalidated by `task.update` and `settings.update`, as well as care/rest changes, and are refused if the clock reads earlier than their creation. Nothing starts from opening the UI, timers, recovery or provider output.
- **Unsupported providers fail closed.** Only `mock` passes preflight and start; installation of a real CLI enables nothing.
- **Care and rest are authoritative.** Full care, awake/rested state and bathroom readiness are rechecked in the same transaction as Start and the care debit. Care and sleep cannot start work or grant XP. Required rest cannot interrupt an active task.
- **One active slot.** A partial unique index allows one task in a slot-occupying state, across processes. Cancellation releases the slot only after the supervisor confirms the (mock) stop.
- **Durable, exactly-once rewards.** Acceptance requires the current review digest and an explicit acknowledgement; unverified or failed checks also need a non-empty override reason (at most 512 bytes). The isolated demo-XP reward, acceptance, state change and request result commit in one SQLite transaction. New acceptance never refills care. Failed simulated checks may be acknowledged for review but earn no XP; real-work XP remains zero. The request id is an idempotency key; a second acceptance returns the stored receipt. Failure, cancellation, discard and recovery never reward. Crash-injection tests cover aborts before and after commit.
- **Reviews are checked against their content.** The digest is recomputed from the stored artifact at `task.review`, at acceptance and at recovery. A mismatch is refused, and recovery marks the task `recovery_required`; it can then only be abandoned (no reward, no file changes).
- **State directory checks.** The directory must be owned by the effective UID with no group/other bits and must not be a symlink; it is created 0700 and the database 0600. Existing entries are never chmodded. Newer or corrupt databases are refused without modification.
- **Folder checks at preflight and start.** Lexical rules from core (absolute, no `..`, not `/`, `/proc`, `/sys`, `/dev`), plus refusal of the home directory (`$HOME` and the `/etc/passwd` entry) and of any folder overlapping the state directory, compared after resolving symlinks through the deepest existing ancestor. If no home directory can be determined, preflight refuses.
- **Bounded input and output.** 64 KiB frames, typed params with unknown fields rejected, bounded error messages, drafts capped at 32 KiB of JSON, artifacts at 24 KiB, bounded pages. Every response is tested to fit one frame. The UI renders provider-authored text as plain text.
- **Best-effort text privacy.** Newly captured instructions, artifact text and override reasons mask common credential patterns and strip terminal/bidirectional controls before persistence and review hashing. Unknown secrets can still escape detection; do not supply secrets. Existing history is not silently rewritten.
- **Fault injection is debug-only.** `CODEPET_FAULT` is ignored by release builds.

## Added in v0.2.0

- **Lifecycle endings never touch work.** An ending changes only the life tables, its
  journal, `meta`, the presence tables, the same transaction's meters and the care revision.
  Tasks, events, artifacts, checks, acceptances, rewards, receipts, settings and project
  folders are never changed (the lifecycle modules contain no `DELETE FROM`, checked by a
  test). Terminal lives are immutable by trigger. See [lifecycle.md](lifecycle.md).
- **Presence input is untrusted and can only under-charge.** `presence.json`, written by the
  bar plugin's leader, must be a regular file (never a symlink) owned by the effective UID,
  not group- or other-writable, at most 2048 bytes, with the same `(dev, inode)` from
  `lstat` and the opened handle. It is parsed strictly and validated by
  `codepet_core::presence`. Credit is capped by CLOCK_BOOTTIME elapsed since the previous
  settlement (at most 8 h), and an append-only ledger consumes each session's time once.
  A missing or refused file charges nothing; a refusal only increments
  `meta.presence_rejections` and the command still succeeds. Uncertain time never kills.
- **`bar-state.json` publication.** After every successful mutating `codepet rpc` request
  (including `state.get`, which may settle), after `codepet state`, and after each committed
  mock-supervisor step, the CLI calls `Store::export_bar_state()`. The export takes the
  database writer lock, builds a bar-safe subset (at most 16 KiB; no task instructions,
  folders, artifacts, logs or provider error text), creates a temporary file exclusively
  with mode 0600 without following symlinks, syncs it and renames it over
  `$XDG_STATE_HOME/codepet/bar-state.json` while the lock is held. It skips the write when
  the file already holds the same or a newer `state_version` of the same database
  (`db_id`), so the file only moves forward. Failed requests never publish. A failed export
  never changes the response or the exit code; it prints one bounded line,
  `codepet: bar-state.json not updated: <reason>` (control characters replaced, reason cut
  to 200 bytes), on stderr. Readers (bar plugin, overlay) treat the file as data and never
  launch Rust because it changed.
- **`codepet --version`** prints `codepet <version>` without opening or creating the state
  directory; the installer uses it to check the installed binary.
- **No new execution surface.** The bar plugin runs only `codepet rpc` and
  `codepet-overlay <verb>` with argv arrays (no shell) after an explicit click, the leader's
  single startup read, or the `overlayOnLoad` setting; it never sends a `task.*` method. The
  overlay launcher quotes every expansion and evaluates nothing.

## Changed in v0.2.1

- **Plain-text names on buttons.** The Adopt, Retire and Keep buttons show the companion's
  name as plain text, like every other label; a name can no longer be read as rich text.
- **Refused `codepet rpc` children are ended.** When the bar refuses a child's answer
  (larger than 64 KiB, not JSON or not the protocol envelope) or the child times out, it
  sends SIGTERM, then SIGKILL on the next 15 s watchdog firing if the child is still
  running, and starts no new request until that child has exited
  ([omarchy-plugin.md](omarchy-plugin.md)).

## Not established

- No sandbox, namespaces, seccomp, cgroups, process-tree containment, deadlines or disk quotas; none are needed yet because nothing is executed, and all are release blockers for real providers.
- No staging, scope canonicalization against the project, comprehensive secret detection, safe apply, backups or rollback.
- No same-UID peer-credential check: there is no socket. Any process running as the same user can run `codepet rpc` or edit the database. The preflight token and review digest prevent accidental or stale actions, not a compromised same-user process.
- Digests detect changes to the artifact that are not matched by a change to the stored digest column; someone able to rewrite both can forge a review. Integrity against the local user is out of scope.
- No adversarial fuzzing; tests are example-based.
- Presence is a convenience clock: a same-user process can write `presence.json` within the bounds above, and deleting it stops decay. `bar-state.json` is equally writable by the same user; it drives only display, never a request or a reward.
- Every `state.get` that settles, and every publication, briefly takes the database writer lock; the cost has not been measured.

Release blockers before any real provider: isolated staging, sandbox/cgroup containment, validated provider adapters, bounded logs and output, journaled apply/rollback, a job socket with peer checks, and adversarial integration tests. See [implementation-spec.md](implementation-spec.md). Neither the QML nor core's in-memory model may substitute for these protections.
