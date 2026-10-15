# Security status (milestone 1)

CodePet still has no real execution surface. It cannot launch coding providers, tests, shells or commands from a task. The mock supervisor spawns no process, opens no socket and never reads or writes the selected project folder. There are no network calls. This is deliberate fail-closed behavior, not proof that a future runner is secure.

## What is enforced now

- **Manual start only.** A task runs only after an explicit `task.start` with a one-use preflight token for the exact draft revision and settings revision. Tokens are 32 random bytes; only their SHA-256 is stored. They expire after 5 minutes, are superseded by a newer preflight, are invalidated by `task.update` and `settings.update`, and are refused if the clock reads earlier than their creation. Nothing starts from opening the UI, timers, recovery or provider output.
- **Unsupported providers fail closed.** Only `mock` passes preflight and start; installation of a real CLI enables nothing.
- **One active slot.** A partial unique index allows one task in a slot-occupying state, across processes. Cancellation releases the slot only after the supervisor confirms the (mock) stop.
- **Durable, exactly-once rewards.** Acceptance requires the current review digest and an explicit acknowledgement; unverified or failed checks also need a non-empty override reason (at most 512 bytes). The reward, meter update, acceptance, state change and request result commit in one SQLite transaction. The request id is an idempotency key; a second acceptance returns the stored receipt. Failure, cancellation, discard and recovery never reward. Crash-injection tests cover aborts before and after commit.
- **Reviews are checked against their content.** The digest is recomputed from the stored artifact at `task.review`, at acceptance and at recovery. A mismatch is refused, and recovery marks the task `recovery_required`; it can then only be abandoned (no reward, no file changes).
- **State directory checks.** The directory must be owned by the effective UID with no group/other bits and must not be a symlink; it is created 0700 and the database 0600. Existing entries are never chmodded. Newer or corrupt databases are refused without modification.
- **Folder checks at preflight and start.** Lexical rules from core (absolute, no `..`, not `/`, `/proc`, `/sys`, `/dev`), plus refusal of the home directory (`$HOME` and the `/etc/passwd` entry) and of any folder overlapping the state directory, compared after resolving symlinks through the deepest existing ancestor. If no home directory can be determined, preflight refuses.
- **Bounded input and output.** 64 KiB frames, typed params with unknown fields rejected, bounded error messages, drafts capped at 32 KiB of JSON, artifacts at 24 KiB, bounded pages. Every response is tested to fit one frame. The UI renders provider-authored text as plain text.
- **Fault injection is debug-only.** `CODEPET_FAULT` is ignored by release builds.

## Not established

- No sandbox, namespaces, seccomp, cgroups, process-tree containment, deadlines or disk quotas; none are needed yet because nothing is executed, and all are release blockers for real providers.
- No staging, scope canonicalization against the project, secret detection, safe apply, backups or rollback.
- No same-UID peer-credential check: there is no socket. Any process running as the same user can run `codepet rpc` or edit the database. The preflight token and review digest prevent accidental or stale actions, not a compromised same-user process.
- Digests detect changes to the artifact that are not matched by a change to the stored digest column; someone able to rewrite both can forge a review. Integrity against the local user is out of scope.
- No adversarial fuzzing; tests are example-based.

Release blockers before any real provider: isolated staging, sandbox/cgroup containment, validated provider adapters, bounded logs and output, journaled apply/rollback, a job socket with peer checks, and adversarial integration tests. See [implementation-spec.md](implementation-spec.md). Neither the QML nor core's in-memory model may substitute for these protections.
