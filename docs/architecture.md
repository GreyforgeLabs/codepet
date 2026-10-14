# Initial architecture

`codepet-core` is a pure in-memory reference model with no IO. Its public transition methods represent explicit user actions or trusted mock-runner reports. They are not a security boundary and must never be exposed directly to provider-authored input.

`codepet` offers help, an availability listing, and an explicitly requested mock demo. It has no provider spawn logic, daemon, file access, socket, or background worker. `std::process::exit` reports an unknown CLI argument only; it launches nothing.

`qml/codepet` is a standalone Qt Quick Window prototype. It intentionally duplicates a small state model for visual iteration and is not wired to Rust. Before production, replace this duplicate state authority with the Rust-owned command/event contract; do not reward in both layers.

Future Linux architecture: QML renders idle UI; explicit short Rust commands read/write state; one per-job Rust supervisor owns staging/containment/execution/review. A job-scoped event helper is optional where native sockets are unavailable and must exit with the job/UI. No permanently resident Rust daemon or idle polling. Omarchy/Quickshell integration is a host adapter, separate from the plain Qt standalone host.

The engine uses opaque integer model capabilities and immutable review records, not cryptographic content digests or production authentication. IDs, receipts and events only live in a single engine instance. `recover_snapshot` transforms an existing in-memory snapshot for testing; it is not file-based crash recovery. Bounded task history fails with Capacity instead of silently deleting records. A production store needs explicit retention/export policy.

Settings validate ranges; decay is not implemented. Paths are checked lexically; real filesystem containment and canonicalization are not implemented. Artifact acceptance is demonstrated for mock results in all care categories; real code-change tasks require safe apply and evidence before reward.
