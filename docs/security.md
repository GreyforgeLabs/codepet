# Security status

This scaffold has no real execution surface: it cannot launch coding providers, tests, shells or commands from a task. It performs no project file reads/writes or network calls. This is deliberate fail-closed behavior, not proof that a future runner is secure.

Only `Provider::Mock` passes preflight. Real providers are unsupported independent of whether they are installed. A completed mock process enters verification, then review; a failed mock execution is terminal. Verification outcomes remain Passed, Failed or NotVerified; missing/failed checks require a bounded nonempty user override reason to accept. A review and explicit acknowledgement precede rewards. Duplicate acceptance returns the stored receipt without adding another reward.

Cancellation remains active until an explicit mock stopped confirmation. Restart modeling interrupts execution without relaunch; a review becomes RecoveryRequired and blocks new work. Production recovery must validate actual containment, journals and digests before releasing that slot.

Release blockers include: SQLite transactions and migrations; canonical path and secret checks; isolated staging; sandbox/cgroup process-tree containment; validated provider version adapters; bounded real IPC with same-user permissions; immutable content review digests; journaled safe apply/rollback; and adversarial integration tests. See implementation-spec.md for full requirements. Neither QML nor core mock token logic may substitute for these protections.
