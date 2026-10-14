# CodePet development invariants

Read README.md and docs/implementation-spec.md before modifying behavior. The spec is a target, not evidence that security or runtime features exist.

- Every real provider launch must follow an explicit user Start for the exact reviewed draft revision. No startup launches, timers, meter-triggered work, retries, queues, crash resume or autonomous task discovery.
- One task owns the active slot through preparation, execution, waiting, verification, review, cancellation and recovery. Cancellation releases it only after containment confirms all children stopped.
- Real providers remain disabled until versioned adapters, filesystem staging, confinement, bounded output, durable state and review/apply gates pass. Never add a shell-command fallback.
- No host-project writes from provider execution. No shell interpolation, implicit network/secrets, privileged actions or unreviewed cleanup.
- Reward only after explicit result acceptance, exactly once. Never reward process exit, progress, failure, cancellation or discard. Production acceptance requires an atomic durable transaction; in-memory receipts do not satisfy that gate.
- Failed execution/containment is terminal; failed or missing checks on a safely completed result may be reviewed only with visible status and explicit override acknowledgement/reason.
- Preserve original files and historical data. Fail closed on uncertainty, conflicts, unsupported isolation, or stale review/preflight state.
- No resident Rust daemon in the target architecture. Explicit short commands and a per-job supervisor only. No idle IPC polling. Keep Omarchy host APIs outside the portable core and standalone QML UI.
- Keep untrusted text plain text, bound inputs/events/logs/artifacts, and avoid new dependencies without a reason.
- Run cargo fmt --all --check, cargo test --workspace --locked and cargo clippy --workspace --all-targets --locked -- -D warnings. Update tests for every state/policy change.
- State verification limits honestly. QML source review does not mean Qt/Omarchy tested; unmeasured resource goals are not benchmarks.
